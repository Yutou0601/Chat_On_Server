use axum::{routing::post, Router, Extension, Json, extract::multipart::Multipart};
use tokio::{fs::{self, File}, io::AsyncWriteExt};
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

    let mime = field.content_type().unwrap_or("application/octet-stream");
    let ext  = if mime.starts_with("audio/webm") {
        "weba"
    } else {
        mime_guess::get_mime_extensions_str(mime)
            .and_then(|a| a.first().copied()).unwrap_or("bin")
    };

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
