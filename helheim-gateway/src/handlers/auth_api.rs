use axum::{
    extract::State,
    response::IntoResponse,
    Json,
    http::StatusCode,
};
use serde::Deserialize;
use std::sync::Arc;
use crate::AppState;
use super::common::{extract_api_key, set_session_cookie};

#[derive(Deserialize)]
pub struct AuthRequest {
    pub email: String,
    pub password: String,
}

/// Register endpoint: POST /api/v1/register
pub async fn register_user(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AuthRequest>,
) -> Result<axum::response::Response, (StatusCode, Json<serde_json::Value>)> {
    match state.api_keys.register_user(&req.email, &req.password) {
        Ok(api_key) => {
            let mut resp = axum::Json(serde_json::json!({
                "status": "ok",
                "api_key": api_key,
                "email": req.email.trim().to_lowercase()
            })).into_response();
            resp.headers_mut().insert(axum::http::header::SET_COOKIE, set_session_cookie(&api_key));
            Ok(resp)
        }
        Err(msg) => Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "status": "error",
            "message": msg
        })))),
    }
}

/// Login endpoint: POST /api/v1/login
pub async fn login_user(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AuthRequest>,
) -> Result<axum::response::Response, (StatusCode, Json<serde_json::Value>)> {
    match state.api_keys.login_user(&req.email, &req.password) {
        Ok(api_key) => {
            let mut resp = axum::Json(serde_json::json!({
                "status": "ok",
                "api_key": api_key
            })).into_response();
            resp.headers_mut().insert(axum::http::header::SET_COOKIE, set_session_cookie(&api_key));
            Ok(resp)
        }
        Err(msg) => Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({
            "status": "error",
            "message": msg
        })))),
    }
}

/// Logout: GET /logout — clears cookie + localStorage, then redirects to login
pub async fn logout() -> axum::response::Response {
    let html = r#"<!DOCTYPE html><html><head><script>
localStorage.removeItem('helheim_api_key');
localStorage.removeItem('helheim_email');
localStorage.removeItem('helheim_settings');
window.location.href='/login';
</script></head><body></body></html>"#;
    let mut resp = axum::response::Html(html).into_response();
    let clear = axum::http::HeaderValue::from_static("helheim_session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0");
    resp.headers_mut().insert(axum::http::header::SET_COOKIE, clear);
    resp
}

/// List all users: GET /api/v1/users (admin only)
pub async fn list_users(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.is_admin(&api_key).await { return Err(StatusCode::FORBIDDEN); }
    let users = state.api_keys.list_users();
    Ok(Json(serde_json::json!({"users": users, "count": users.len()})))
}

/// Delete user: DELETE /api/v1/users/:user_id (admin only)
pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(user_id): axum::extract::Path<i64>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.is_admin(&api_key).await { return Err(StatusCode::FORBIDDEN); }
    if state.api_keys.delete_user(user_id) {
        Ok(Json(serde_json::json!({"status": "ok"})))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}
