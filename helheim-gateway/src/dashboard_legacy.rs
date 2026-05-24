use axum::{
    extract::{State, Query},
    response::{Html, IntoResponse},
    Json,
    http::StatusCode,
};
use serde::Deserialize;
use std::sync::Arc;
use crate::AppState;
use crate::auth::TenantUpdate;
use std::collections::HashMap;

const LANDING_HTML: &str = include_str!("../static/landing.html");
const DASHBOARD_HTML: &str = include_str!("../static/dashboard.html");
const CHAT_HTML: &str = include_str!("../static/chat.html");
const LOGIN_HTML: &str = include_str!("../static/login.html");
const TENANTS_HTML: &str = include_str!("../static/tenants.html");
const DEMO_HTML: &str = include_str!("../static/demo.html");

/// Extract API key from cookie
fn extract_cookie_key(headers: &axum::http::HeaderMap) -> Option<String> {
    headers.get(axum::http::header::COOKIE)?
        .to_str().ok()?
        .split(';')
        .find_map(|c| {
            let c = c.trim();
            if c.starts_with("helheim_session=") {
                Some(c["helheim_session=".len()..].to_string())
            } else {
                None
            }
        })
}

/// Redirect to login page
fn login_redirect(page: &str) -> Html<String> {
    Html(format!(
        "<!DOCTYPE html><html><head><meta http-equiv=\"refresh\" content=\"0;url=/login?redirect={}\"></head><body></body></html>",
        page
    ))
}

#[derive(Deserialize)]
pub struct AuthRequest {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct SaveChatRequest {
    pub id: String,
    pub title: String,
    pub messages: serde_json::Value,
    pub model: Option<String>,
    pub system_prompt: Option<String>,
}

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

#[derive(Deserialize)]
pub struct WidgetChatRequest {
    pub messages: Vec<WidgetMessage>,
}

#[derive(Deserialize, serde::Serialize, Clone)]
pub struct WidgetMessage {
    pub role: String,
    pub content: String,
}

/// Landing page
pub async fn landing_page() -> Html<&'static str> {
    Html(LANDING_HTML)
}

/// Demo page — public, no auth needed
pub async fn demo_page() -> Html<&'static str> {
    Html(DEMO_HTML)
}

#[derive(Deserialize)]
pub struct DemoChatRequest {
    pub messages: Vec<WidgetMessage>,
    pub demo_template: Option<String>,
}

/// Demo chat: POST /api/v1/demo/chat
/// Public endpoint — uses admin API key + template config for inference.
/// Rate limited to prevent abuse (no auth needed).
pub async fn demo_chat(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DemoChatRequest>,
) -> Result<axum::response::Response, (StatusCode, Json<serde_json::Value>)> {
    use crate::openai::ChatMessage;
    use crate::tools;

    // Find the template
    let tmpl_id = req.demo_template.as_deref().unwrap_or("klantenservice");
    let templates = tools::bot_templates();
    let tmpl = templates.iter().find(|t| t.id == tmpl_id).unwrap_or(&templates[0]);

    // Get admin key for inference
    let admin_key = state.api_keys.get_admin_key().await.ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({"error": "Demo niet beschikbaar"})),
    ))?;

    // Build tool-aware system prompt from template
    let enabled: Vec<String> = tmpl.tools.iter().map(|s| s.to_string()).collect();
    let system_content = tools::build_tool_prompt(tmpl.system_prompt, "", &enabled);

    let mut messages = vec![ChatMessage {
        role: "system".to_string(),
        content: system_content,
    }];
    // Only keep last 10 messages to limit context
    let start = if req.messages.len() > 10 { req.messages.len() - 10 } else { 0 };
    for m in &req.messages[start..] {
        messages.push(ChatMessage {
            role: m.role.clone(),
            content: m.content.clone(),
        });
    }

    // First inference pass
    let output = run_inference(&state, &admin_key, "auto", &messages).await?;

    // Check for tool calls
    let tool_calls = tools::parse_tool_calls(&output);
    let config = serde_json::Value::Null;

    if tool_calls.is_empty() || enabled.is_empty() {
        return Ok(axum::Json(serde_json::json!({
            "response": output,
        })).into_response());
    }

    // Execute tools
    let mut tool_results = Vec::new();
    for (tool_id, param) in &tool_calls {
        if enabled.iter().any(|e| e == tool_id) {
            let result = tools::execute_tool(tool_id, param, &config).await;
            tool_results.push(result);
        }
    }

    let tool_output: Vec<String> = tool_results.iter()
        .map(|r| {
            if r.success {
                format!("[TOOL_RESULT: {}] {}", r.tool_id, r.output)
            } else {
                format!("[TOOL_ERROR: {}] {}", r.tool_id, r.output)
            }
        })
        .collect();

    messages.push(ChatMessage {
        role: "assistant".to_string(),
        content: output.clone(),
    });
    messages.push(ChatMessage {
        role: "system".to_string(),
        content: format!(
            "Tool resultaten:\n{}\n\nVerwerk deze resultaten in een duidelijk, vriendelijk antwoord voor de gebruiker. Gebruik GEEN [TOOL_CALL] meer.",
            tool_output.join("\n")
        ),
    });

    let final_output = run_inference(&state, &admin_key, "auto", &messages).await?;

    Ok(axum::Json(serde_json::json!({
        "response": final_output,
        "tools_used": tool_results.iter().map(|r| &r.tool_id).collect::<Vec<_>>(),
    })).into_response())
}

/// Dashboard - authenticated view
pub async fn dashboard_page(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Html<String> {
    let api_key = match extract_cookie_key(&headers) {
        Some(k) if !k.is_empty() => k,
        _ => return login_redirect("dashboard"),
    };
    if !state.api_keys.validate(&api_key).await {
        return login_redirect("dashboard");
    }

    let is_admin = state.api_keys.is_admin(&api_key).await;
    let stats = state.queue.stats().await;
    let nodes = state.queue.get_nodes().await;
    let node_count = nodes.len();
    let total_gpus: u32 = nodes.values().map(|n| n.capabilities.gpu_count).sum();

    let html = DASHBOARD_HTML
        .replace("{{API_KEY}}", &api_key)
        .replace("{{ROLE}}", if is_admin { "ADMIN" } else { "STANDARD" })
        .replace("{{ROLE_COLOR}}", if is_admin { "red" } else { "green" })
        .replace("{{NODE_COUNT}}", &node_count.to_string())
        .replace("{{GPU_COUNT}}", &total_gpus.to_string())
        .replace("{{COMPLETED}}", &stats.completed.to_string())
        .replace("{{QUEUED}}", &(stats.queued + stats.active).to_string());

    Html(html)
}

/// Chat interface - authenticated view
pub async fn chat_page(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Html<String> {
    let api_key = match extract_cookie_key(&headers) {
        Some(k) if !k.is_empty() => k,
        _ => return login_redirect("chat"),
    };
    if !state.api_keys.validate(&api_key).await {
        return login_redirect("chat");
    }

    let html = CHAT_HTML.replace("{{API_KEY}}", &api_key);
    Html(html)
}

/// Tenants management page
pub async fn tenants_page(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Html<String> {
    let api_key = match extract_cookie_key(&headers) {
        Some(k) if !k.is_empty() => k,
        _ => return login_redirect("tenants"),
    };
    if !state.api_keys.validate(&api_key).await {
        return login_redirect("tenants");
    }
    let html = TENANTS_HTML.replace("{{API_KEY}}", &api_key);
    Html(html)
}

/// Login page
pub async fn login_page() -> Html<&'static str> {
    Html(LOGIN_HTML)
}

fn set_session_cookie(key: &str) -> axum::http::HeaderValue {
    axum::http::HeaderValue::from_str(
        &format!("helheim_session={}; Path=/; HttpOnly; SameSite=Lax; Max-Age=2592000", key)
    ).unwrap_or_else(|_| axum::http::HeaderValue::from_static(""))
}

/// Register endpoint: POST /api/v1/register
pub async fn register_user(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AuthRequest>,
) -> Result<axum::response::Response, (StatusCode, Json<serde_json::Value>)> {
    match state.api_keys.register_user(&req.email, &req.password) {
        Ok(api_key) => {
            let mut resp = axum::Json(serde_json::json!({
                "status": "ok",
                "api_key": api_key,
                "email": req.email.trim().to_lowercase()
            })).into_response();
            resp.headers_mut().insert(axum::http::header::SET_COOKIE, set_session_cookie(&api_key));
            Ok(resp)
        }
        Err(msg) => Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "status": "error",
            "message": msg
        })))),
    }
}

/// Login endpoint: POST /api/v1/login
pub async fn login_user(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AuthRequest>,
) -> Result<axum::response::Response, (StatusCode, Json<serde_json::Value>)> {
    match state.api_keys.login_user(&req.email, &req.password) {
        Ok(api_key) => {
            let mut resp = axum::Json(serde_json::json!({
                "status": "ok",
                "api_key": api_key
            })).into_response();
            resp.headers_mut().insert(axum::http::header::SET_COOKIE, set_session_cookie(&api_key));
            Ok(resp)
        }
        Err(msg) => Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({
            "status": "error",
            "message": msg
        })))),
    }
}

/// Logout: GET /logout — clears cookie and redirects to login
pub async fn logout() -> axum::response::Response {
    let mut resp = axum::response::Redirect::to("/login").into_response();
    let clear = axum::http::HeaderValue::from_static("helheim_session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0");
    resp.headers_mut().insert(axum::http::header::SET_COOKIE, clear);
    resp
}

// === Chat persistence ===

fn extract_api_key(headers: &axum::http::HeaderMap) -> Option<String> {
    headers.get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Save/update a chat: POST /api/v1/chats
pub async fn save_chat(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<SaveChatRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }
    let messages_str = serde_json::to_string(&req.messages).unwrap_or_default();
    state.api_keys.save_chat(&api_key, &req.id, &req.title, &messages_str, req.model.as_deref(), req.system_prompt.as_deref())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"status": "ok", "id": req.id})))
}

/// List chats: GET /api/v1/chats
pub async fn list_chats(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }
    let chats = state.api_keys.list_chats(&api_key);
    Ok(Json(serde_json::json!({"chats": chats})))
}

/// Get a chat: GET /api/v1/chats/:chat_id
pub async fn get_chat(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(chat_id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }
    state.api_keys.get_chat(&api_key, &chat_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

// === User management (admin) ===

/// List all users: GET /api/v1/users (admin only)
pub async fn list_users(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.is_admin(&api_key).await { return Err(StatusCode::FORBIDDEN); }
    let users = state.api_keys.list_users();
    Ok(Json(serde_json::json!({"users": users, "count": users.len()})))
}

/// Delete user: DELETE /api/v1/users/:user_id (admin only)
pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(user_id): axum::extract::Path<i64>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.is_admin(&api_key).await { return Err(StatusCode::FORBIDDEN); }
    if state.api_keys.delete_user(user_id) {
        Ok(Json(serde_json::json!({"status": "ok"})))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

// === Tenant endpoints ===

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

/// Widget chat: POST /api/v1/widget/:tenant_id/chat
/// Public endpoint — no API key needed from the end user.
/// Injects tenant FAQ + system prompt, uses tenant's API key for inference.
/// Supports tool calls: if the AI response contains [TOOL_CALL: ...], executes the tool
/// and feeds the result back for a second pass.
pub async fn widget_chat(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
    Json(req): Json<WidgetChatRequest>,
) -> Result<axum::response::Response, (StatusCode, Json<serde_json::Value>)> {
    use crate::openai::ChatMessage;
    use crate::tools;

    let tenant = state.api_keys.get_tenant_public(&tenant_id)
        .ok_or((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Bot not found"}))))?;

    if !tenant.active {
        return Err((StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Bot is disabled"}))));
    }

    // Parse enabled tools from tenant config
    let enabled_tools: Vec<String> = serde_json::from_str(&tenant.tools).unwrap_or_default();
    let tool_config: serde_json::Value = serde_json::from_str(&tenant.tool_config).unwrap_or_default();

    // RAG: retrieve relevant document chunks if any exist
    let all_chunks = state.api_keys.get_all_chunks(&tenant_id);
    let user_query = req.messages.last().map(|m| m.content.as_str()).unwrap_or("");
    let rag_context = if !all_chunks.is_empty() {
        let relevant = retrieve_relevant_chunks(user_query, &all_chunks, 3);
        if !relevant.is_empty() {
            state.events.log("rag_query", "widget", &format!("tenant={} chunks={}", tenant_id, relevant.len()),
                serde_json::json!({"tenant": tenant_id, "query": &user_query[..user_query.len().min(100)], "chunks_found": relevant.len()}),
                None, true);
            format!("\n\nRelevante documenten:\n{}", relevant.join("\n---\n"))
        } else { String::new() }
    } else { String::new() };

    // Build tool-aware system prompt with RAG context
    let faq_plus_rag = format!("{}{}", tenant.faq, rag_context);
    let system_content = tools::build_tool_prompt(&tenant.system_prompt, &faq_plus_rag, &enabled_tools);

    // Build messages: system + user conversation
    let mut messages = vec![ChatMessage {
        role: "system".to_string(),
        content: system_content,
    }];
    for m in &req.messages {
        messages.push(ChatMessage {
            role: m.role.clone(),
            content: m.content.clone(),
        });
    }

    // First inference pass
    let output = run_inference(&state, &tenant.api_key, &tenant.model, &messages).await?;

    // Check for tool calls in the response
    let tool_calls = tools::parse_tool_calls(&output);

    if tool_calls.is_empty() || enabled_tools.is_empty() {
        // No tools needed — return directly
        state.api_keys.increment_tenant_messages(&tenant_id);
        return Ok(axum::Json(serde_json::json!({
            "response": output,
            "tenant": tenant.name,
        })).into_response());
    }

    // Execute tools
    let mut tool_results = Vec::new();
    for (tool_id, param) in &tool_calls {
        if enabled_tools.contains(tool_id) {
            let t_start = std::time::Instant::now();
            let result = tools::execute_tool(tool_id, param, &tool_config).await;
            let t_ms = t_start.elapsed().as_millis() as u64;
            state.events.log(
                if result.success { "tool_result" } else { "error" },
                "widget", &format!("tool={} tenant={}", tool_id, tenant_id),
                serde_json::json!({"tool": tool_id, "param": param, "success": result.success, "output_len": result.output.len(), "tenant": tenant_id}),
                Some(t_ms), result.success,
            );
            tool_results.push(result);
        }
    }

    // Build tool results into a follow-up message
    let tool_output: Vec<String> = tool_results.iter()
        .map(|r| {
            if r.success {
                format!("[TOOL_RESULT: {}] {}", r.tool_id, r.output)
            } else {
                format!("[TOOL_ERROR: {}] {}", r.tool_id, r.output)
            }
        })
        .collect();

    // Add the AI's first response + tool results as context for second pass
    messages.push(ChatMessage {
        role: "assistant".to_string(),
        content: output.clone(),
    });
    messages.push(ChatMessage {
        role: "system".to_string(),
        content: format!(
            "Tool resultaten:\n{}\n\nVerwerk deze resultaten in een duidelijk, vriendelijk antwoord voor de gebruiker. Gebruik GEEN [TOOL_CALL] meer.",
            tool_output.join("\n")
        ),
    });

    // Second inference pass with tool results
    let final_output = run_inference(&state, &tenant.api_key, &tenant.model, &messages).await?;

    state.api_keys.increment_tenant_messages(&tenant_id);
    Ok(axum::Json(serde_json::json!({
        "response": final_output,
        "tenant": tenant.name,
        "tools_used": tool_results.iter().map(|r| &r.tool_id).collect::<Vec<_>>(),
    })).into_response())
}

/// Helper: submit inference task and wait for result
async fn run_inference(
    state: &Arc<AppState>,
    api_key: &str,
    model: &str,
    messages: &[crate::openai::ChatMessage],
) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    let prompt = crate::openai::build_prompt(messages);
    let prompt_len = prompt.len();
    let task_type = helheim_protocol::TaskType::AiInference {
        model: model.to_string(),
        prompt,
        max_tokens: 512,
    };

    let task_id = state.queue.submit(api_key.to_string(), task_type, None).await
        .map_err(|e| {
            state.events.log("error", "inference", &format!("Submit failed: {}", e),
                serde_json::json!({"model": model}), None, false);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("Task failed: {}", e)})))
        })?;

    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(120);
    loop {
        if start.elapsed() > timeout {
            state.events.log("error", "inference", "Timeout after 120s",
                serde_json::json!({"model": model, "task_id": task_id}), Some(120000), false);
            return Err((StatusCode::GATEWAY_TIMEOUT, Json(serde_json::json!({"error": "Timeout"}))));
        }
        if let Some(task) = state.queue.get_task(&task_id).await {
            match task.status {
                helheim_protocol::TaskStatus::Completed => {
                    let output = task.result.as_ref().map(|r| r.output.clone()).unwrap_or_default();
                    let ms = start.elapsed().as_millis() as u64;
                    state.events.log("inference", "api", &format!("model={} prompt={}ch output={}ch", model, prompt_len, output.len()),
                        serde_json::json!({"model": model, "task_id": task_id, "prompt_len": prompt_len, "output_len": output.len(), "latency_ms": ms}),
                        Some(ms), true);
                    return Ok(output);
                }
                helheim_protocol::TaskStatus::Failed => {
                    let err = task.result.as_ref().and_then(|r| r.error.clone()).unwrap_or_else(|| "Inference failed".to_string());
                    let ms = start.elapsed().as_millis() as u64;
                    state.events.log("error", "inference", &format!("Failed: {}", err),
                        serde_json::json!({"model": model, "task_id": task_id, "error": err}), Some(ms), false);
                    return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": err}))));
                }
                _ => { tokio::time::sleep(tokio::time::Duration::from_millis(100)).await; }
            }
        } else {
            state.events.log("error", "inference", "Task lost",
                serde_json::json!({"model": model, "task_id": task_id}), None, false);
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Task lost"}))));
        }
    }
}

/// List available tools: GET /api/v1/tools
pub async fn list_available_tools() -> Json<serde_json::Value> {
    let tools = crate::tools::available_tools();
    Json(serde_json::json!({"tools": tools}))
}

// === Debug / Admin ===

const DEBUG_HTML: &str = include_str!("../static/debug.html");

/// Debug page: GET /debug (admin only)
pub async fn debug_page(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Html<String> {
    let api_key = match extract_cookie_key(&headers) {
        Some(k) if !k.is_empty() => k,
        _ => return login_redirect("debug"),
    };
    if !state.api_keys.is_admin(&api_key).await {
        return Html("<h1>Admin only</h1>".to_string());
    }
    let html = DEBUG_HTML.replace("{{API_KEY}}", &api_key);
    Html(html)
}

/// Debug events API: GET /api/v1/debug/events?limit=100&kind=inference
pub async fn debug_events(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.is_admin(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }
    let limit = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(100usize);
    let kind = params.get("kind").map(|s| s.as_str());
    let events = state.events.recent(limit, kind);
    let counters = state.events.counters();
    Ok(Json(serde_json::json!({
        "events": events,
        "counters": counters,
    })))
}

/// Debug counters API: GET /api/v1/debug/counters
pub async fn debug_counters(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.is_admin(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }
    let counters = state.events.counters();
    let stats = state.queue.stats().await;
    let nodes = state.queue.get_nodes().await;
    Ok(Json(serde_json::json!({
        "counters": counters,
        "queue": {"queued": stats.queued, "active": stats.active, "completed": stats.completed, "nodes_online": stats.nodes_online},
        "nodes": nodes.len(),
    })))
}

// === Usage API ===

/// Usage stats: GET /api/v1/usage/stats (admin sees all, user sees own)
pub async fn usage_stats(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }
    let is_admin = state.api_keys.is_admin(&api_key).await;
    let stats = if is_admin {
        state.api_keys.get_usage_stats(None)
    } else {
        state.api_keys.get_usage_stats(Some(&api_key))
    };
    Ok(Json(stats))
}

/// Per-tenant usage: GET /api/v1/usage/tenants
pub async fn usage_by_tenant(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }
    let tenants = state.api_keys.get_tenant_usage(&api_key);
    Ok(Json(serde_json::json!({"tenants": tenants})))
}

// === RAG / Document API ===

#[derive(Deserialize)]
pub struct UploadDocRequest {
    pub tenant_id: String,
    pub filename: String,
    pub content: String,
}

/// Upload document: POST /api/v1/documents
pub async fn upload_document(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<UploadDocRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let api_key = extract_api_key(&headers).ok_or((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "unauthorized"}))))?;
    if !state.api_keys.validate(&api_key).await {
        return Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "invalid key"}))));
    }

    // Chunk the document
    let chunks = chunk_text(&req.content, 500, 50);
    let chunk_count = chunks.len();
    let chunks_json = serde_json::to_string(&chunks).unwrap_or_else(|_| "[]".to_string());

    let doc_id = state.api_keys.add_document(&req.tenant_id, &req.filename, &req.content, &chunks_json, chunk_count)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))))?;

    state.events.log("rag_query", "upload", &format!("doc={} chunks={} tenant={}", req.filename, chunk_count, req.tenant_id),
        serde_json::json!({"filename": req.filename, "chunks": chunk_count, "tenant_id": req.tenant_id, "doc_id": doc_id}),
        None, true);

    Ok(Json(serde_json::json!({"status": "ok", "doc_id": doc_id, "chunks": chunk_count})))
}

/// List documents: GET /api/v1/documents/:tenant_id
pub async fn list_documents(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }
    let docs = state.api_keys.list_documents(&tenant_id);
    Ok(Json(serde_json::json!({"documents": docs})))
}

/// Delete document: DELETE /api/v1/documents/:tenant_id/:doc_id
pub async fn delete_document(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    axum::extract::Path((tenant_id, doc_id)): axum::extract::Path<(String, i64)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }
    if state.api_keys.delete_document(&tenant_id, doc_id) {
        Ok(Json(serde_json::json!({"status": "ok"})))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// Simple text chunking: split text into overlapping chunks
fn chunk_text(text: &str, chunk_size: usize, overlap: usize) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() { return vec![]; }
    if words.len() <= chunk_size { return vec![words.join(" ")]; }

    let mut chunks = Vec::new();
    let mut start = 0;
    while start < words.len() {
        let end = (start + chunk_size).min(words.len());
        chunks.push(words[start..end].join(" "));
        if end >= words.len() { break; }
        start += chunk_size - overlap;
    }
    chunks
}

/// Simple keyword-based retrieval (no embeddings needed — works with any model)
fn retrieve_relevant_chunks(query: &str, chunks: &[String], max_chunks: usize) -> Vec<String> {
    let query_words: Vec<String> = query.to_lowercase()
        .split_whitespace()
        .filter(|w| w.len() > 2)
        .map(|w| w.to_string())
        .collect();

    if query_words.is_empty() || chunks.is_empty() {
        return chunks.iter().take(max_chunks).cloned().collect();
    }

    let mut scored: Vec<(usize, f64)> = chunks.iter().enumerate().map(|(i, chunk)| {
        let chunk_lower = chunk.to_lowercase();
        let score: f64 = query_words.iter()
            .filter(|w| chunk_lower.contains(w.as_str()))
            .count() as f64;
        // Bonus for exact phrase match
        let phrase_bonus = if chunk_lower.contains(&query.to_lowercase()) { 3.0 } else { 0.0 };
        (i, score + phrase_bonus)
    }).collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.iter()
        .filter(|(_, score)| *score > 0.0)
        .take(max_chunks)
        .map(|(i, _)| chunks[*i].clone())
        .collect()
}

// === Agent Workflows ===

#[derive(Deserialize)]
pub struct AgentRequest {
    pub task: String,
    pub tenant_id: Option<String>,
}

/// Agent endpoint: POST /api/v1/agent
/// Multi-step reasoning: plan → execute tools → synthesize
pub async fn run_agent(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<AgentRequest>,
) -> Result<axum::response::Response, (StatusCode, Json<serde_json::Value>)> {
    use crate::openai::ChatMessage;
    use crate::tools;

    let api_key = extract_api_key(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "unauthorized"}))))?;
    if !state.api_keys.validate(&api_key).await {
        return Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "invalid key"}))));
    }

    let all_tools = tools::available_tools();
    let tool_list: String = all_tools.iter()
        .map(|t| format!("- {}(param): {}", t.id, t.description))
        .collect::<Vec<_>>()
        .join("\n");

    // Step 1: Planning — ask the AI to create a step-by-step plan
    let plan_prompt = format!(
        "Je bent een AI agent die taken uitvoert in meerdere stappen. Je hebt deze tools:\n{}\n\n\
        Taak: {}\n\n\
        Maak een plan met maximaal 5 stappen. Elke stap is een [TOOL_CALL: tool(param)] of een analyse-stap.\n\
        Geef ALLEEN het plan, geen uitleg. Format:\n\
        STEP 1: [TOOL_CALL: tool(param)]\n\
        STEP 2: [TOOL_CALL: tool(param)]\n\
        STEP 3: Analyseer resultaten\n\
        ...\n\
        Als je geen tools nodig hebt, geef dan direct antwoord.",
        tool_list, req.task
    );

    let plan_messages = vec![ChatMessage {
        role: "system".to_string(),
        content: plan_prompt,
    }, ChatMessage {
        role: "user".to_string(),
        content: req.task.clone(),
    }];

    state.events.log("agent", "api", &format!("Planning: {}", &req.task[..req.task.len().min(80)]),
        serde_json::json!({"task": req.task}), None, true);

    let plan_output = run_inference(&state, &api_key, "auto", &plan_messages).await?;

    // Step 2: Execute tool calls from the plan
    let tool_calls = tools::parse_tool_calls(&plan_output);
    let config = serde_json::Value::Null;
    let mut all_results = Vec::new();
    let mut steps_log = Vec::new();

    steps_log.push(serde_json::json!({"step": "plan", "output": plan_output}));

    for (tool_id, param) in &tool_calls {
        let t_start = std::time::Instant::now();
        let result = tools::execute_tool(tool_id, param, &config).await;
        let t_ms = t_start.elapsed().as_millis() as u64;

        state.events.log(
            if result.success { "tool_result" } else { "error" },
            "agent", &format!("tool={} param={}", tool_id, &param[..param.len().min(50)]),
            serde_json::json!({"tool": tool_id, "param": param, "success": result.success, "latency_ms": t_ms}),
            Some(t_ms), result.success,
        );

        steps_log.push(serde_json::json!({
            "step": "tool",
            "tool": tool_id,
            "param": param,
            "success": result.success,
            "output": &result.output[..result.output.len().min(500)],
            "latency_ms": t_ms,
        }));

        all_results.push(result);
    }

    // Step 3: Synthesize — feed all results back for a final answer
    let results_text: String = all_results.iter()
        .map(|r| {
            if r.success {
                format!("[RESULT: {}] {}", r.tool_id, r.output)
            } else {
                format!("[ERROR: {}] {}", r.tool_id, r.output)
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let synth_messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: "Je bent een AI agent. Verwerk de onderstaande resultaten tot een duidelijk, compleet antwoord. Gebruik geen [TOOL_CALL] meer.".to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: format!("Oorspronkelijke taak: {}\n\nPlan:\n{}\n\nResultaten:\n{}", req.task, plan_output, results_text),
        },
    ];

    let final_output = run_inference(&state, &api_key, "auto", &synth_messages).await?;

    steps_log.push(serde_json::json!({"step": "synthesis", "output": final_output}));

    state.events.log("agent", "api", &format!("Completed: {} tools, task={}", all_results.len(), &req.task[..req.task.len().min(50)]),
        serde_json::json!({"task": req.task, "tools_used": all_results.len(), "steps": steps_log.len()}),
        None, true);

    // Log usage
    state.api_keys.log_usage(&api_key, req.tenant_id.as_deref(), "agent", Some("auto"),
        req.task.len(), final_output.len(), 0, None, true);

    Ok(axum::Json(serde_json::json!({
        "response": final_output,
        "plan": plan_output,
        "steps": steps_log,
        "tools_used": all_results.iter().map(|r| &r.tool_id).collect::<Vec<_>>(),
    })).into_response())
}

/// Widget JS: GET /widget/:tenant_id.js
/// Returns embeddable JavaScript that creates a chat bubble on any website.
pub async fn widget_js(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
) -> axum::response::Response {
    // Strip .js suffix if present
    let tid = tenant_id.trim_end_matches(".js");
    let tenant = match state.api_keys.get_tenant_public(tid) {
        Some(t) if t.active => t,
        _ => {
            return (StatusCode::NOT_FOUND, "// Bot not found").into_response();
        }
    };

    let js = WIDGET_JS_TEMPLATE
        .replace("{{TENANT_ID}}", &tenant.id)
        .replace("{{TENANT_NAME}}", &tenant.name)
        .replace("{{WELCOME}}", &tenant.welcome_message)
        .replace("{{COLOR_PRIMARY}}", &tenant.color_primary)
        .replace("{{COLOR_BG}}", &tenant.color_bg)
        .replace("{{COLOR_TEXT}}", &tenant.color_text)
        .replace("{{API_BASE}}", "https://api.helheim-ai.dev");

    (
        [(axum::http::header::CONTENT_TYPE, "application/javascript; charset=utf-8")],
        js,
    ).into_response()
}

const WIDGET_JS_TEMPLATE: &str = include_str!("../static/widget.js");

/// Delete a chat: DELETE /api/v1/chats/:chat_id
pub async fn delete_chat(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(chat_id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = extract_api_key(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.api_keys.validate(&api_key).await { return Err(StatusCode::UNAUTHORIZED); }
    if state.api_keys.delete_chat(&api_key, &chat_id) {
        Ok(Json(serde_json::json!({"status": "ok"})))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}
