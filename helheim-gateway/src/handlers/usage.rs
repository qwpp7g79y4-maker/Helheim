use axum::{
    extract::State,
    http::{StatusCode, HeaderMap},
    Json,
};
use std::sync::Arc;
use crate::AppState;

fn extract_api_key(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

pub async fn get_usage(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Accept API key from Bearer header or query param
    let api_key = extract_api_key(&headers)
        .or_else(|| params.get("api_key").cloned())
        .ok_or(StatusCode::BAD_REQUEST)?;

    if !state.api_keys.validate(&api_key).await {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let records = state.queue.get_usage(&api_key).await;
    let (prompt_total, completion_total, tokens_total, compute_ms_total) =
        state.queue.get_usage_summary(&api_key).await;

    Ok(Json(serde_json::json!({
        "summary": {
            "prompt_tokens": prompt_total,
            "completion_tokens": completion_total,
            "total_tokens": tokens_total,
            "total_compute_ms": compute_ms_total,
            "total_requests": records.len(),
        },
        "records": records,
    })))
}
