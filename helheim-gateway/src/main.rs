use axum::{
    extract::{State, DefaultBodyLimit},
    http::StatusCode,
    response::Json as AxumJson,
    routing::{get, post},
    Router,
};
use serde::Serialize;
use std::sync::Arc;
use tower_http::{cors::CorsLayer, trace::TraceLayer, services::{ServeDir, ServeFile}};
use tracing::info;

use helheim_core::network::DiscoveryService;
use helheim_core::orchestra::Orchestrator;
use helheim_core::orchestra::parser::HelParser;
use helheim_core::common::context::ExecutionContext;

#[derive(Serialize)]
struct ExecuteResponse {
    status: String,
    result: Option<String>,
    message: Option<String>,
}

#[derive(Serialize)]
struct ErrorResponse {
    status: String,
    error: String,
}

#[derive(Clone)]
struct AppState {
    orchestrator: Arc<Orchestrator>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("helheim_gateway=info,axum=info")
        .init();

    let dashboard_path = std::env::var("HELHEIM_DASHBOARD_DIR")
        .unwrap_or_else(|_| "helheim-dashboard".to_string());

    let discovery = Arc::new(DiscoveryService::new());
    let orchestrator = Arc::new(Orchestrator::new(discovery));
    let _ = orchestrator.bootstrap().await;

    let state = AppState { orchestrator };

    let app = Router::new()
        .route("/api/execute", post(execute_handler))
        .route("/health", get(health_handler))
        .layer(DefaultBodyLimit::max(1024 * 1024 * 10))
        .layer(
            CorsLayer::new()
                .allow_origin(
                    std::env::var("HELHEIM_ALLOWED_ORIGINS")
                        .unwrap_or_else(|_| "http://localhost:3000,http://127.0.0.1:3000".to_string())
                        .split(',')
                        .filter_map(|s| s.trim().parse::<axum::http::HeaderValue>().ok())
                        .collect::<Vec<_>>()
                )
                .allow_methods(vec![axum::http::Method::GET, axum::http::Method::POST])
                .allow_headers(vec![axum::http::header::CONTENT_TYPE, axum::http::header::AUTHORIZATION])
        )
        .layer(TraceLayer::new_for_http())
        .fallback_service(
            ServeDir::new(&dashboard_path)
                .fallback(ServeFile::new(format!("{}/index.html", dashboard_path))),
        )
        .with_state(state);

    let addr = "0.0.0.0:8080";
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    info!("Helheim Gateway listening on http://{}", addr);
    info!("POST /api/execute  — body: {{\"script\": \"...\"}} or raw .hel text");
    info!("GET  /health       — liveness check");

    axum::serve(listener, app).await.unwrap();
}

async fn health_handler() -> &'static str {
    "ok"
}

async fn execute_handler(
    State(state): State<AppState>,
    body: axum::body::Bytes,
) -> Result<AxumJson<ExecuteResponse>, (StatusCode, AxumJson<ErrorResponse>)> {
    let raw_input = if let Ok(json_val) = serde_json::from_slice::<serde_json::Value>(&body) {
        if let Some(s) = json_val.get("script").and_then(|v| v.as_str()) {
            s.to_string()
        } else if let Some(payload) = json_val.get("hsp_payload").and_then(|v| v.as_str()) {
            payload.to_string()
        } else {
            String::from_utf8_lossy(&body).to_string()
        }
    } else {
        String::from_utf8_lossy(&body).to_string()
    };

    let mut script = raw_input.clone();
    let mut is_secure = false;

    let master_key = helheim_core::shield::crypto::HelSigner::get_master_key();
    if let Ok(decrypted) = helheim_core::shield::HelheimShield::decrypt_packet_with_key(&raw_input, &master_key) {
        if decrypted != raw_input && !decrypted.is_empty() {
            info!("HSP payload decrypted");
            script = decrypted;
            is_secure = true;
        }
    }

    let mut ctx = ExecutionContext::sandbox();
    if script.starts_with("SIGNED: ") {
        if let Some((sig_part, script_part)) = script[8..].split_once(" | ") {
            use base64::Engine;
            if let Ok(sig_bytes) = base64::engine::general_purpose::STANDARD.decode(sig_part.trim()) {
                if helheim_core::shield::crypto::HelSigner::verify_update(script_part.as_bytes(), &sig_bytes).is_ok() {
                    info!("Valid signature — elevated privileges activated");
                    ctx = ExecutionContext::default_privileged();
                    script = script_part.to_string();
                } else {
                    info!("Invalid signature — rejecting script execution");
                    return Err((
                        StatusCode::UNAUTHORIZED,
                        AxumJson(ErrorResponse { status: "error".to_string(), error: "Invalid signature for SIGNED script".to_string() }),
                    ));
                }
            } else {
                return Err((
                    StatusCode::BAD_REQUEST,
                    AxumJson(ErrorResponse { status: "error".to_string(), error: "Malformed Base64 signature".to_string() }),
                ));
            }
        }
    }

    if script.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            AxumJson(ErrorResponse { status: "error".to_string(), error: "Empty script".to_string() }),
        ));
    }

    info!("Executing script ({} bytes, secure={})", script.len(), is_secure);

    let ast = match HelParser::parse(&script) {
        Ok(a) => a,
        Err(e) => return Err((
            StatusCode::BAD_REQUEST,
            AxumJson(ErrorResponse { status: "error".to_string(), error: format!("Parse error: {}", e) }),
        )),
    };

    match state.orchestrator.execute_ast(ast, ctx).await {
        Ok(result) => Ok(AxumJson(ExecuteResponse {
            status: "success".to_string(),
            result,
            message: None,
        })),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            AxumJson(ErrorResponse { status: "error".to_string(), error: format!("Execution error: {}", e) }),
        )),
    }
}

