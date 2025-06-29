// src/routes/mod.rs

use axum::Router;

pub mod auth;
pub mod upload;
pub mod ws;
pub mod gpt4o;

/// 汇总各个子路由
pub fn router() -> Router {
    Router::new()
        .nest(
            "/api",
            Router::new()
                .merge(auth::router())
                .merge(upload::router())
                .merge(gpt4o::router()),  // /api/gpt4o
        )
        .nest(
            "/ws",
            ws::router(),
        )
}
