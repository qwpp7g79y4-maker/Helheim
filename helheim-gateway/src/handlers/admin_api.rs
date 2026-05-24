use axum::{
    extract::{State, Path},
    response::{IntoResponse, Response, Html},
    Json,
    http::StatusCode,
};
use std::sync::Arc;
use crate::AppState;
use super::common::extract_api_key;

/// GET /api/v1/admin/status — full platform status for admin panel
pub async fn admin_status(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Response {
    let api_key = match extract_api_key(&headers) {
        Some(k) => k,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    if !state.api_keys.is_admin(&api_key).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    let features = state.external.get_features().await;
    let health = state.external.get_health().await;
    let providers = state.external.list_providers_full().await;
    let models = state.external.list_all_models().await;
    let stats = state.queue.stats().await;
    let nodes = state.queue.get_nodes().await;

    let node_list: Vec<serde_json::Value> = nodes.iter().map(|(id, n)| {
        serde_json::json!({
            "id": id,
            "online": n.online,
            "cpu_cores": n.capabilities.cpu_cores,
            "ram_mb": n.capabilities.ram_mb,
            "gpu_count": n.capabilities.gpu_count,
            "gpu_models": n.capabilities.gpu_models,
            "load": n.load,
        })
    }).collect();

    // User stats
    let user_count = state.api_keys.count_users();
    let usage_stats = state.api_keys.get_usage_stats_summary();

    Json(serde_json::json!({
        "features": features,
        "providers": providers,
        "provider_health": health,
        "external_models": models,
        "cluster": {
            "nodes": node_list,
            "node_count": nodes.len(),
            "stats": {
                "completed": stats.completed,
                "queued": stats.queued,
                "active": stats.active,
            }
        },
        "users": {
            "total": user_count,
        },
        "usage": usage_stats,
    })).into_response()
}

/// POST /api/v1/admin/features — update feature flags
pub async fn update_features(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(body): Json<crate::external_api::FeatureFlags>,
) -> Response {
    let api_key = match extract_api_key(&headers) {
        Some(k) => k,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    if !state.api_keys.is_admin(&api_key).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    match state.external.set_features(body).await {
        Ok(()) => Json(serde_json::json!({"status": "saved"})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// GET /api/v1/admin/health — provider health status
pub async fn provider_health(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Response {
    let api_key = match extract_api_key(&headers) {
        Some(k) => k,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    if !state.api_keys.is_admin(&api_key).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    let health = state.external.get_health().await;
    Json(serde_json::json!({"health": health})).into_response()
}

/// GET /api/v1/admin/models — all external models across providers
pub async fn list_external_models(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Response {
    let api_key = match extract_api_key(&headers) {
        Some(k) => k,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    if !state.api_keys.is_admin(&api_key).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    let models = state.external.list_all_models().await;
    Json(serde_json::json!({"models": models, "count": models.len()})).into_response()
}

/// POST /api/v1/admin/reload — reload providers from disk
pub async fn reload_providers(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Response {
    let api_key = match extract_api_key(&headers) {
        Some(k) => k,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    if !state.api_keys.is_admin(&api_key).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    state.external.reload().await;
    Json(serde_json::json!({"status": "reloaded"})).into_response()
}

/// GET /api/v1/admin/users/:api_key/activity — detailed user activity
pub async fn user_activity(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(target_key): Path<String>,
) -> Response {
    let api_key = match extract_api_key(&headers) {
        Some(k) => k,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    if !state.api_keys.is_admin(&api_key).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    let activity = state.api_keys.get_user_activity(&target_key);
    Json(activity).into_response()
}

/// GET /terms — Terms of Service, Privacy Policy, Disclaimer
pub async fn terms_page() -> Html<String> {
    Html(include_str!("../../static/terms.html").to_string())
}

/// POST /api/v1/feedback — submit feedback (any user, even anonymous)
pub async fn submit_feedback(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let api_key = extract_api_key(&headers);
    let email = api_key.as_deref().and_then(|k| state.api_keys.get_email_for_key(k));
    let category = body["category"].as_str().unwrap_or("general");
    let message = match body["message"].as_str() {
        Some(m) if !m.trim().is_empty() => m,
        _ => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "message is required"}))).into_response(),
    };
    let page = body["page"].as_str();
    let ua = headers.get("user-agent").and_then(|v| v.to_str().ok());

    match state.api_keys.submit_feedback(api_key.as_deref(), email.as_deref(), category, message, page, ua) {
        Ok(id) => Json(serde_json::json!({"status": "submitted", "id": id})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// GET /api/v1/admin/feedback — list all feedback (admin only)
pub async fn list_feedback(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let api_key = match extract_api_key(&headers) {
        Some(k) => k,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    if !state.api_keys.is_admin(&api_key).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    let status_filter = params.get("status").map(|s| s.as_str());
    let feedback = state.api_keys.list_feedback(status_filter);
    let new_count = state.api_keys.count_new_feedback();
    Json(serde_json::json!({"feedback": feedback, "new_count": new_count})).into_response()
}

/// POST /api/v1/admin/feedback/:id — update feedback status (admin only)
pub async fn update_feedback(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(feedback_id): Path<i64>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let api_key = match extract_api_key(&headers) {
        Some(k) => k,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    if !state.api_keys.is_admin(&api_key).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    let status = body["status"].as_str().unwrap_or("reviewed");
    let admin_note = body["admin_note"].as_str();
    let ok = state.api_keys.update_feedback(feedback_id, status, admin_note);
    Json(serde_json::json!({"updated": ok})).into_response()
}

/// GET /feedback — feedback page
pub async fn feedback_page(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let api_key = extract_api_key(&headers).unwrap_or_default();
    let is_admin = if !api_key.is_empty() { state.api_keys.is_admin(&api_key).await } else { false };
    let html = include_str!("../../static/feedback.html")
        .replace("{{ROLE}}", if is_admin { "ADMIN" } else { "STANDARD" })
        .replace("{{ROLE_COLOR}}", if is_admin { "red" } else { "green" })
        .replace("{{API_KEY}}", &api_key);
    Html(html).into_response()
}

/// GET /api/v1/admin/users — detailed user list with credits, plan, providers
pub async fn admin_users(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Response {
    let api_key = match extract_api_key(&headers) {
        Some(k) => k,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    if !state.api_keys.is_admin(&api_key).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    let users = state.api_keys.list_users_detailed();
    Json(serde_json::json!({"users": users})).into_response()
}
