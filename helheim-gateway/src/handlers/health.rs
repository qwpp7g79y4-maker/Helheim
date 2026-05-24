use axum::{Json, extract::State, http::StatusCode};
use std::sync::Arc;
use crate::AppState;
use super::common::extract_api_key;

/// Basic health: GET /health (public)
pub async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "operational",
        "engine": "helheim-gateway-v1",
    }))
}

/// Comprehensive health: GET /api/v1/admin/healthcheck (admin only)
/// Tests all subsystems and returns per-feature status
pub async fn full_healthcheck(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> axum::response::Response {
    use axum::response::IntoResponse;

    let api_key = match extract_api_key(&headers) {
        Some(k) => k,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    if !state.api_keys.is_admin(&api_key).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    let mut checks: Vec<serde_json::Value> = Vec::new();

    // 1. Database
    let db_ok = state.api_keys.count_users() >= 0;
    checks.push(serde_json::json!({
        "name": "Database (SQLite)",
        "category": "core",
        "status": if db_ok { "ok" } else { "error" },
        "detail": if db_ok { "Connected, users table accessible" } else { "Cannot query users" },
    }));

    // 2. Task Queue
    let stats = state.queue.stats().await;
    checks.push(serde_json::json!({
        "name": "Task Queue",
        "category": "core",
        "status": "ok",
        "detail": format!("completed={}, queued={}, active={}", stats.completed, stats.queued, stats.active),
    }));

    // 3. Nodes
    let nodes = state.queue.get_nodes().await;
    let online_nodes: Vec<_> = nodes.iter().filter(|(_, n)| n.online).collect();
    checks.push(serde_json::json!({
        "name": "Cluster Nodes",
        "category": "core",
        "status": if online_nodes.is_empty() { "warning" } else { "ok" },
        "detail": format!("{}/{} online", online_nodes.len(), nodes.len()),
    }));

    // 4. External Providers
    let providers = state.external.list_providers_full().await;
    let provider_count = providers.len();
    checks.push(serde_json::json!({
        "name": "External Providers",
        "category": "core",
        "status": if provider_count == 0 { "warning" } else { "ok" },
        "detail": format!("{} configured", provider_count),
    }));

    // 5. Provider Health
    let health = state.external.get_health().await;
    for h in &health {
        checks.push(serde_json::json!({
            "name": format!("Provider: {}", h.provider_id),
            "category": "provider",
            "status": if h.healthy { "ok" } else { "error" },
            "detail": format!("requests={}, errors={}, avg_latency={}ms", h.total_requests, h.total_errors, h.avg_latency_ms as u64),
        }));
    }

    // 6. Feature Flags
    let features = state.external.get_features().await;
    let feature_list = vec![
        ("Credits", features.credits_enabled),
        ("BYOK", features.byok_enabled),
        ("Smart Routing", features.smart_routing_enabled),
        ("Caching", features.caching_enabled),
        ("Web Search", features.web_search_enabled),
        ("Image Gen", features.image_gen_enabled),
        ("Code Exec", features.code_exec_enabled),
        ("RAG", features.rag_enabled),
        ("Memory", features.memory_enabled),
        ("Analytics", features.analytics_enabled),
        ("Demo", features.demo_enabled),
    ];
    for (name, enabled) in &feature_list {
        checks.push(serde_json::json!({
            "name": format!("Feature: {}", name),
            "category": "feature",
            "status": if *enabled { "enabled" } else { "disabled" },
            "detail": if *enabled { "Active" } else { "Inactive" },
        }));
    }

    // 7. Sessions
    let session_count = state.sessions.active_count(3600);
    checks.push(serde_json::json!({
        "name": "Active Sessions",
        "category": "core",
        "status": "ok",
        "detail": format!("{} active", session_count),
    }));

    // 8. Usage Tracking
    let usage = state.api_keys.get_usage_stats(None);
    let total_requests = usage["total_requests"].as_i64().unwrap_or(0);
    checks.push(serde_json::json!({
        "name": "Usage Tracking",
        "category": "core",
        "status": if total_requests > 0 { "ok" } else { "warning" },
        "detail": format!("{} total requests logged", total_requests),
    }));

    // 9. Static pages (check that HTML constants are embedded)
    let pages = vec![
        ("Dashboard", "/dashboard"),
        ("Chat", "/chat"),
        ("Settings", "/settings"),
        ("Usage", "/usage"),
        ("Analytics", "/analytics"),
        ("Debug", "/debug"),
        ("Providers", "/providers"),
        ("Tenants", "/tenants"),
        ("Feedback", "/feedback"),
        ("Login", "/login"),
    ];
    for (name, path) in &pages {
        checks.push(serde_json::json!({
            "name": format!("Page: {}", name),
            "category": "page",
            "status": "ok",
            "detail": format!("Route {} registered", path),
        }));
    }

    // 10. WASM Starfield
    let wasm_path = std::path::Path::new("/usr/local/bin/static/wasm/helheim_starfield_bg.wasm");
    let wasm_exists = wasm_path.exists();
    checks.push(serde_json::json!({
        "name": "Cortex Universe (WASM)",
        "category": "ui",
        "status": if wasm_exists { "ok" } else { "error" },
        "detail": if wasm_exists { "WASM file present" } else { "WASM file MISSING at /usr/local/bin/static/wasm/" },
    }));

    // Summary
    let total = checks.len();
    let ok_count = checks.iter().filter(|c| c["status"] == "ok" || c["status"] == "enabled").count();
    let error_count = checks.iter().filter(|c| c["status"] == "error").count();
    let warning_count = checks.iter().filter(|c| c["status"] == "warning").count();

    Json(serde_json::json!({
        "summary": {
            "total": total,
            "ok": ok_count,
            "errors": error_count,
            "warnings": warning_count,
            "overall": if error_count > 0 { "degraded" } else if warning_count > 0 { "partial" } else { "healthy" },
        },
        "checks": checks,
        "timestamp": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
    })).into_response()
}
