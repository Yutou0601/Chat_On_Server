use axum::{Router, routing::get, extract::{ws::{WebSocketUpgrade, WebSocket, Message}, Query, Extension}};
use futures_util::stream::StreamExt;
use serde::Deserialize;
use serde_json::{json, Value};
use crate::{state::{RoomMap, Tx}, utils::jwt, error::AppResult};

#[derive(Deserialize)] struct WsQuery { room:Option<String>, token:String }

pub fn router() -> Router {
    Router::new().route("/chat", get(ws_handler))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(q): Query<WsQuery>,
    Extension(secret): Extension<String>,
    Extension(rooms): Extension<RoomMap>,
) -> AppResult<impl IntoResponse> {
    let uid = jwt::verify(&q.token, &secret).ok_or(crate::error::bad("bad token"))?;
    let name = "anon".to_string();                    // ← demo: 省略 DB 查詢

    let room = q.room.unwrap_or_else(||"lobby".into());
    let tx   = {
        let mut m = rooms.write().await;
        let st = m.entry(room.clone()).or_default();
        if st.tx.is_none() { let (s,_) = tokio::sync::broadcast::channel(100); st.tx = Some(s); }
        st.tx.as_ref().unwrap().clone()
    };
    Ok(ws.on_upgrade(move |s| user_ws(s,uid,name,room,tx,rooms)))
}

/* ---------------- per user ---------------- */
async fn user_ws(
    mut sock: WebSocket,
    uid: String, name:String, room:String,
    tx: Tx, rooms: RoomMap
){
    let mut rx = tx.subscribe();
    /* 進房 */
    {
        let mut m = rooms.write().await;
        let st = m.get_mut(&room).unwrap();
        st.users.push((uid.clone(),name.clone()));
        broadcast_users(&tx,&st.users);
    }

    while let Some(Ok(Message::Text(raw))) = sock.next().await {
        let msg = build_msg(&raw,&name);
        tx.send(msg).ok();
    }
    /* 離房 */
    {
        let mut m = rooms.write().await;
        let st = m.get_mut(&room).unwrap();
        st.users.retain(|(id,_)|id!=&uid);
        broadcast_users(&tx,&st.users);
    }
}

fn build_msg(raw:&str, sender:&str)->String {
    serde_json::from_str::<Value>(raw)
        .map(|mut v|{ v["name"]=json!(sender); v.to_string() })
        .unwrap_or_else(|_| json!({"type":"text","name":sender,"text":raw}).to_string())
}

fn broadcast_users(tx:&Tx,list:&[(String,String)]){
    let names:Vec<_>=list.iter().map(|(_,n)|n.clone()).collect();
    tx.send(json!({"type":"users","list":names}).to_string()).ok();
}
