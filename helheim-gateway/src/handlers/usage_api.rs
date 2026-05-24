use axum::{
    extract::State,
    Json,
    http::StatusCode,
};
use std::sync::Arc;
use crate::AppState;
use super::common::extract_api_key;

/// Usage stats: GET /api/v1/usage/stats (admin sees all, user sees own)
pub async fn usage_stats(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }
    let is_admin = state.api_keys.is_admin(&api_key).await;
    let stats = if is_admin {
        state.api_keys.get_usage_stats(None)
    } else {
        state.api_keys.get_usage_stats(Some(&api_key))
    };
    Ok(Json(stats))
}

/// Per-user usage: GET /api/v1/usage/users (admin only)
pub async fn usage_by_user(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }
    if !state.api_keys.is_admin(&api_key).await { return Err(StatusCode::FORBIDDEN); }
    let users = state.api_keys.get_user_activity_stats();
    Ok(Json(serde_json::json!({"users": users})))
}

/// Per-tenant usage: GET /api/v1/usage/tenants
pub async fn usage_by_tenant(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }
    let tenants = state.api_keys.get_tenant_usage(&api_key);
    Ok(Json(serde_json::json!({"tenants": tenants})))
}
