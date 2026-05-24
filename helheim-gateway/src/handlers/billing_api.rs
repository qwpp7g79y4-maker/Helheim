use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
    http::StatusCode,
};
use std::sync::Arc;
use crate::AppState;
use super::common::{extract_api_key, extract_cookie_key};

/// Get user profile: GET /api/v1/profile
pub async fn get_profile(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers)
        .or_else(|| extract_cookie_key(&headers))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }

    let profile = state.api_keys.get_user_profile(&api_key)
        .unwrap_or_else(|| serde_json::json!({"email": "unknown", "credits": 0, "plan": "free"}));

    let user_providers = state.api_keys.list_user_providers(&api_key);

    Ok(Json(serde_json::json!({
        "profile": profile,
        "providers": user_providers,
        "api_key": api_key,
    })))
}

/// Save user provider key (BYOK): POST /api/v1/profile/providers
pub async fn save_user_provider(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let api_key = match extract_api_key(&headers).or_else(|| extract_cookie_key(&headers)) {
        Some(k) => k,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    if !state.api_keys.validate(&api_key).await {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let provider_id = body["provider_id"].as_str().unwrap_or("").to_string();
    let provider_api_key = body["api_key"].as_str().unwrap_or("").to_string();

    if provider_id.is_empty() || provider_api_key.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "provider_id and api_key required"}))).into_response();
    }

    match state.api_keys.save_user_provider(&api_key, &provider_id, &provider_api_key) {
        Ok(()) => Json(serde_json::json!({"status": "saved", "provider_id": provider_id})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// Delete user provider key: DELETE /api/v1/profile/providers/:provider_id
pub async fn delete_user_provider(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(provider_id): axum::extract::Path<String>,
) -> Json<serde_json::Value> {
    let api_key = match extract_api_key(&headers).or_else(|| extract_cookie_key(&headers)) {
        Some(k) => k,
        None => return Json(serde_json::json!({"error": "unauthorized"})),
    };
    if state.api_keys.delete_user_provider(&api_key, &provider_id) {
        Json(serde_json::json!({"status": "deleted"}))
    } else {
        Json(serde_json::json!({"status": "not_found"}))
    }
}

/// Get credit balance and history: GET /api/v1/credits
pub async fn get_credits(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers)
        .or_else(|| extract_cookie_key(&headers))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }

    let credits = state.api_keys.get_credits(&api_key);
    let history = state.api_keys.get_credit_history(&api_key, 50);

    Ok(Json(serde_json::json!({
        "credits": credits,
        "history": history,
    })))
}

/// Admin: add credits to a user: POST /api/v1/credits/add
pub async fn add_credits(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let api_key = match extract_api_key(&headers) {
        Some(k) => k,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    if !state.api_keys.is_admin(&api_key).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    let target_key = body["api_key"].as_str().unwrap_or("").to_string();
    let amount = body["amount"].as_i64().unwrap_or(0);
    let reason = body["reason"].as_str().unwrap_or("admin_grant").to_string();

    if target_key.is_empty() || amount == 0 {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "api_key and amount required"}))).into_response();
    }

    match state.api_keys.adjust_credits(&target_key, amount, &reason, None, None) {
        Ok(new_balance) => Json(serde_json::json!({"status": "ok", "new_balance": new_balance})).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// Get pricing table: GET /api/v1/pricing
pub async fn get_pricing(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers)
        .or_else(|| extract_cookie_key(&headers))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }

    let pricing = state.api_keys.list_pricing();
    Ok(Json(serde_json::json!({"pricing": pricing})))
}

/// Admin: set pricing: POST /api/v1/pricing
pub async fn set_pricing(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let api_key = match extract_api_key(&headers) {
        Some(k) => k,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    if !state.api_keys.is_admin(&api_key).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    let provider = body["provider"].as_str().unwrap_or("").to_string();
    let model = body["model"].as_str().unwrap_or("").to_string();
    let cost = body["cost_per_1k_tokens"].as_f64().unwrap_or(0.001);
    let price = body["price_per_1k_tokens"].as_f64().unwrap_or(0.003);

    if provider.is_empty() || model.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "provider and model required"}))).into_response();
    }

    match state.api_keys.set_pricing(&provider, &model, cost, price) {
        Ok(()) => Json(serde_json::json!({"status": "saved"})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}
