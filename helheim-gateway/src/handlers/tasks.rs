use axum::{
    extract::{State, Json, Path},
    http::StatusCode,
};
use std::sync::Arc;
use helheim_protocol::*;
use crate::AppState;

pub async fn submit_task(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ApiRequest>,
) -> Result<Json<ApiResponse>, StatusCode> {
    // Validate API key
    if !state.api_keys.validate(&req.api_key).await {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Block dangerous task types for non-admin keys
    let is_dangerous = matches!(&req.task, 
        helheim_protocol::TaskType::Execute { .. } | 
        helheim_protocol::TaskType::Store { .. }
    );
    if is_dangerous && !state.api_keys.is_admin(&req.api_key).await {
        return Ok(Json(ApiResponse {
            task_id: String::new(),
            status: TaskStatus::Failed,
            result: None,
            error: Some("Permission denied: this task type requires admin privileges".to_string()),
        }));
    }

    match state.queue.submit(req.api_key, req.task, req.priority).await {
        Ok(task_id) => Ok(Json(ApiResponse {
            task_id,
            status: TaskStatus::Queued,
            result: None,
            error: None,
        })),
        Err(e) => Ok(Json(ApiResponse {
            task_id: String::new(),
            status: TaskStatus::Failed,
            result: None,
            error: Some(e.to_string()),
        })),
    }
}

pub async fn get_task(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Result<Json<ApiResponse>, StatusCode> {
    match state.queue.get_task(&task_id).await {
        Some(task) => Ok(Json(ApiResponse {
            task_id: task.id,
            status: task.status,
            result: task.result,
            error: None,
        })),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn complete_task(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(task_id): Path<String>,
    Json(result): Json<TaskResult>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !crate::handlers::nodes::verify_cluster_secret(&headers, &state.cluster_secret) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    state.queue.complete_task(&task_id, result).await;
    state.queue.try_assign().await;
    Ok(Json(serde_json::json!({ "status": "completed", "task_id": task_id })))
}
