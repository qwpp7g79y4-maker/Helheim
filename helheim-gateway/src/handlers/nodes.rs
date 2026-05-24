use axum::{
    extract::{State, Json, Path},
    http::{StatusCode, HeaderMap},
};
use std::sync::Arc;
use helheim_protocol::*;
use crate::AppState;

pub async fn list_nodes(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    state.queue.check_timeouts().await;
    let nodes = state.queue.get_nodes().await;
    let node_list: Vec<serde_json::Value> = nodes.iter().map(|(id, state)| {
        serde_json::json!({
            "node_id": id,
            "online": state.online,
            "active_tasks": state.active_tasks,
            "active_inference": state.active_inference,
            "load": state.load,
            "loaded_models": state.loaded_models,
            "capabilities": state.capabilities.capabilities,
            "gpu_count": state.capabilities.gpu_count,
            "gpu_models": state.capabilities.gpu_models,
            "total_vram_mb": state.capabilities.total_vram_mb,
            "ram_mb": state.capabilities.ram_mb,
            "cpu_cores": state.capabilities.cpu_cores,
        })
    }).collect();
    let online_count = nodes.values().filter(|n| n.online).count();

    Json(serde_json::json!({ "nodes": node_list, "count": online_count }))
}

pub async fn register_node(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(caps): Json<NodeCapabilities>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !verify_cluster_secret(&headers, &state.cluster_secret) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let node_id = caps.node_id.clone();
    state.queue.register_node(caps).await;
    state.queue.try_assign().await;
    Ok(Json(serde_json::json!({ "status": "registered", "node_id": node_id })))
}

pub async fn node_heartbeat(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(msg): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !verify_cluster_secret(&headers, &state.cluster_secret) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let node_id = msg["node_id"].as_str().unwrap_or("");
    let load = msg["load"].as_f64().unwrap_or(0.0);
    let loaded_models: Vec<String> = msg["loaded_models"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let active_inference = msg["active_inference"].as_u64().unwrap_or(0) as u32;
    if state.queue.heartbeat_with_models(node_id, load, loaded_models, active_inference).await {
        state.queue.try_assign().await;
        Ok(Json(serde_json::json!({ "status": "ok" })))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

pub async fn get_node_tasks(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(node_id): Path<String>,
) -> Result<Json<Vec<Task>>, StatusCode> {
    if !verify_cluster_secret(&headers, &state.cluster_secret) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    state.queue.try_assign().await;
    Ok(Json(state.queue.get_assigned_tasks(&node_id).await))
}

pub fn verify_cluster_secret(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get("x-cluster-secret")
        .and_then(|v| v.to_str().ok())
        .map(|v| v == expected)
        .unwrap_or(false)
}
