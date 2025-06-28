use axum::{routing::post, Json, Router, Extension};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use crate::{error::{AppResult, AppErr}, utils::jwt};

#[derive(Deserialize)] struct AuthInput { username:String, password:String }
#[derive(Serialize)]  struct TokenJson  { token:String, user_id:String }

pub fn router() -> Router {
    Router::new()
        .route("/register", post(register))
        .route("/login"   , post(login))
}

async fn register(
    Extension(pool): Extension<SqlitePool>,
    Json(p): Json<AuthInput>,
) -> AppResult<()> {
    let salt = nanoid::nanoid!(16);
    let hash = format!("{salt}${}", p.password);
    let uid  = uuid::Uuid::new_v4().to_string();

    sqlx::query("INSERT INTO users (id,username,password) VALUES (?,?,?)")
        .bind(&uid).bind(&p.username).bind(&hash)
        .execute(&pool).await?;
    Ok(())
}

async fn login(
    Extension(pool): Extension<SqlitePool>,
    Extension(secret): Extension<String>,
    Json(p): Json<AuthInput>,
) -> AppResult<Json<TokenJson>> {
    let row: (String,String,String) =
        sqlx::query_as("SELECT id,username,password FROM users WHERE username=?")
        .bind(&p.username).fetch_optional(&pool).await?
        .ok_or(AppErr::Bad("user not found".into()))?;

    if row.2.split('$').last() != Some(p.password.as_str()) {
        return Err(AppErr::Bad("wrong password".into()));
    }

    let tk = jwt::sign(&row.0, &secret);
    Ok(Json(TokenJson{token:tk, user_id:row.0}))
}
