use axum::{http::StatusCode, response::IntoResponse};
use std::fmt::Display;

pub type AppResult<T> = Result<T, AppErr>;

#[derive(thiserror::Error, Debug)]   // ✅ thiserror 宏
pub enum AppErr {
    #[error("Bad request: {0}")]
    Bad(String),

    #[error("IO: {0}")]
    Io(#[from] std::io::Error),

    #[error("DB: {0}")]
    Db(#[from] sqlx::Error),
}

impl IntoResponse for AppErr {
    fn into_response(self) -> axum::response::Response {
        let (code, body) = match self {
            AppErr::Bad(msg) => (StatusCode::BAD_REQUEST, msg),
            other            => (StatusCode::INTERNAL_SERVER_ERROR, other.to_string()),
        };
        (code, body).into_response()
    }
}

/* ── 小助手：把任何 error 轉成 Bad / Io ── */
pub fn bad<E: Display>(e: E) -> AppErr { AppErr::Bad(e.to_string()) }
/* 若手動使用 ⇒ io(std_err)? */
pub fn io<E: Into<std::io::Error>>(e: E) -> AppErr {
    AppErr::Io(e.into())
}
