use axum::{
    extract::{State, Json},
    http::{StatusCode, HeaderMap},
};
use serde::Deserialize;
use std::sync::Arc;
use crate::AppState;

fn extract_api_key(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

// --- Hash endpoint ---

#[derive(Deserialize)]
pub struct HashRequest {
    pub path: String,
}

pub async fn hash(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<HashRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let task_type = helheim_protocol::TaskType::Hash { path: req.path };
    let task_id = state.queue.submit(api_key, task_type, None).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "task_id": task_id, "status": "queued" })))
}

// --- Log Analysis endpoint ---

#[derive(Deserialize)]
pub struct LogAnalysisRequest {
    pub path: String,
    pub pattern: Option<String>,
}

pub async fn log_analysis(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<LogAnalysisRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let task_type = helheim_protocol::TaskType::LogAnalysis { path: req.path, pattern: req.pattern };
    let task_id = state.queue.submit(api_key, task_type, None).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "task_id": task_id, "status": "queued" })))
}
