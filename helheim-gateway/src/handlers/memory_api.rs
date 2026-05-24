use axum::{
    extract::State,
    Json,
    http::StatusCode,
};
use serde::Deserialize;
use std::sync::Arc;
use crate::AppState;
use super::common::extract_api_key;

#[derive(Deserialize)]
pub struct StoreMemoryRequest {
    pub name: String,
    pub content: String,
    pub memory_type: Option<String>,
    pub tenant_id: Option<String>,
}

#[derive(Deserialize)]
pub struct RecallRequest {
    pub query: String,
    pub tenant_id: Option<String>,
    pub max_results: Option<usize>,
}

/// Store a memory: POST /api/v1/memories
pub async fn store_memory(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<StoreMemoryRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let api_key = extract_api_key(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "unauthorized"}))))?;
    if !state.api_keys.validate(&api_key).await {
        return Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "invalid key"}))));
    }

    let memory_type = req.memory_type.as_deref().unwrap_or("FACT");
    let tenant_id = req.tenant_id.as_deref();

    match state.api_keys.store_memory(&api_key, tenant_id, &req.name, &req.content, memory_type) {
        Ok(id) => {
            let count = state.api_keys.count_memories(&api_key, tenant_id);
            state.events.log("memory", "store", &format!("name={} type={}", req.name, memory_type),
                serde_json::json!({"memory_id": id, "name": req.name, "type": memory_type}), None, true);
            Ok(Json(serde_json::json!({"status": "ok", "id": id, "total_memories": count})))
        }
        Err(msg) => Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": msg})))),
    }
}

/// Recall relevant memories: POST /api/v1/memories/recall
pub async fn recall_memories(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<RecallRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }

    let max = req.max_results.unwrap_or(5).min(20);
    let memories = state.api_keys.recall_memories(&api_key, req.tenant_id.as_deref(), &req.query, max);

    Ok(Json(serde_json::json!({
        "query": req.query,
        "count": memories.len(),
        "memories": memories,
    })))
}

/// List all memories: GET /api/v1/memories
pub async fn list_memories(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    query: axum::extract::Query<ListMemoriesQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }

    let memories = state.api_keys.list_memories(&api_key, query.tenant_id.as_deref());
    let count = state.api_keys.count_memories(&api_key, query.tenant_id.as_deref());

    Ok(Json(serde_json::json!({
        "count": count,
        "memories": memories,
    })))
}

#[derive(Deserialize)]
pub struct ListMemoriesQuery {
    pub tenant_id: Option<String>,
}

/// Delete a memory: DELETE /api/v1/memories/:id
pub async fn delete_memory(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(memory_id): axum::extract::Path<i64>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }

    if state.api_keys.delete_memory(&api_key, memory_id) {
        Ok(Json(serde_json::json!({"status": "ok"})))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}
