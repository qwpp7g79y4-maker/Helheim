use axum::{
    extract::State,
    Json,
    http::StatusCode,
};
use std::sync::Arc;
use crate::AppState;
use super::common::extract_api_key;

/// Live analytics: GET /api/v1/analytics (admin only)
pub async fn get_analytics(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.is_admin(&api_key).await { return Err(StatusCode::FORBIDDEN); }

    // Cleanup stale sessions
    state.sessions.cleanup();

    // Active sessions (seen in last 5 minutes)
    let active = state.sessions.get_sessions(Some(300));
    let active_count = active.len();

    // Recent sessions (seen in last hour)
    let recent = state.sessions.get_sessions(Some(3600));

    // Usage stats from SQLite
    let usage = state.api_keys.get_usage_stats(None);

    // Per-user breakdown from usage_log (last 24h)
    let user_stats = state.api_keys.get_user_activity_stats();

    Ok(Json(serde_json::json!({
        "live": {
            "active_now": active_count,
            "active_sessions": active,
            "recent_sessions": recent.len(),
        },
        "users": user_stats,
        "usage": usage,
    })))
}
