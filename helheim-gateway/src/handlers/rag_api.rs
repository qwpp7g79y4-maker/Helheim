use axum::{
    extract::State,
    Json,
    http::StatusCode,
};
use serde::Deserialize;
use std::sync::Arc;
use crate::AppState;
use super::common::{extract_api_key, chunk_text};

#[derive(Deserialize)]
pub struct UploadDocRequest {
    pub tenant_id: String,
    pub filename: String,
    pub content: String,
}

/// Upload document: POST /api/v1/documents
pub async fn upload_document(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<UploadDocRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let api_key = extract_api_key(&headers).ok_or((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "unauthorized"}))))?;
    if !state.api_keys.validate(&api_key).await {
        return Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "invalid key"}))));
    }

    let chunks = chunk_text(&req.content, 500, 50);
    let chunk_count = chunks.len();
    let chunks_json = serde_json::to_string(&chunks).unwrap_or_else(|_| "[]".to_string());

    let doc_id = state.api_keys.add_document(&req.tenant_id, &req.filename, &req.content, &chunks_json, chunk_count)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))))?;

    state.events.log("rag_query", "upload", &format!("doc={} chunks={} tenant={}", req.filename, chunk_count, req.tenant_id),
        serde_json::json!({"filename": req.filename, "chunks": chunk_count, "tenant_id": req.tenant_id, "doc_id": doc_id}),
        None, true);

    Ok(Json(serde_json::json!({"status": "ok", "doc_id": doc_id, "chunks": chunk_count})))
}

/// List documents: GET /api/v1/documents/:tenant_id
pub async fn list_documents(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }
    let docs = state.api_keys.list_documents(&tenant_id);
    Ok(Json(serde_json::json!({"documents": docs})))
}

/// Delete document: DELETE /api/v1/documents/:tenant_id/:doc_id
pub async fn delete_document(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    axum::extract::Path((tenant_id, doc_id)): axum::extract::Path<(String, i64)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }
    if state.api_keys.delete_document(&tenant_id, doc_id) {
        Ok(Json(serde_json::json!({"status": "ok"})))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}
