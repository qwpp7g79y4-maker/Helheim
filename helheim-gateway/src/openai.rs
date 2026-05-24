use axum::{
    extract::{State, Json},
    http::{StatusCode, HeaderMap},
    response::{IntoResponse, Response, sse::{Event, Sse}},
};
use futures::stream;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

use crate::AppState;

// --- OpenAI-compatible request/response types ---

#[derive(Deserialize)]
pub struct ChatCompletionRequest {
    pub model: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<u32>,
    #[allow(dead_code)]
    pub temperature: Option<f32>,
    pub stream: Option<bool>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Serialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChatChoice>,
    pub usage: UsageInfo,
}

#[derive(Serialize)]
pub struct ChatChoice {
    pub index: u32,
    pub message: ChatMessage,
    pub finish_reason: String,
}

#[derive(Serialize)]
pub struct StreamChoice {
    pub index: u32,
    pub delta: StreamDelta,
    pub finish_reason: Option<String>,
}

#[derive(Serialize)]
pub struct StreamDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

#[derive(Serialize)]
pub struct StreamChunk {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<StreamChoice>,
}

#[derive(Serialize)]
pub struct UsageInfo {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[allow(dead_code)]
#[derive(Serialize)]
pub struct ModelObject {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub owned_by: String,
}

#[allow(dead_code)]
#[derive(Serialize)]
pub struct ModelListResponse {
    pub object: String,
    pub data: Vec<ModelObject>,
}

#[allow(dead_code)]
#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: ErrorDetail,
}

#[allow(dead_code)]
#[derive(Serialize)]
pub struct ErrorDetail {
    pub message: String,
    pub r#type: String,
    pub code: String,
}

fn extract_api_key(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

fn error_response(status: StatusCode, msg: &str, code: &str) -> Response {
    let body = serde_json::json!({
        "error": {
            "message": msg,
            "type": "error",
            "code": code,
        }
    });
    (status, Json(body)).into_response()
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// --- Handlers ---

/// Wait for task completion with configurable timeout (seconds)
async fn wait_for_task_timeout(
    state: &Arc<AppState>,
    task_id: &str,
    timeout_secs: u64,
) -> Result<(String, u64), Response> {
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);
    let poll_interval = tokio::time::Duration::from_millis(100);

    loop {
        if start.elapsed() > timeout {
            return Err(error_response(StatusCode::GATEWAY_TIMEOUT, "Inference timed out", "timeout"));
        }

        if let Some(task) = state.queue.get_task(task_id).await {
            match task.status {
                helheim_protocol::TaskStatus::Completed => {
                    let output = task.result.as_ref().map(|r| r.output.clone()).unwrap_or_default();
                    let duration_ms = task.result.as_ref().map(|r| r.duration_ms).unwrap_or(0);
                    return Ok((output, duration_ms));
                }
                helheim_protocol::TaskStatus::Failed => {
                    let err_msg = task.result
                        .as_ref()
                        .and_then(|r| r.error.clone())
                        .unwrap_or_else(|| "Inference failed".to_string());
                    return Err(error_response(StatusCode::INTERNAL_SERVER_ERROR, &err_msg, "inference_error"));
                }
                _ => {
                    tokio::time::sleep(poll_interval).await;
                }
            }
        } else {
            return Err(error_response(StatusCode::INTERNAL_SERVER_ERROR, "Task disappeared", "server_error"));
        }
    }
}

pub async fn chat_completions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<ChatCompletionRequest>,
) -> Response {
    // 1. Auth
    let api_key = match extract_api_key(&headers) {
        Some(k) => k,
        None => return error_response(StatusCode::UNAUTHORIZED, "Missing Authorization header", "auth_required"),
    };

    if !state.api_keys.validate(&api_key).await {
        return error_response(StatusCode::UNAUTHORIZED, "Invalid API key", "invalid_api_key");
    }

    // 1b. Rate limit
    let (allowed, _remaining, retry_after) = state.rate_limiter.check(&api_key);
    if !allowed {
        return error_response(
            StatusCode::TOO_MANY_REQUESTS,
            &format!("Rate limit exceeded. Retry after {} seconds.", retry_after),
            "rate_limit_exceeded",
        );
    }

    // 1c. Track session
    let email = state.api_keys.get_email_for_key(&api_key);
    state.sessions.touch(&api_key, email.as_deref(), req.model.as_deref(), Some("chat"), None, 0);

    // 2. Build prompt from messages (with memory recall)
    let model = req.model.unwrap_or_else(|| "auto".to_string());
    let max_tokens = req.max_tokens.unwrap_or(256);
    let stream = req.stream.unwrap_or(false);

    // Memory recall: hybrid (keyword + vector) based on last user message
    let user_query = req.messages.iter().rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.as_str())
        .unwrap_or("");
    let mut messages_with_memory = req.messages.clone();
    if !user_query.is_empty() {
        let query_embedding = state.external.generate_embedding(user_query).await;
        let memories = state.api_keys.recall_memories_hybrid(&api_key, None, user_query, query_embedding.as_deref(), 3);
        if !memories.is_empty() {
            let mem_texts: Vec<String> = memories.iter()
                .filter_map(|m| m["content"].as_str().map(|s| s.to_string()))
                .filter(|s| !s.trim_start().starts_with("system:"))
                .collect();
            if !mem_texts.is_empty() {
                let memory_context = format!(
                    "Relevante herinneringen uit eerdere gesprekken:\n{}\n\nGebruik deze context alleen als het relevant is voor de huidige vraag.",
                    mem_texts.join("\n---\n")
                );
                messages_with_memory.insert(0, ChatMessage {
                    role: "system".to_string(),
                    content: memory_context,
                });
                let mode = if query_embedding.is_some() { "hybrid" } else { "keyword" };
                state.events.log("memory_recall", "openai", &format!("memories={} mode={}", memories.len(), mode),
                    serde_json::json!({"memories_found": memories.len(), "mode": mode, "query_preview": &user_query[..user_query.len().min(80)]}),
                    None, true);
            }
        }
    }

    let prompt = build_prompt(&messages_with_memory);

    info!("[OPENAI] Chat completion: model={}, tokens={}, stream={}, messages={}", model, max_tokens, stream, req.messages.len());

    // 2b. Universal credit check (admin is exempt)
    let is_admin = state.api_keys.is_admin(&api_key).await;
    if !is_admin {
        let credits = state.api_keys.get_credits(&api_key);
        if credits <= 0 {
            return error_response(StatusCode::PAYMENT_REQUIRED,
                "Geen credits meer. Koop credits via Settings of voeg je eigen API key toe bij Providers.",
                "insufficient_credits");
        }
    }

    // 3. Check if this is an external provider request (e.g. "groq/llama-3.3-70b")
    let (output, prompt_tokens, completion_tokens, duration_ms) = if let Some((provider, ext_model)) = crate::external_api::ExternalProviders::parse_external_model(&model) {
        // External API proxy
        let ext_messages: Vec<serde_json::Value> = messages_with_memory.iter().map(|m| {
            serde_json::json!({"role": m.role, "content": m.content})
        }).collect();

        // Check if user has their own key for this provider (BYOK)
        let user_provider_key = state.api_keys.get_user_provider_key(&api_key, &provider);
        let using_byok = user_provider_key.is_some();

        // Use user's own key if available, otherwise use admin/global key
        let result = if let Some(ref user_key) = user_provider_key {
            state.external.proxy_chat_completion_with_key(&provider, &ext_model, &ext_messages, max_tokens, req.temperature, user_key).await
        } else {
            state.external.proxy_chat_completion(&provider, &ext_model, &ext_messages, max_tokens, req.temperature).await
        };

        match result {
            Ok(resp) => {
                // Deduct credits if using admin key (not BYOK, not admin)
                if !using_byok && !is_admin {
                    let credit_cost = state.api_keys.calculate_credit_cost(&provider, &ext_model, resp.total_tokens);
                    let _ = state.api_keys.adjust_credits(&api_key, -credit_cost,
                        &format!("inference:{}/{}", provider, ext_model), Some(&resp.model), None);
                }

                let billing = if using_byok { "byok" } else if is_admin { "admin" } else { "credits" };
                state.events.log("external_api", &provider, &format!("model={} tokens={} billing={}", resp.model, resp.total_tokens, billing),
                    serde_json::json!({"provider": resp.provider, "model": resp.model, "tokens": resp.total_tokens, "duration_ms": resp.duration_ms, "billing": billing}),
                    None, true);
                (resp.content, resp.prompt_tokens, resp.completion_tokens, resp.duration_ms)
            }
            Err(e) => return error_response(StatusCode::BAD_GATEWAY, &e, "external_api_error"),
        }
    } else {
        // Local Helheim inference via task queue (with auto-fallback to Groq)
        let task_type = helheim_protocol::TaskType::AiInference {
            model: model.clone(),
            prompt: prompt.clone(),
            max_tokens,
        };

        let task_id = match state.queue.submit(api_key.clone(), task_type, None).await {
            Ok(id) => id,
            Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &format!("Task submit failed: {}", e), "server_error"),
        };

        // Use shorter timeout for local inference when auto — allows fallback
        let local_timeout = if model == "auto" { 30 } else { 120 };
        let local_result = wait_for_task_timeout(&state, &task_id, local_timeout).await;

        match local_result {
            Ok((output, duration_ms)) => {
                let pt = ((prompt.len() + 3) / 4).max(1) as u32;
                let ct = ((output.len() + 3) / 4).max(1) as u32;
                state.queue.update_usage_tokens(&task_id, pt, ct).await;
                state.events.log("inference", "local", &format!("model={} tokens={}", model, ct),
                    serde_json::json!({"model": model, "tokens": ct, "duration_ms": duration_ms, "source": "local"}),
                    Some(duration_ms), true);
                (output, pt, ct, duration_ms)
            }
            Err(_) if model == "auto" => {
                // Auto-fallback to Groq when local inference fails/times out
                info!("[OPENAI] Local inference failed/timed out, falling back to groq");
                let ext_messages: Vec<serde_json::Value> = messages_with_memory.iter().map(|m| {
                    serde_json::json!({"role": m.role, "content": m.content})
                }).collect();
                match state.external.proxy_chat_completion("groq", "llama-3.3-70b-versatile", &ext_messages, max_tokens, req.temperature).await {
                    Ok(resp) => {
                        state.events.log("inference", "groq", &format!("auto-fallback model={} tokens={}", resp.model, resp.total_tokens),
                            serde_json::json!({"model": resp.model, "tokens": resp.total_tokens, "duration_ms": resp.duration_ms, "source": "groq_fallback"}),
                            Some(resp.duration_ms), true);
                        (resp.content, resp.prompt_tokens, resp.completion_tokens, resp.duration_ms)
                    }
                    Err(e) => return error_response(StatusCode::BAD_GATEWAY, &format!("Local + Groq fallback both failed: {}", e), "inference_error"),
                }
            }
            Err(err_response) => return err_response,
        }
    };

    info!("[OPENAI] Completed in {}ms: {} tokens (stream={})", duration_ms, completion_tokens, stream);

    // 4a. Log usage to usage_log table for stats/analytics
    let source = if model.contains('/') { "external" } else { "local" };
    state.api_keys.log_usage(&api_key, None, source, Some(&model),
        prompt.len(), output.len(), duration_ms, None, true);

    // Update session with token count
    state.sessions.touch(&api_key, email.as_deref(), Some(&model), Some("chat"), None, (prompt_tokens + completion_tokens) as u64);

    // 4b. PepAI Pipeline: post-process response through cognitive subsystems
    let output = {
        let mut pepai_state = state.api_keys.get_pepai_state(&api_key);
        let pepai_result = crate::pepai::pipeline::PepaiPipeline::process(
            &mut pepai_state,
            user_query,
            &output,
            0,
        );
        state.api_keys.save_pepai_state(&api_key, &pepai_state);
        state.events.log("pepai", "pipeline",
            &format!("intent={:?} auth={:.2} coh={:.1} warn={}", pepai_result.intent, pepai_result.authenticity_index, pepai_result.coherence_meter, pepai_result.observer_warning),
            serde_json::json!({
                "intent": format!("{:?}", pepai_result.intent),
                "authenticity": pepai_result.authenticity_index,
                "coherence": pepai_result.coherence_meter,
                "coherence_status": pepai_result.coherence_status,
                "observer_warning": pepai_result.observer_warning,
                "warning_type": pepai_result.warning_type,
                "pattern_entropy": pepai_result.pattern_entropy,
                "sanitized": pepai_result.sanitized,
            }),
            Some(duration_ms), true);
        pepai_result.output
    };

    // 5. Build response
    let response_id = format!("chatcmpl-{}-{:04x}", now_unix(), rand::random::<u16>());
    let response = if stream {
        // SSE streaming: split output into word chunks
        let chat_id = response_id.clone();
        let created = now_unix();
        let model_clone = model.clone();

        // Split into words, preserving spaces
        let words: Vec<String> = output.split_inclusive(' ')
            .map(|s| s.to_string())
            .collect();

        let mut events: Vec<Result<Event, std::convert::Infallible>> = Vec::new();

        // First chunk: role
        let role_chunk = StreamChunk {
            id: chat_id.clone(),
            object: "chat.completion.chunk".to_string(),
            created,
            model: model_clone.clone(),
            choices: vec![StreamChoice {
                index: 0,
                delta: StreamDelta { role: Some("assistant".to_string()), content: None },
                finish_reason: None,
            }],
        };
        events.push(Ok(Event::default().data(serde_json::to_string(&role_chunk).unwrap())));

        // Content chunks: one per word
        for word in &words {
            let chunk = StreamChunk {
                id: chat_id.clone(),
                object: "chat.completion.chunk".to_string(),
                created,
                model: model_clone.clone(),
                choices: vec![StreamChoice {
                    index: 0,
                    delta: StreamDelta { role: None, content: Some(word.clone()) },
                    finish_reason: None,
                }],
            };
            events.push(Ok(Event::default().data(serde_json::to_string(&chunk).unwrap())));
        }

        // Final chunk: finish_reason
        let done_chunk = StreamChunk {
            id: chat_id.clone(),
            object: "chat.completion.chunk".to_string(),
            created,
            model: model_clone,
            choices: vec![StreamChoice {
                index: 0,
                delta: StreamDelta { role: None, content: None },
                finish_reason: Some("stop".to_string()),
            }],
        };
        events.push(Ok(Event::default().data(serde_json::to_string(&done_chunk).unwrap())));

        // [DONE] marker
        events.push(Ok(Event::default().data("[DONE]".to_string())));

        Sse::new(stream::iter(events)).into_response()
    } else {
        // Standard JSON response
        Json(ChatCompletionResponse {
            id: response_id,
            object: "chat.completion".to_string(),
            created: now_unix(),
            model,
            choices: vec![ChatChoice {
                index: 0,
                message: ChatMessage {
                    role: "assistant".to_string(),
                    content: output,
                },
                finish_reason: "stop".to_string(),
            }],
            usage: UsageInfo {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            },
        }).into_response()
    };

    // Auto-archive: if conversation is long enough, archive older messages to memory
    if req.messages.len() >= 6 {
        let archive_count = req.messages.len().saturating_sub(4); // Keep last 4, archive the rest
        let to_archive: Vec<serde_json::Value> = req.messages[..archive_count].iter()
            .map(|m| serde_json::json!({"role": m.role, "content": m.content}))
            .collect();
        if !to_archive.is_empty() {
            state.api_keys.auto_archive_session(&api_key, None, &to_archive);
        }
    }

    response
}

// Models registry loaded from static/models.json at compile time
const MODELS_JSON: &str = include_str!("../static/models.json");

pub async fn list_models(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let now = now_unix();
    let registry: Vec<serde_json::Value> = serde_json::from_str(MODELS_JSON).unwrap_or_default();

    // Build OpenAI-compatible model list (id + aliases)
    let mut models = Vec::new();
    for entry in &registry {
        // Main model ID
        if let Some(id) = entry["id"].as_str() {
            models.push(serde_json::json!({
                "id": id,
                "object": "model",
                "created": now,
                "owned_by": "helheim",
            }));
        }
        // Aliases
        if let Some(aliases) = entry["aliases"].as_array() {
            for alias in aliases {
                if let Some(a) = alias.as_str() {
                    models.push(serde_json::json!({
                        "id": a,
                        "object": "model",
                        "created": now,
                        "owned_by": "helheim",
                    }));
                }
            }
        }
    }

    // Add ALL external provider models (not just default)
    for m in state.external.list_all_models().await {
        if let (Some(id), Some(provider)) = (m["id"].as_str(), m["provider"].as_str()) {
            models.push(serde_json::json!({
                "id": id,
                "object": "model",
                "created": now,
                "owned_by": provider,
            }));
        }
    }

    // Deduplicate (aliases may overlap with IDs)
    let mut seen = std::collections::HashSet::new();
    models.retain(|m| seen.insert(m["id"].as_str().unwrap_or("").to_string()));

    Json(serde_json::json!({
        "object": "list",
        "data": models,
    }))
}

// Full registry endpoint (for landing page / dashboard)
pub async fn model_registry(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let mut registry: Vec<serde_json::Value> = serde_json::from_str(MODELS_JSON).unwrap_or_default();

    // Add external provider models as entries
    for p in state.external.list_providers_full().await {
        if p.has_key {
            if let Some(ref dm) = p.default_model {
                registry.push(serde_json::json!({
                    "id": format!("{}/{}", p.id, dm),
                    "name": format!("{} — {}", p.name, dm),
                    "params": "API",
                    "size_gb": 0,
                    "quant": "API",
                    "category": "external",
                    "aliases": [],
                    "tag": p.name,
                    "description": format!("External API via {}", p.name),
                }));
            }
        }
    }

    Json(serde_json::Value::Array(registry))
}

pub async fn list_providers(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let providers = state.external.list_providers_full().await;
    Json(serde_json::json!({
        "providers": providers,
        "usage": "Use model format 'provider/model-name' (e.g. 'groq/llama-3.3-70b-versatile')",
    }))
}

pub async fn reload_providers(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    state.external.reload().await;
    let providers = state.external.list_providers_full().await;
    Json(serde_json::json!({
        "status": "reloaded",
        "providers": providers,
    }))
}

pub async fn save_provider(
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let id = body["id"].as_str().unwrap_or("").to_string();
    let name = body["name"].as_str().unwrap_or("").to_string();
    let api_key = body["api_key"].as_str().unwrap_or("").to_string();
    let base_url = body["base_url"].as_str().unwrap_or("").to_string();
    let default_model = body["default_model"].as_str().map(|s| s.to_string());

    if id.is_empty() || name.is_empty() || base_url.is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "id, name, and base_url are required", "missing_fields");
    }

    let config = crate::external_api::ProviderConfig {
        name,
        api_key,
        base_url,
        default_model,
        models: Vec::new(),
        enabled: true,
        priority: 0,
        fallback_to: None,
    };

    match state.external.save_provider(&id, config).await {
        Ok(()) => Json(serde_json::json!({"status": "saved", "id": id})).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e, "save_error"),
    }
}

pub async fn delete_provider(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(provider_id): axum::extract::Path<String>,
) -> Json<serde_json::Value> {
    match state.external.delete_provider(&provider_id).await {
        Ok(true) => Json(serde_json::json!({"status": "deleted", "id": provider_id})),
        Ok(false) => Json(serde_json::json!({"status": "not_found", "id": provider_id})),
        Err(e) => Json(serde_json::json!({"status": "error", "message": e})),
    }
}

pub async fn providers_page() -> axum::response::Html<String> {
    let html = include_str!("../static/providers.html")
        .replace("{{ROLE}}", "ADMIN")
        .replace("{{ROLE_COLOR}}", "red");
    axum::response::Html(html)
}

pub fn build_prompt(messages: &[ChatMessage]) -> String {
    // Serialize messages as JSON array so the node-agent can format each turn
    // in the correct model-specific chat template (ChatML, Llama3, Mistral, etc.)
    // Format: [{"role":"system","content":"..."},{"role":"user","content":"..."},...]
    let msgs: Vec<serde_json::Value> = messages.iter().map(|m| {
        serde_json::json!({"role": m.role, "content": m.content})
    }).collect();
    serde_json::to_string(&msgs).unwrap_or_else(|_| {
        // Fallback: just the last user message
        messages.iter().rev().find(|m| m.role == "user")
            .map(|m| m.content.clone()).unwrap_or_default()
    })
}
