mod state;
mod error;
mod utils {
    pub mod jwt;
    pub mod clean;
}
mod routes;

use axum::{
    Router, Extension, extract::DefaultBodyLimit
};
use tower_http::{limit::RequestBodyLimitLayer, services::ServeDir};
use sqlx::SqlitePool;
use crate::state::{RoomMap, MediaLog};
use crate::utils::clean;
use error::AppErr;

const BODY_LIMIT: usize = 100 * 1024 * 1024;

#[tokio::main]
async fn main() -> Result<(), AppErr> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt().init();

    let pool  = SqlitePool::connect(&std::env::var("DATABASE_URL")?).await?;
    let rooms = RoomMap::default();
    let media = MediaLog::default();

    tokio::spawn(clean::task(media.clone()));  // 啟動清道夫

    let app = Router::new()
        .nest("/",     ServeDir::new("static").into_service())
        .merge(routes::router())
        .layer(Extension(pool))
        .layer(Extension(rooms))
        .layer(Extension(media))
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
        .layer(RequestBodyLimitLayer::new(BODY_LIMIT));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}
