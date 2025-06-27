//! Axum 0.7 Chat Server — Rooms / History60 / Media Upload / Online List
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
use axum_extra::extract::Multipart as AxumMultipart;   // ← 唯一的 Multipart 來源
use chrono::Utc;
use dotenvy::dotenv;
use futures::stream::StreamExt;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use mime_guess;
use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{sqlite::SqlitePoolOptions, FromRow, SqlitePool};
use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
};
use tokio::sync::{broadcast, RwLock};
use tower_http::services::ServeDir;
use tracing::info;

/* ---------- 型別 ---------- */
type Tx = broadcast::Sender<String>;

#[derive(Default)]
struct RoomState {
    tx: Option<Tx>,
    users:   Vec<(String, String)>, // (uid, username)
    history: VecDeque<String>,      // 最近 60 則訊息
}
type RoomMap = Arc<RwLock<HashMap<String, RoomState>>>;

/* ---------- JWT ---------- */
#[derive(Debug, Serialize, Deserialize)]
struct Claims { sub:String, exp:i64 }

/* ---------- main ---------- */
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();
    tracing_subscriber::fmt().init();

    let db  = std::env::var("DATABASE_URL").unwrap_or("sqlite://chat.db".into());
    let key = std::env::var("JWT_SECRET").unwrap_or("change_this_secret".into());

    let pool  = SqlitePoolOptions::new().connect(&db).await?;
    let rooms: RoomMap = Arc::new(RwLock::new(HashMap::new()));   // 明確型別

    let app = Router::new()
        .nest_service("/", ServeDir::new("static").append_index_html_on_directories(true))
        .route("/api/register", post(register))
        .route("/api/login",    post(login))
        .route("/api/upload",   post(upload_file))
        .route("/ws/chat",      get(ws_handler))
        .layer(Extension(pool))
        .layer(Extension(key))
        .layer(Extension(rooms));

    let addr = SocketAddr::from(([0,0,0,0],3000));
    info!("Listening on {addr}");
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}

/* ---------- 註冊 / 登入 ---------- */
#[derive(Deserialize)] struct AuthInput { username:String, password:String }
#[derive(Serialize)]  struct TokenJson  { token:String, user_id:String }
#[derive(FromRow)]   struct UserRow    { id:String, username:String, password:String }

async fn register(
    Extension(pool):Extension<SqlitePool>,
    Json(p):Json<AuthInput>
)->impl IntoResponse{
    let salt:String = rand::thread_rng().sample_iter(&Alphanumeric).take(16).map(char::from).collect();
    let hash=format!("{salt}${}",p.password);
    let uid =uuid::Uuid::new_v4().to_string();
    match sqlx::query!("INSERT INTO users (id,username,password) VALUES (?,?,?)",uid,p.username,hash)
        .execute(&pool).await {
        Ok(_) => StatusCode::CREATED.into_response(),
        Err(e)=> (StatusCode::BAD_REQUEST,e.to_string()).into_response()
    }
}

async fn login(
    Extension(pool):Extension<SqlitePool>,
    Extension(secret):Extension<String>,
    Json(p):Json<AuthInput>
)->impl IntoResponse{
    let row:Option<UserRow>=sqlx::query_as("SELECT id,username,password FROM users WHERE username=?")
        .bind(&p.username).fetch_optional(&pool).await.unwrap();
    if let Some(u)=row{
        if u.password.split('$').last()==Some(p.password.as_str()){
            let claims=Claims{sub:u.id.clone(),exp:Utc::now().timestamp()+86400};
            let token =encode(&Header::default(),&claims,&EncodingKey::from_secret(secret.as_bytes())).unwrap();
            return Json(TokenJson{token,user_id:u.id}).into_response();
        }
    }
    (StatusCode::UNAUTHORIZED,"invalid creds").into_response()
}

/* ---------- WebSocket ---------- */
#[derive(Deserialize)] struct WsQuery { room:Option<String>, token:String }

async fn ws_handler(
    ws:WebSocketUpgrade,
    Query(q):Query<WsQuery>,
    Extension(secret):Extension<String>,
    Extension(pool):Extension<SqlitePool>,
    Extension(rooms):Extension<RoomMap>,
)->impl IntoResponse{
    let uid=verify_jwt(&q.token,&secret).unwrap_or_default();
    if uid.is_empty(){return (StatusCode::UNAUTHORIZED,"bad token").into_response()}

    let name:String=sqlx::query_scalar("SELECT username FROM users WHERE id=?")
        .bind(&uid).fetch_one(&pool).await.unwrap_or("??".into());

    let room=q.room.unwrap_or("lobby".into());
    let tx={
        let mut map=rooms.write().await;
        let st=map.entry(room.clone()).or_default();
        if st.tx.is_none(){ let (s,_) = broadcast::channel::<String>(100); st.tx=Some(s); }
        st.tx.as_ref().unwrap().clone()
    };
    ws.on_upgrade(move|sock|user_ws(sock,uid,name,room,tx,rooms))
}

fn verify_jwt(t:&str, secret:&str)->Result<String,()>{
    decode::<Claims>(t,&DecodingKey::from_secret(secret.as_bytes()),&Validation::new(Algorithm::HS256))
        .map(|d|d.claims.sub).map_err(|_|())
}

async fn user_ws(mut socket:WebSocket, uid:String, name:String, room:String, tx:Tx, rooms:RoomMap){
    let mut rx=tx.subscribe();
    { // 進房
        let mut map=rooms.write().await;
        let st=map.get_mut(&room).unwrap();
        st.users.push((uid.clone(),name.clone()));
        broadcast_users(&tx,&st.users);
    }
    { // 歷史
        let map=rooms.read().await;
        if let Some(st)=map.get(&room){
            for m in &st.history { let _ = socket.send(Message::Text(m.clone())).await; }
        }
    }
    loop{
        tokio::select!{
            Some(Ok(Message::Text(raw))) = socket.next() => {
                let msg = build_msg(&raw,&name);
                {   // 存歷史
                    let mut map=rooms.write().await;
                    let st=map.get_mut(&room).unwrap();
                    st.history.push_back(msg.clone());
                    if st.history.len()>60 { st.history.pop_front(); }
                }
                let _ = tx.send(msg);
            }
            Ok(m) = rx.recv() => {
                if socket.send(Message::Text(m)).await.is_err(){ break; }
            }
        }
    }
    { // 離房
        let mut map=rooms.write().await;
        let st=map.get_mut(&room).unwrap();
        st.users.retain(|(id,_)|id!=&uid);
        broadcast_users(&tx,&st.users);
    }
}

fn build_msg(raw:&str, sender:&str)->String{
    match serde_json::from_str::<Value>(raw){
        Ok(mut v)=>{ v["name"]=json!(sender); v.to_string() }
        Err(_)   => json!({"type":"text","name":sender,"text":raw}).to_string()
    }
}

fn broadcast_users(tx:&Tx,list:&[(String,String)]){
    let names:Vec<_>=list.iter().map(|(_,n)|n.clone()).collect();
    let _ = tx.send(json!({"type":"users","list":names}).to_string());
}

/* ---------- /api/upload ---------- */
async fn upload_file(mut mp:AxumMultipart)->Result<Json<Value>,(StatusCode,String)>{
    while let Some(field)=mp.next_field().await.map_err(|e|(StatusCode::BAD_REQUEST,e.to_string()))?{
        let mime:String=field.content_type().unwrap_or("application/octet-stream").to_owned();
        let ext=mime_guess::get_mime_extensions_str(&mime).and_then(|a|a.first().copied()).unwrap_or("bin");

        let fname=format!("uploads/{}.{}",uuid::Uuid::new_v4(),ext);
        let full =PathBuf::from("static").join(&fname);
        tokio::fs::create_dir_all(full.parent().unwrap()).await.ok();

        let bytes=field.bytes().await.map_err(|e|(StatusCode::BAD_REQUEST,e.to_string()))?;
        tokio::fs::write(&full,bytes).await.map_err(|e|(StatusCode::INTERNAL_SERVER_ERROR,e.to_string()))?;

        return Ok(json!({"url":format!("/{fname}"),"mime":mime}).into())
    }
    Err((StatusCode::BAD_REQUEST,"no file".into()))
}
