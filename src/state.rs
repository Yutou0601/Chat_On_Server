use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::Arc,
};
use tokio::sync::{broadcast, RwLock};

pub type Tx = broadcast::Sender<String>;

/* ------------ WebSocket 房間 ------------ */
#[derive(Default)]
pub struct RoomState {
    pub tx:      Option<Tx>,
    pub users:   Vec<(String, String)>,    // (uid, username)
    pub history: VecDeque<String>,         // 最新 60 則
}
pub type RoomMap = Arc<RwLock<HashMap<String, RoomState>>>;

/* ------------ 上傳媒體清單 -------------- */
#[derive(Clone)]
pub struct MediaEntry {
    pub path: PathBuf,
    pub size: u64,
    pub room: String,
}
pub type MediaLog = Arc<RwLock<VecDeque<MediaEntry>>>;
