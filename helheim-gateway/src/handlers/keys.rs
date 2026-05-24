use axum::{
    extract::{State, Json},
    http::StatusCode,
};
use std::sync::Arc;
use crate::AppState;

#[derive(serde::Deserialize)]
pub struct CreateKeyRequest {
    pub name: String,
    pub admin_key: Option<String>,
}

pub async fn create_api_key(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateKeyRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Only admin keys can create new keys
    if let Some(ref admin_key) = req.admin_key {
        if !state.api_keys.is_admin(admin_key).await {
            return Err(StatusCode::UNAUTHORIZED);
        }
    } else {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let key = state.api_keys.create_key(&req.name).await;
    Ok(Json(serde_json::json!({ "api_key": key, "name": req.name, "role": "standard" })))
}
