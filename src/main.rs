//! Axum 0.7 Chat Server – 房間 / 在線名單 / Username 暱稱 / Query-Token
#![allow(clippy::let_and_return)]

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Extension, Json, Router,
};
use chrono::Utc;
use dotenvy::dotenv;
use futures::stream::StreamExt;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{sqlite::SqlitePoolOptions, FromRow, SqlitePool};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::Arc,
};
use tokio::sync::{broadcast, RwLock};
use tower_http::services::ServeDir;
use tracing::info;

/* ===== 型別定義 ===== */

type Tx = broadcast::Sender<String>;

#[derive(Default)]
struct RoomState {
    tx: Option<Tx>,
    users: Vec<(String, String)>, // (uid, username)
}
type RoomMap = Arc<RwLock<HashMap<String, RoomState>>>;

/* ===== JWT Claims ===== */
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: i64,
}

/* ===== main ===== */

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();
    tracing_subscriber::fmt().init();

    let db_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://chat.db".into());
    let jwt_secret =
        std::env::var("JWT_SECRET").unwrap_or_else(|_| "change_this_secret".into());

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;

    let rooms: RoomMap = Arc::new(RwLock::new(HashMap::new()));

    let app = Router::new()
        .nest_service("/", ServeDir::new("static").append_index_html_on_directories(true))
        .route("/api/register", post(register))
        .route("/api/login", post(login))
        .route("/ws/chat", get(ws_handler))
        .layer(Extension(pool))
        .layer(Extension(jwt_secret))
        .layer(Extension(rooms));

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    info!("Listening on {addr}");
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}

/* ===== REST ===== */

#[derive(Deserialize)]
struct AuthInput {
    username: String,
    password: String,
}
#[derive(Serialize)]
struct TokenJson {
    token: String,
    user_id: String,
}

#[derive(FromRow)]
struct UserRow {
    id: String,
    username: String,
    password: String,
}

async fn register(
    Extension(pool): Extension<SqlitePool>,
    Json(p): Json<AuthInput>,
) -> impl IntoResponse {
    let salt: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(16)
        .map(char::from)
        .collect();
    let hash = format!("{salt}${}", p.password);
    let uid = uuid::Uuid::new_v4().to_string();

    match sqlx::query!(
        "INSERT INTO users (id, username, password) VALUES (?, ?, ?)",
        uid,
        p.username,
        hash
    )
    .execute(&pool)
    .await
    {
        Ok(_) => StatusCode::CREATED.into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn login(
    Extension(pool): Extension<SqlitePool>,
    Extension(secret): Extension<String>,
    Json(p): Json<AuthInput>,
) -> impl IntoResponse {
    let row: Option<UserRow> =
        sqlx::query_as::<_, UserRow>(
            "SELECT id, username, password FROM users WHERE username = ?",
        )
        .bind(&p.username)
        .fetch_optional(&pool)
        .await
        .unwrap();

    if let Some(u) = row {
        if u.password.split('$').last() == Some(p.password.as_str()) {
            let claims = Claims {
                sub: u.id.clone(),
                exp: Utc::now().timestamp() + 24 * 3600,
            };
            let token =
                encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))
                    .unwrap();
            return Json(TokenJson {
                token,
                user_id: u.id,
            })
            .into_response();
        }
    }
    (StatusCode::UNAUTHORIZED, "invalid credentials").into_response()
}

/* ===== WebSocket ===== */

#[derive(Deserialize)]
struct WsQuery {
    room: Option<String>,
    token: String,
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(q): Query<WsQuery>,
    Extension(secret): Extension<String>,
    Extension(pool): Extension<SqlitePool>,
    Extension(rooms): Extension<RoomMap>,
) -> impl IntoResponse {
    // 驗證 JWT
    let uid = match verify_jwt(&q.token, &secret) {
        Ok(id) => id,
        Err(_) => return (StatusCode::UNAUTHORIZED, "bad token").into_response(),
    };

    // 取得 username
    let username: String = sqlx::query_scalar("SELECT username FROM users WHERE id = ?")
        .bind(&uid)
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|_| "??".into());

    let room = q.room.unwrap_or_else(|| "lobby".into());

    let tx = {
        let mut map = rooms.write().await;
        let state = map.entry(room.clone()).or_default();
        if state.tx.is_none() {
            let (s, _) = broadcast::channel::<String>(100);
            state.tx = Some(s);
        }
        state.tx.as_ref().unwrap().clone()
    };

    ws.on_upgrade(move |sock| user_ws(sock, uid, username, room, tx, rooms))
}

fn verify_jwt(token: &str, secret: &str) -> Result<String, ()> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )
    .map_err(|_| ())?;
    Ok(data.claims.sub)
}

async fn user_ws(
    mut socket: WebSocket,
    uid: String,
    name: String,
    room: String,
    tx: Tx,
    rooms: RoomMap,
) {
    let mut rx = tx.subscribe();

    // 加入線上名單並廣播
    {
        let mut map = rooms.write().await;
        let state = map.get_mut(&room).unwrap();
        state.users.push((uid.clone(), name.clone()));
        broadcast_users(&tx, &state.users);
    }

    // 主迴圈
    loop {
        tokio::select! {
            Some(Ok(Message::Text(txt))) = socket.next() => {
                let _ = tx.send(json!({"type":"chat","name":name,"text":txt}).to_string());
            }
            Ok(m) = rx.recv() => {
                if socket.send(Message::Text(m)).await.is_err(){ break; }
            }
        }
    }

    // 離線
    {
        let mut map = rooms.write().await;
        let state = map.get_mut(&room).unwrap();
        state.users.retain(|(id, _)| id != &uid);
        broadcast_users(&tx, &state.users);
    }
}

fn broadcast_users(tx: &Tx, list:&[(String,String)]) {
    let names: Vec<_> = list.iter().map(|(_,n)| n.clone()).collect();
    let _ = tx.send(json!({"type":"users","list":names}).to_string());
}
