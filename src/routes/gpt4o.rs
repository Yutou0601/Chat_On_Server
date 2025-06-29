// src/routes/gpt4o.rs

use axum::{
    extract::Json as ReqJson,
    response::{IntoResponse, Json as RespJson},
    http::StatusCode,
    routing::post,
    Router,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;

#[derive(Deserialize)]
pub struct GptReq {
    pub prompt: String,
}

#[derive(Serialize)]
struct GptResp {
    answer: String,
}

pub fn router() -> Router {
    Router::new().route("/gpt4o", post(handler))
}

async fn handler(ReqJson(payload): ReqJson<GptReq>) -> impl IntoResponse {
    // 1) 取得 API Key
    let api_key = match env::var("OPENAI_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Missing OPENAI_API_KEY".to_string(),
            )
                .into_response()
        }
    };
    // 2) 取得 base URL（可再 .env 裡設 OPENAI_API_BASE）
    let base_url = env::var("OPENAI_API_BASE")
        .unwrap_or_else(|_| "https://api.chatanywhere.tech/v1".into());

    // 3) 準備並送出請求
    let client = Client::new();
    let req_body = serde_json::json!({
        "model": "gpt-4o",
        "messages": [
            { "role": "user", "content": payload.prompt }
        ]
    });
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));

    let resp = match client
        .post(&url)
        .bearer_auth(api_key)
        .json(&req_body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("request error: {}", e),
            )
                .into_response()
        }
    };

    // 4) 如果非 2xx，就把狀態與 body 原封不動回傳
    if !resp.status().is_success() {
        let status = StatusCode::from_u16(resp.status().as_u16())
            .unwrap_or(StatusCode::BAD_GATEWAY);
        let text = resp.text().await.unwrap_or_default();
        return (status, text).into_response();
    }

    // 5) 解析 JSON 回應
    #[derive(Deserialize)]
    struct Choice {
        message: Value,
    }
    #[derive(Deserialize)]
    struct ApiResp {
        choices: Vec<Choice>,
    }
    let api: ApiResp = match resp.json().await {
        Ok(json) => json,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("parse error: {}", e),
            )
                .into_response();
        }
    };

    // 6) 擷取回覆
    let answer = api
        .choices
        .get(0)
        .and_then(|c| c.message.get("content"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    // 7) 成功回傳
    (
        StatusCode::OK,
        RespJson(GptResp { answer }),
    )
        .into_response()
}
