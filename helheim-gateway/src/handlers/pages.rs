use axum::{
    extract::{State, Query},
    response::{Html, IntoResponse, Response},
    http::header,
};
use std::collections::HashMap;
use std::sync::Arc;
use crate::AppState;
use super::common::{extract_cookie_key, login_redirect, set_session_cookie};

/// Extract key from cookie, header, or ?key= query param
fn extract_key_from_all(headers: &axum::http::HeaderMap, params: &HashMap<String, String>) -> Option<String> {
    // 1. Cookie or Authorization header
    if let Some(k) = extract_cookie_key(headers) {
        if !k.is_empty() { return Some(k); }
    }
    // 2. ?key= query parameter
    if let Some(k) = params.get("key") {
        let k = k.trim().to_string();
        if !k.is_empty() { return Some(k); }
    }
    None
}

/// Serve HTML page, setting cookie if key came from query param
fn serve_page(html: String, api_key: &str, headers: &axum::http::HeaderMap) -> Response {
    let has_cookie = headers.get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(|c| c.contains("helheim_session="))
        .unwrap_or(false);

    if has_cookie {
        Html(html).into_response()
    } else {
        // Set cookie so subsequent requests work
        let mut resp = Html(html).into_response();
        resp.headers_mut().insert(header::SET_COOKIE, set_session_cookie(api_key));
        resp
    }
}

const LANDING_HTML: &str = include_str!("../../static/landing.html");
const DASHBOARD_HTML: &str = include_str!("../../static/dashboard.html");
const CHAT_HTML: &str = include_str!("../../static/chat.html");
const LOGIN_HTML: &str = include_str!("../../static/login.html");
const TENANTS_HTML: &str = include_str!("../../static/tenants.html");
const DEMO_HTML: &str = include_str!("../../static/demo.html");
const DEBUG_HTML: &str = include_str!("../../static/debug.html");
const ANALYTICS_HTML: &str = include_str!("../../static/analytics.html");
const SETTINGS_HTML: &str = include_str!("../../static/settings.html");
const USAGE_HTML: &str = include_str!("../../static/usage.html");
const HEALTH_HTML: &str = include_str!("../../static/health.html");
const DOCS_HTML: &str = include_str!("../../static/docs.html");
const STATUS_HTML: &str = include_str!("../../static/status.html");

/// Landing page
pub async fn landing_page() -> Html<&'static str> {
    Html(LANDING_HTML)
}

/// Demo page — public, no auth needed
pub async fn demo_page() -> Html<&'static str> {
    Html(DEMO_HTML)
}

/// Docs page — public API documentation
pub async fn docs_page() -> Html<&'static str> {
    Html(DOCS_HTML)
}

/// Status page — public service status
pub async fn status_page() -> Html<&'static str> {
    Html(STATUS_HTML)
}

/// Login page
pub async fn login_page() -> Html<&'static str> {
    Html(LOGIN_HTML)
}

/// Dashboard - authenticated view
pub async fn dashboard_page(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let api_key = match extract_key_from_all(&headers, &params) {
        Some(k) => k,
        _ => return login_redirect("dashboard").into_response(),
    };
    if !state.api_keys.validate(&api_key).await {
        return login_redirect("dashboard").into_response();
    }

    let is_admin = state.api_keys.is_admin(&api_key).await;
    let stats = state.queue.stats().await;
    let nodes = state.queue.get_nodes().await;
    let node_count = nodes.len();
    let total_gpus: u32 = nodes.values().map(|n| n.capabilities.gpu_count).sum();

    let html = DASHBOARD_HTML
        .replace("{{API_KEY}}", &api_key)
        .replace("{{ROLE}}", if is_admin { "ADMIN" } else { "STANDARD" })
        .replace("{{ROLE_COLOR}}", if is_admin { "red" } else { "green" })
        .replace("{{HELHEIM_IS_ADMIN}}", if is_admin { "true" } else { "false" })
        .replace("{{NODE_COUNT}}", &node_count.to_string())
        .replace("{{GPU_COUNT}}", &total_gpus.to_string())
        .replace("{{COMPLETED}}", &stats.completed.to_string())
        .replace("{{QUEUED}}", &(stats.queued + stats.active).to_string());

    serve_page(html, &api_key, &headers)
}

/// Chat interface - authenticated view
pub async fn chat_page(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let api_key = match extract_key_from_all(&headers, &params) {
        Some(k) => k,
        _ => return login_redirect("chat").into_response(),
    };
    if !state.api_keys.validate(&api_key).await {
        return login_redirect("chat").into_response();
    }
    let is_admin = state.api_keys.is_admin(&api_key).await;
    let html = CHAT_HTML.replace("{{API_KEY}}", &api_key)
        .replace("{{ROLE}}", if is_admin { "ADMIN" } else { "STANDARD" })
        .replace("{{ROLE_COLOR}}", if is_admin { "red" } else { "green" });
    serve_page(html, &api_key, &headers)
}

/// Tenants management page
pub async fn tenants_page(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let api_key = match extract_key_from_all(&headers, &params) {
        Some(k) => k,
        _ => return login_redirect("tenants").into_response(),
    };
    if !state.api_keys.validate(&api_key).await {
        return login_redirect("tenants").into_response();
    }
    let is_admin = state.api_keys.is_admin(&api_key).await;
    let html = TENANTS_HTML.replace("{{API_KEY}}", &api_key)
        .replace("{{ROLE}}", if is_admin { "ADMIN" } else { "STANDARD" })
        .replace("{{ROLE_COLOR}}", if is_admin { "red" } else { "green" });
    serve_page(html, &api_key, &headers)
}

/// Debug page: GET /debug (admin only)
pub async fn debug_page(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let api_key = match extract_key_from_all(&headers, &params) {
        Some(k) => k,
        _ => return login_redirect("debug").into_response(),
    };
    if !state.api_keys.is_admin(&api_key).await {
        return Html("<h1>Admin only</h1>".to_string()).into_response();
    }
    let html = DEBUG_HTML.replace("{{API_KEY}}", &api_key)
        .replace("{{ROLE}}", "ADMIN")
        .replace("{{ROLE_COLOR}}", "red");
    serve_page(html, &api_key, &headers)
}

/// Analytics page: GET /analytics (admin only)
pub async fn analytics_page(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let api_key = match extract_key_from_all(&headers, &params) {
        Some(k) => k,
        _ => return login_redirect("analytics").into_response(),
    };
    if !state.api_keys.is_admin(&api_key).await {
        return Html("<h1>Admin only</h1>".to_string()).into_response();
    }
    let html = ANALYTICS_HTML.replace("{{API_KEY}}", &api_key)
        .replace("{{ROLE}}", "ADMIN")
        .replace("{{ROLE_COLOR}}", "red");
    serve_page(html, &api_key, &headers)
}

/// Usage page: GET /usage (authenticated)
pub async fn usage_page(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let api_key = match extract_key_from_all(&headers, &params) {
        Some(k) => k,
        _ => return login_redirect("usage").into_response(),
    };
    if !state.api_keys.validate(&api_key).await {
        return login_redirect("usage").into_response();
    }
    let is_admin = state.api_keys.is_admin(&api_key).await;
    let html = USAGE_HTML
        .replace("{{API_KEY}}", &api_key)
        .replace("{{ROLE}}", if is_admin { "ADMIN" } else { "STANDARD" })
        .replace("{{ROLE_COLOR}}", if is_admin { "red" } else { "green" });
    serve_page(html, &api_key, &headers)
}

/// Settings page: GET /settings (authenticated)
pub async fn settings_page(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let api_key = match extract_key_from_all(&headers, &params) {
        Some(k) => k,
        _ => return login_redirect("settings").into_response(),
    };
    if !state.api_keys.validate(&api_key).await {
        return login_redirect("settings").into_response();
    }
    let is_admin = state.api_keys.is_admin(&api_key).await;
    let html = SETTINGS_HTML.replace("{{API_KEY}}", &api_key)
        .replace("{{ROLE}}", if is_admin { "ADMIN" } else { "STANDARD" })
        .replace("{{ROLE_COLOR}}", if is_admin { "red" } else { "green" });
    serve_page(html, &api_key, &headers)
}

/// Health dashboard: GET /health-dashboard (admin only)
pub async fn health_page(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let api_key = match extract_key_from_all(&headers, &params) {
        Some(k) => k,
        _ => return login_redirect("health-dashboard").into_response(),
    };
    if !state.api_keys.is_admin(&api_key).await {
        return Html("<h1>Admin only</h1>".to_string()).into_response();
    }
    let html = HEALTH_HTML.replace("{{API_KEY}}", &api_key)
        .replace("{{ROLE}}", "ADMIN")
        .replace("{{ROLE_COLOR}}", "red");
    serve_page(html, &api_key, &headers)
}
