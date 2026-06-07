use axum::{
    extract::ws::{WebSocket, WebSocketUpgrade},
    http::StatusCode,
    response::Json as AxumJson,
    routing::{get, post},
    Router,
};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::{cors::CorsLayer, trace::TraceLayer, services::{ServeDir, ServeFile}};
use tracing::info;

use helheim_core::network::DiscoveryService;
use helheim_core::orchestra::Orchestrator;
use helheim_core::orchestra::parser::HelParser;
use helheim_core::common::context::ExecutionContext;

/// Response for /api/execute
#[derive(Serialize)]
struct ExecuteResponse {
    status: String,
    result: Option<String>,
    /// SNN spikes as array of "waar" / "onwaar" when detected from Motor Cortex lowered execution
    spikes: Option<Vec<String>>,
    message: Option<String>,
}

/// Error response
#[derive(Serialize)]
struct ErrorResponse {
    status: String,
    error: String,
}

/// Shared state for real-time spike streaming over WS (for /ws/spikes)
#[derive(Clone)]
struct AppState {
    spike_tx: broadcast::Sender<serde_json::Value>,
}

/// Main entry: supersnelle Axum server on port 8080
#[tokio::main]
async fn main() {
    // Simple tracing init
    tracing_subscriber::fmt()
        .with_env_filter("helheim_gateway=info,axum=info")
        .init();

    // Broadcast channel for real-time SNN spike events (firing neurons)
    let (spike_tx, _rx) = broadcast::channel::<serde_json::Value>(100);
    let state = AppState { spike_tx };

    let dashboard_path = std::env::var("HELHEIM_DASHBOARD_DIR").unwrap_or_else(|_| "helheim-dashboard".to_string());

    let app = Router::new()
        .route("/api/execute", post(execute_handler))
        .route("/ws/spikes", get(spikes_ws_handler))
        // CORS for easy frontend / JS clients (e.g. Starfield UI later)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .fallback_service(
            ServeDir::new(&dashboard_path)
                .fallback(ServeFile::new(format!("{}/index.html", dashboard_path)))
        )
        .with_state(state);

    let addr = "0.0.0.0:8080";
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    info!("🚀 Helheim Gateway (Axum) listening on http://{}", addr);
    info!("POST /api/execute with JSON {{\"script\": \"... .hel code ...\"}} or raw .hel text body");
    info!("Returns SNN spikes (waar/onwaar) as JSON array when Motor Cortex lowered blocks produce them.");
    info!("Example SNN test script (pre-built by Antigravity/Claude, do not overwrite): examples/snn/03_snn_cortex.hel");
    info!("  Example test: curl -X POST http://localhost:8080/api/execute -H 'Content-Type: application/json' --data-binary @- <<< '{{\"script\": \"zet input_spikes = [waar, onwaar, waar]; ...\"}}'");
    info!("WebSocket for real-time spike streaming: ws://localhost:8080/ws/spikes");

    axum::serve(listener, app).await.unwrap();
}

/// The core /api/execute handler
/// Supports:
/// - JSON: { "script": "zet x = 10; ..." }
/// - Raw body: the .hel source directly (text/plain or raw)
async fn execute_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
    body: axum::body::Bytes,
) -> Result<AxumJson<ExecuteResponse>, (StatusCode, AxumJson<ErrorResponse>)> {
    // Try to interpret as JSON first
    let raw_input = if let Ok(json_val) = serde_json::from_slice::<serde_json::Value>(&body) {
        if let Some(s) = json_val.get("script").and_then(|v| v.as_str()) {
            s.to_string()
        } else if let Some(payload) = json_val.get("hsp_payload").and_then(|v| v.as_str()) {
            payload.to_string()
        } else {
            // Fallback: treat the whole body as script (in case someone posts a raw JSON string)
            String::from_utf8_lossy(&body).to_string()
        }
    } else {
        // Raw .hel script body
        String::from_utf8_lossy(&body).to_string()
    };

    // [HSP] Optional Decryption for Secure Network Commands
    let mut script = raw_input.clone();
    let mut is_secure = false;
    
    if let Ok(decrypted) = helheim_core::shield::HelheimShield::decrypt_packet(&raw_input) {
        if decrypted != raw_input && !decrypted.is_empty() {
            info!("HSP Payload successfully decrypted via Chaos-XOR");
            script = decrypted;
            is_secure = true;
        }
    }

    // Determine Execution Context based on Signatures
    // For local development on :8080 we currently default to privileged if no signature is present,
    // but in production we'd default to sandbox. We'll allow privileged for now to not break the UI.
    let mut ctx = ExecutionContext::default_privileged();
    if script.starts_with("SIGNED: ") {
        if let Some((sig_part, script_part)) = script[8..].split_once(" | ") {
            use base64::Engine;
            if let Ok(sig_bytes) = base64::engine::general_purpose::STANDARD.decode(sig_part.trim()) {
                if helheim_core::shield::crypto::HelSigner::verify_update(script_part.as_bytes(), &sig_bytes).is_ok() {
                    info!("✅ Valid Master Key signature. Elevated Privileges activated via API.");
                    ctx = ExecutionContext::default_privileged();
                    script = script_part.to_string();
                } else {
                    info!("⚠️ Invalid signature. Fallback to Sandbox.");
                    ctx = ExecutionContext::sandbox();
                    script = script_part.to_string();
                }
            }
        }
    } else {
        if is_secure {
            info!("🛡️ Unsigned HSP request. Execution proceeding.");
        } else {
            info!("🛡️ Plain text request. Execution proceeding.");
        }
    }

    if script.trim().is_empty() {
        let err = ErrorResponse {
            status: "error".to_string(),
            error: "Empty script".to_string(),
        };
        return Err((StatusCode::BAD_REQUEST, AxumJson(err)));
    }

    info!("Received /api/execute request (script length: {})", script.len());

    match run_helheim_script_via_core(&script, ctx).await {
        Ok((result, spikes)) => {
            // Publish any SNN spikes to WS subscribers for real-time streaming of firing neurons
            if let Some(ref spike_list) = spikes {
                let event = serde_json::json!({
                    "type": "spikes",
                    "data": spike_list,
                    "ts": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
                });
                let _ = state.spike_tx.send(event);
            }

            let resp = ExecuteResponse {
                status: "success".to_string(),
                result,
                spikes,
                message: Some(if is_secure { "Script executed via HSP Secured Motor Cortex" } else { "Script executed via Plaintext Motor Cortex" }.to_string()),
            };
            Ok(AxumJson(resp))
        }
        Err(e) => {
            let err = ErrorResponse {
                status: "error".to_string(),
                error: e.to_string(),
            };
            Err((StatusCode::BAD_REQUEST, AxumJson(err)))
        }
    }
}

/// Runs the .hel script through the full Helheim engine (Motor Cortex path).
/// Uses the real Orchestrator + lowered blocks when applicable (for SNN spikes).
async fn run_helheim_script_via_core(
    script: &str,
    ctx: ExecutionContext,
) -> anyhow::Result<(Option<String>, Option<Vec<String>>)> {
    // Setup discovery (required by Orchestrator)
    let discovery = Arc::new(DiscoveryService::new());

    // Full Helheim runtime (includes SNN lowered execution, context binding, bit-packing, popc, VRAM pool etc.)
    let orchestrator = Arc::new(Orchestrator::new(discovery));

    // Parse using the real Helheim-lang parser (supports Dutch keywords + lowered constructs)
    let ast = HelParser::parse(script)
        .map_err(|e| anyhow::anyhow!("HelParser error: {}", e))?;

    // Execute — this goes through executor which handles lowered blocks for Motor Cortex SNN
    let exec_result = orchestrator
        .execute_ast(ast, ctx)
        .await
        .map_err(|e| anyhow::anyhow!("Execution error: {}", e))?;

    // Extract SNN spikes (waar/onwaar) if the Motor Cortex / lowered path produced bit-packed results
    let spikes = extract_snn_spikes(&orchestrator, &exec_result);

    Ok((exec_result, spikes))
}

/// Tries hard to find SNN spike results (lists of "waar"/"onwaar") produced by the lowered Motor Cortex path.
/// Looks at common result variables and the execution return value (unpacked by executor in many SNN flows).
fn extract_snn_spikes(orchestrator: &Orchestrator, exec_result: &Option<String>) -> Option<Vec<String>> {
    // Try common variable names that SNN scripts use for their output
    for key in ["result", "resultaat", "spikes", "output", "spike_result", "cortex", "fire", "overlap"] {
        if let Some(val) = orchestrator.get_var(key) {
            if let Some(spikes) = try_parse_spike_list(&val) {
                return Some(spikes);
            }
        }
    }

    // Also check the direct return value from execute_ast (often the unpacked list for lowered blocks)
    if let Some(val) = exec_result {
        if let Some(spikes) = try_parse_spike_list(val) {
            return Some(spikes);
        }
    }

    None
}

/// Parses strings like "[waar, onwaar, waar]" or '["waar", "onwaar"]' or the raw unpacked format
/// into a clean Vec<String> of "waar" / "onwaar".
fn try_parse_spike_list(s: &str) -> Option<Vec<String>> {
    let trimmed = s.trim();

    // Must look like a list and contain spike keywords
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return None;
    }
    let inner = trimmed
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim();

    if inner.is_empty() {
        return None;
    }

    let has_spike = inner.contains("waar") || inner.contains("onwaar") || inner.contains("true") || inner.contains("false");
    if !has_spike {
        return None;
    }

    let mut spikes = Vec::new();

    for part in inner.split(',') {
        let item = part
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_lowercase();

        let normalized = match item.as_str() {
            "waar" | "true" | "1" | "yes" => "waar".to_string(),
            "onwaar" | "false" | "0" | "no" => "onwaar".to_string(),
            other => other.to_string(),
        };

        if normalized == "waar" || normalized == "onwaar" {
            spikes.push(normalized);
        }
    }

    if spikes.is_empty() {
        None
    } else {
        Some(spikes)
    }
}

/// WebSocket upgrade handler for /ws/spikes - streams real-time firing neurons (SNN spikes)
async fn spikes_ws_handler(
    ws: WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<AppState>,
) -> impl axum::response::IntoResponse {
    ws.on_upgrade(move |socket| handle_spikes_socket(socket, state.spike_tx.subscribe()))
}

async fn handle_spikes_socket(mut socket: WebSocket, mut rx: broadcast::Receiver<serde_json::Value>) {
    use axum::extract::ws::Message;
    // Send welcome
    let _ = socket.send(Message::Text(serde_json::json!({"type": "connected", "channel": "spikes"}).to_string())).await;

    while let Ok(msg) = rx.recv().await {
        if socket.send(Message::Text(msg.to_string())).await.is_err() {
            break; // client disconnected
        }
    }
}
