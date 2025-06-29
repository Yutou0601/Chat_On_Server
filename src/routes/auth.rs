//! routes/auth.rs
use axum::{
    routing::post,
    response::IntoResponse,              // ★ register 需要回傳 IntoResponse
    extract::{Extension, Json},
    http::StatusCode,
    Router,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::{
    error::{AppErr, AppResult},
    utils::jwt,
};

#[derive(Deserialize)]
struct AuthInput {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct TokenJson {
    token: String,
    user_id: String,
}

pub fn router() -> Router {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
}

/* ---------------- Register ---------------- */
async fn register(
    Extension(pool): Extension<SqlitePool>,
    Json(p): Json<AuthInput>,
) -> AppResult<impl IntoResponse> {
    // nanoid / uuid 需要在 Cargo.toml 加：
    // nanoid = "0.4"     uuid = { version = "1", features = ["v4"] }
    let salt = nanoid::nanoid!(16);
    let hash = format!("{salt}${}", p.password);
    let uid  = uuid::Uuid::new_v4().to_string();

    sqlx::query("INSERT INTO users (id, username, password) VALUES (?,?,?)")
        .bind(&uid)
        .bind(&p.username)
        .bind(&hash)
        .execute(&pool)
        .await?;

    Ok(StatusCode::CREATED)              // ★ 201 Created
}

/* ---------------- Login ---------------- */
async fn login(
    Extension(pool): Extension<SqlitePool>,
    Extension(secret): Extension<String>,
    Json(p): Json<AuthInput>,
) -> AppResult<Json<TokenJson>> {
    // 直接用 tuple 取回 (id, username, password)
    let (id, _uname, pwd): (String, String, String) =
        sqlx::query_as("SELECT id, username, password FROM users WHERE username = ?")
            .bind(&p.username)
            .fetch_optional(&pool)
            .await?
            .ok_or_else(|| AppErr::Bad("user not found".into()))?;

    // 簡易比對；正式環境請改用 bcrypt/argon2
    if pwd.split('$').last() != Some(p.password.as_str()) {
        return Err(AppErr::Bad("wrong password".into()));
    }

    let token = jwt::sign(&id, &secret);
    Ok(Json(TokenJson { token, user_id: id }))
}
