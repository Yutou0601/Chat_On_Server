//! routes/ws.rs ── WebSocket 進房 / 離房 / 廣播

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Extension, Query,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::stream::{StreamExt, TryStreamExt};    // try_next / recv
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::SqlitePool;                                  // ← 取 username 會用到

use crate::{
    error::{bad, AppResult},
    state::{RoomMap, Tx},
    utils::jwt,
};

/* ─────────────── Query 物件 ─────────────── */
#[derive(Deserialize)]
struct WsQuery {
    room:  Option<String>,
    token: String,
}

/* ─────────────── 路由註冊 ─────────────── */
pub fn router() -> Router {
    Router::new().route("/chat", get(ws_handler))
}

/* ─────────────── WebSocket 入口 ─────────────── */
async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(q): Query<WsQuery>,
    Extension(secret): Extension<String>,
    Extension(pool): Extension<SqlitePool>,   // ★ 多帶一個連線池
    Extension(rooms): Extension<RoomMap>,
) -> AppResult<impl IntoResponse> {
    /* 1. 驗證 JWT → 取得 uid --------------------------------------------- */
    let uid = jwt::verify(&q.token, &secret).ok_or(bad("bad token"))?;

    /* 2. 資料庫撈 username ------------------------------------------------ */
    let name = sqlx::query_scalar::<_, String>("SELECT username FROM users WHERE id = ?")
        .bind(&uid)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| bad("user not found"))?;

    /* 3. 取得房間的 broadcast Sender -------------------------------------- */
    let room = q.room.unwrap_or_else(|| "lobby".into());
    let tx = {
        let mut map = rooms.write().await;
        let st = map.entry(room.clone()).or_default();
        if st.tx.is_none() {
            let (s, _) = tokio::sync::broadcast::channel::<String>(100);
            st.tx = Some(s);
        }
        st.tx.as_ref().unwrap().clone()
    };

    /* 4. 升級 WebSocket --------------------------------------------------- */
    Ok(ws.on_upgrade(move |sock| user_ws(sock, uid, name, room, tx, rooms)))
}

/* ─────────────── 每位使用者的連線生命週期 ─────────────── */
async fn user_ws(
    mut sock: WebSocket,
    uid: String,
    name: String,
    room: String,
    tx: Tx,
    rooms: RoomMap,
) {
    let mut rx = tx.subscribe();

    /* ---- 進房：更新使用者列表 ---- */
    {
        let mut map = rooms.write().await;
        let st = map.get_mut(&room).unwrap();
        st.users.push((uid.clone(), name.clone()));
        broadcast_users(&tx, &st.users);
    }

    /* ---- 同步收發 ---- */
    loop {
        tokio::select! {
            /* Client → Server */
            msg_from_client = sock.try_next() => {
                match msg_from_client {
                    Ok(Some(Message::Text(text))) => {
                        let msg = build_msg(&text, &name);
                        let _ = tx.send(msg);          // 廣播給房間
                    }
                    Ok(Some(Message::Close(_))) | Ok(None) | Err(_) => break,
                    _ => {} // 忽略 Binary / Ping / Pong
                }
            }

            /* Server → Client（房間廣播） */
            Ok(msg_from_room) = rx.recv() => {
                if sock.send(Message::Text(msg_from_room)).await.is_err() {
                    break;
                }
            }
        }
    }

    /* ---- 離房：移除使用者並更新列表 ---- */
    {
        let mut map = rooms.write().await;
        if let Some(st) = map.get_mut(&room) {
            st.users.retain(|(id, _)| id != &uid);
            broadcast_users(&tx, &st.users);
        }
    }
}

/* ─────────────── 工具函式 ─────────────── */
/// 把前端送來的 raw JSON/Text 填入 sender 欄位
fn build_msg(raw: &str, sender: &str) -> String {
    serde_json::from_str::<Value>(raw)
        .map(|mut v| {
            v["name"] = json!(sender);
            v.to_string()
        })
        .unwrap_or_else(|_| json!({ "type": "text", "name": sender, "text": raw }).to_string())
}

fn broadcast_users(tx: &Tx, list: &[(String, String)]) {
    let names: Vec<_> = list.iter().map(|(_, n)| n.clone()).collect();
    let _ = tx.send(json!({ "type": "users", "list": names }).to_string());
}
