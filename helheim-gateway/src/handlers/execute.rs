use axum::{
    extract::{State, Json},
    response::{IntoResponse, Response},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, error};
use crate::AppState;

#[derive(Deserialize)]
pub struct ExecuteRequest {
    pub script: String,
}

#[derive(Serialize)]
pub struct ExecuteResponse {
    pub success: bool,
    pub output: Option<String>,
    pub error: Option<String>,
}

pub async fn execute_script(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ExecuteRequest>,
) -> impl IntoResponse {
    info!("[GATEWAY] Verzoek ontvangen voor native Helheim execution.");

    if payload.script.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(ExecuteResponse {
            success: false,
            output: None,
            error: Some("Instructie mag niet leeg zijn.".into()),
        })).into_response();
    }

    // Native Execution via injected Orchestrator
    // Since process_command currently outputs to stdout dynamically, we await it
    // and stream back any errors directly.
    match state.orchestrator.process_command(&payload.script).await {
        Ok(_) => {
            (StatusCode::OK, Json(ExecuteResponse {
                success: true,
                output: Some("Instructie succesvol afgerond door the Swarm.".into()),
                error: None,
            })).into_response()
        }
        Err(e) => {
            let err_msg = e.to_string();
            error!("[GATEWAY] Fout tijdens executie LLK: {}", err_msg);
            
            // Foutafhandeling wordt robuust: stroomt leesbare foutmelding terug
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ExecuteResponse {
                success: false,
                output: None,
                error: Some(err_msg),
            })).into_response()
        }
    }
}
