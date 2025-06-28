use tokio::{fs, time};
use crate::state::{MediaLog};

pub const DISK_CAP: u64 = 10 * 1024 * 1024 * 1024; // 10 GB

pub async fn task(log: MediaLog) {
    let mut tick = time::interval(time::Duration::from_secs(30));
    loop {
        tick.tick().await;
        let used: u64 = log.read().await.iter().map(|m| m.size).sum();
        if used <= DISK_CAP { continue; }

        let mut lg = log.write().await;
        let mut space = used;
        while space > DISK_CAP {
            if let Some(old) = lg.pop_front() {
                if fs::remove_file(&old.path).await.is_ok() {
                    space -= old.size;
                }
            } else { break; }
        }
    }
}
