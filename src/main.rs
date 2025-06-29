//! Axum 0.7 Chat Server — main.rs
#![allow(clippy::let_and_return)]

/* ── 內部模組 ──────────────────────── */
mod state;
mod error;
mod utils {
    pub mod jwt;
    pub mod clean;
}
mod routes;

/* ── 外部依賴 ──────────────────────── */
use axum::{
    extract::DefaultBodyLimit,
    routing::get_service,
    Router, Extension,
};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use tower_http::{limit::RequestBodyLimitLayer, services::ServeDir};
use tracing::info;

/* ── 自家型別 ──────────────────────── */
use crate::{
    error::AppErr,
    state::{RoomMap, MediaLog},
    utils::clean,
};

/* ── 全域常數 ──────────────────────── */
const BODY_LIMIT: usize = 100 * 1024 * 1024;      // 100 MB

/* ── main ─────────────────────────── */
#[tokio::main]
async fn main() -> Result<(), AppErr> {
    /* 1. .env 與日誌 */
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt().init();

    /* 2. DB & 共用狀態 */
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://chat.db".into());

    let pool: SqlitePool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;

    let rooms = RoomMap::default();
    let media = MediaLog::default();
    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "change_this_secret".into());

    /* 3. 背景──磁碟清道夫 */
    tokio::spawn(clean::task(media.clone()));

    /* 4. 靜態檔服務  
         tower-http 0.6 的 ServeDir Error=Infallible ⇒ 無需 handle_error */
    let static_service = get_service(
        ServeDir::new("static")
            .append_index_html_on_directories(true)   // ← 先呼叫這裡
    );

    /* 5. Router */
    let app = Router::new()
        .nest_service("/", static_service)          // http://host/↔static/**
        .merge(routes::router())                    // /api /ws...
        .layer(Extension(pool))
        .layer(Extension(rooms))
        .layer(Extension(media))
        .layer(Extension(jwt_secret))
        .layer(DefaultBodyLimit::max(BODY_LIMIT))   // axum extract
        .layer(RequestBodyLimitLayer::new(BODY_LIMIT)); // tower-http

    /* 6. 監聽 */
    let addr = "0.0.0.0:3000";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("🚀  Listening on http://{addr}");

    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}
