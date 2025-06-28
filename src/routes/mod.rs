use axum::Router;

pub mod auth;
pub mod upload;
pub mod ws;

pub fn router() -> Router {
    Router::new()
        .nest("/api",  auth::router().merge(upload::router()))
        .nest("/ws",   ws::router())
}
