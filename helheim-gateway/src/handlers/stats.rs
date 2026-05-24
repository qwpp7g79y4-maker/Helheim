use axum::extract::State;
use axum::Json;
use std::sync::Arc;
use crate::AppState;

pub async fn get_stats(
    State(state): State<Arc<AppState>>,
) -> Json<helheim_taskqueue::QueueStats> {
    Json(state.queue.stats().await)
}
