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
pub struct SaveChatRequest {
    pub id: String,
    pub title: String,
    pub messages: serde_json::Value,
    pub model: Option<String>,
    pub system_prompt: Option<String>,
}

/// Save/update a chat: POST /api/v1/chats
pub async fn save_chat(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<SaveChatRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }
    let messages_str = serde_json::to_string(&req.messages).unwrap_or_default();
    state.api_keys.save_chat(&api_key, &req.id, &req.title, &messages_str, req.model.as_deref(), req.system_prompt.as_deref())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"status": "ok", "id": req.id})))
}

/// List chats: GET /api/v1/chats
pub async fn list_chats(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }
    let chats = state.api_keys.list_chats(&api_key);
    Ok(Json(serde_json::json!({"chats": chats})))
}

/// Get a chat: GET /api/v1/chats/:chat_id
pub async fn get_chat(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(chat_id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }
    state.api_keys.get_chat(&api_key, &chat_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// Delete a chat: DELETE /api/v1/chats/:chat_id
pub async fn delete_chat(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(chat_id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }
    if state.api_keys.delete_chat(&api_key, &chat_id) {
        Ok(Json(serde_json::json!({"status": "ok"})))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// Search chats (FTS5): GET /api/v1/chats/search?q=...&offset=0&limit=50
pub async fn search_chats(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    axum::extract::Query(params): axum::extract::Query<SearchParams>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }
    let query = params.q.unwrap_or_default();
    if query.trim().is_empty() {
        let offset = params.offset.unwrap_or(0);
        let limit = params.limit.unwrap_or(100);
        let chats = state.api_keys.list_chats_paginated(&api_key, offset, limit);
        let total = state.api_keys.count_chats(&api_key);
        return Ok(Json(serde_json::json!({"chats": chats, "total": total})));
    }
    let chats = state.api_keys.search_chats(&api_key, &query);
    Ok(Json(serde_json::json!({"chats": chats, "query": query, "total": chats.len()})))
}

#[derive(Deserialize)]
pub struct SearchParams {
    pub q: Option<String>,
    pub offset: Option<u64>,
    pub limit: Option<u64>,
}

/// Pin/unpin a chat: POST /api/v1/chats/:chat_id/pin
pub async fn pin_chat(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(chat_id): axum::extract::Path<String>,
    Json(req): Json<PinRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }
    if state.api_keys.pin_chat(&api_key, &chat_id, req.pinned) {
        Ok(Json(serde_json::json!({"status": "ok", "pinned": req.pinned})))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

#[derive(Deserialize)]
pub struct PinRequest {
    pub pinned: bool,
}

/// Tag a chat: POST /api/v1/chats/:chat_id/tags
pub async fn tag_chat(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(chat_id): axum::extract::Path<String>,
    Json(req): Json<TagRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }
    if state.api_keys.tag_chat(&api_key, &chat_id, &req.tags) {
        Ok(Json(serde_json::json!({"status": "ok", "tags": req.tags})))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

#[derive(Deserialize)]
pub struct TagRequest {
    pub tags: Vec<String>,
}
