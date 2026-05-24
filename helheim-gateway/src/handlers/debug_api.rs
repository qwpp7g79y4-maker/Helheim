use axum::{
    extract::{State, Query},
    Json,
    http::StatusCode,
};
use std::sync::Arc;
use std::collections::HashMap;
use crate::AppState;
use super::common::extract_api_key;

/// Debug events API: GET /api/v1/debug/events?limit=100&kind=inference
pub async fn debug_events(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.is_admin(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }
    let limit = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(100usize);
    let kind = params.get("kind").map(|s| s.as_str());
    let events = state.events.recent(limit, kind);
    let counters = state.events.counters();
    Ok(Json(serde_json::json!({
        "events": events,
        "counters": counters,
    })))
}

/// Debug counters API: GET /api/v1/debug/counters
pub async fn debug_counters(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.is_admin(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }
    let counters = state.events.counters();
    let stats = state.queue.stats().await;
    let nodes = state.queue.get_nodes().await;
    Ok(Json(serde_json::json!({
        "counters": counters,
        "queue": {"queued": stats.queued, "active": stats.active, "completed": stats.completed, "nodes_online": stats.nodes_online},
        "nodes": nodes.len(),
    })))
}
