use axum::{routing::post, Router, Extension, Json, extract::multipart::Multipart};
use tokio::{fs::{self, File}, io::AsyncWriteExt};
use futures_util::stream::StreamExt;
use bytes::Bytes;
use serde_json::json;
use crate::{
    state::{MediaEntry, MediaLog},
    error::{AppResult, bad, io},
};

pub fn router() -> Router {
    Router::new().route("/upload", post(upload_file))
}

pub async fn upload_file(
    Extension(media): Extension<MediaLog>,
    mut mp: Multipart,
) -> AppResult<Json<serde_json::Value>> {
    let Some(mut field) = mp.next_field().await.map_err(bad)? else {
        return Err(bad("no file"));
    };

    // ---- MIME 先複製成 String，避免 borrow 衝突 ----
    let mime: String = field
        .content_type()
        .map(|s| s.to_owned())          // to_owned 解除 &str 借用
        .unwrap_or_else(|| "application/octet-stream".into());
        
    let ext = mime_guess::get_mime_extensions_str(&mime) // 改成 &mime
        .and_then(|arr| arr.first().copied())
        .unwrap_or("bin");

    let fname = format!("uploads/{}.{}", uuid::Uuid::new_v4(), ext);
    let full = {
        fs::create_dir_all("static/uploads").await.map_err(io)?;   // <- 用 io() 包
        std::path::Path::new("static").join(&fname)
    };

    let mut file = File::create(&full).await.map_err(io)?;
    while let Some(chunk) = field.next().await {
        let chunk:Bytes = chunk.map_err(bad)?;
        file.write_all(&chunk).await.map_err(io)?;
    }
    file.flush().await.map_err(io)?;

    let meta = fs::metadata(&full).await.map_err(io)?;
    media.write().await.push_back(MediaEntry{path:full, size:meta.len(), room:"global".into()});

    Ok(Json(json!({"url": format!("/{fname}"), "mime":mime})))
}
