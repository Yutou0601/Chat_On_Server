use axum::{http::StatusCode, response::{IntoResponse, Response}};
use std::fmt::Display;
use thiserror::Error;

/// 公開給全專案的結果型別
pub type AppResult<T> = Result<T, AppErr>;

#[derive(Error, Debug)]
pub enum AppErr {
    #[error("Bad request: {0}")]
    Bad(String),

    #[error("IO: {0}")]
    Io(#[from] std::io::Error),

    #[error("DB: {0}")]
    Db(#[from] sqlx::Error),

    #[error("Env: {0}")]
    Var(#[from] std::env::VarError),
}

/* -------- axum ⟷ AppErr -------- */
impl IntoResponse for AppErr {
    fn into_response(self) -> Response {
        let (code, body) = match self {
            AppErr::Bad(msg) => (StatusCode::BAD_REQUEST, msg),
            other            => (StatusCode::INTERNAL_SERVER_ERROR, other.to_string()),
        };
        (code, body).into_response()
    }
}

/* -------- 方便手動轉 -------- */
pub fn bad<E: Display>(e: E) -> AppErr { AppErr::Bad(e.to_string()) }
pub fn io<E: Into<std::io::Error>>(e: E) -> AppErr { AppErr::Io(e.into()) }
