use axum::{
    extract::State,
    Json,
    http::StatusCode,
};
use serde::Deserialize;
use std::sync::Arc;
use crate::AppState;
use crate::auth::TenantUpdate;
use super::common::extract_api_key;

#[derive(Deserialize)]
pub struct CreateTenantRequest {
    pub name: String,
    pub template: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateTenantRequest {
    pub name: Option<String>,
    pub domain: Option<String>,
    pub faq: Option<String>,
    pub system_prompt: Option<String>,
    pub welcome_message: Option<String>,
    pub model: Option<String>,
    pub color_primary: Option<String>,
    pub color_bg: Option<String>,
    pub color_text: Option<String>,
    pub bot_type: Option<String>,
    pub tools: Option<String>,
    pub tool_config: Option<String>,
    pub active: Option<bool>,
}

/// Create tenant: POST /api/v1/tenants
pub async fn create_tenant(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<CreateTenantRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let api_key = extract_api_key(&headers).ok_or((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "unauthorized"}))))?;
    if !state.api_keys.validate(&api_key).await {
        return Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "invalid key"}))));
    }
    let id = state.api_keys.create_tenant(&api_key, &req.name)
        .map_err(|msg| (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": msg}))))?;

    // Apply template if specified
    if let Some(ref tmpl_id) = req.template {
        if let Some(tmpl) = crate::tools::bot_templates().into_iter().find(|t| t.id == tmpl_id.as_str()) {
            let tools_json = serde_json::to_string(&tmpl.tools).unwrap_or_else(|_| "[]".into());
            let updates = TenantUpdate {
                name: None, domain: None, faq: None,
                system_prompt: Some(tmpl.system_prompt.to_string()),
                welcome_message: Some(tmpl.welcome_message.to_string()),
                model: None, color_primary: None, color_bg: None, color_text: None,
                bot_type: Some(tmpl.id.to_string()),
                tools: Some(tools_json),
                tool_config: None, active: None,
            };
            let _ = state.api_keys.update_tenant(&api_key, &id, &updates);
        }
    }

    Ok(Json(serde_json::json!({"status": "ok", "id": id})))
}

/// List bot templates: GET /api/v1/templates
pub async fn list_templates() -> Json<serde_json::Value> {
    Json(serde_json::json!({"templates": crate::tools::bot_templates()}))
}

/// List tenants: GET /api/v1/tenants
pub async fn list_tenants(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }
    let tenants = state.api_keys.list_tenants(&api_key);
    Ok(Json(serde_json::json!({"tenants": tenants})))
}

/// Get tenant: GET /api/v1/tenants/:tenant_id
pub async fn get_tenant(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }
    state.api_keys.get_tenant(&api_key, &tenant_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// Update tenant: PUT /api/v1/tenants/:tenant_id
pub async fn update_tenant(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
    Json(req): Json<UpdateTenantRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let api_key = extract_api_key(&headers).ok_or((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "unauthorized"}))))?;
    if !state.api_keys.validate(&api_key).await {
        return Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "invalid key"}))));
    }
    let updates = TenantUpdate {
        name: req.name, domain: req.domain, faq: req.faq,
        system_prompt: req.system_prompt, welcome_message: req.welcome_message,
        model: req.model, color_primary: req.color_primary,
        color_bg: req.color_bg, color_text: req.color_text,
        bot_type: req.bot_type, tools: req.tools, tool_config: req.tool_config,
        active: req.active,
    };
    match state.api_keys.update_tenant(&api_key, &tenant_id, &updates) {
        Ok(()) => Ok(Json(serde_json::json!({"status": "ok"}))),
        Err(msg) => Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": msg})))),
    }
}

/// Delete tenant: DELETE /api/v1/tenants/:tenant_id
pub async fn delete_tenant(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }
    if state.api_keys.delete_tenant(&api_key, &tenant_id) {
        Ok(Json(serde_json::json!({"status": "ok"})))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// List available tools: GET /api/v1/tools
pub async fn list_available_tools() -> Json<serde_json::Value> {
    let tools = crate::tools::available_tools();
    Json(serde_json::json!({"tools": tools}))
}
