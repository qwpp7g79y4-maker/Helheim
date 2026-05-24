use axum::{
    extract::State,
    response::IntoResponse,
    Json,
    http::StatusCode,
};
use serde::Deserialize;
use std::sync::Arc;
use crate::AppState;
use super::common::run_inference;

#[derive(Deserialize)]
pub struct WidgetChatRequest {
    pub messages: Vec<WidgetMessage>,
}

#[derive(Deserialize, serde::Serialize, Clone)]
pub struct WidgetMessage {
    pub role: String,
    pub content: String,
}

/// Widget chat: POST /api/v1/widget/:tenant_id/chat
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

    // RAG: hybrid retrieval (keyword + vector) for document chunks
    let user_query = req.messages.last().map(|m| m.content.as_str()).unwrap_or("");
    let query_embedding = state.external.generate_embedding(user_query).await;
    let chunks_with_emb = state.api_keys.get_all_chunks_with_embeddings(&tenant_id);
    let rag_context = if !chunks_with_emb.is_empty() {
        let relevant = crate::external_api::retrieve_hybrid(
            user_query, query_embedding.as_deref(), &chunks_with_emb, 3,
        );
        if !relevant.is_empty() {
            let mode = if query_embedding.is_some() { "hybrid" } else { "keyword" };
            state.events.log("rag_query", "widget", &format!("tenant={} chunks={} mode={}", tenant_id, relevant.len(), mode),
                serde_json::json!({"tenant": tenant_id, "query": &user_query[..user_query.len().min(100)], "chunks_found": relevant.len(), "mode": mode}),
                None, true);
            format!("\n\nRelevante documenten:\n{}", relevant.join("\n---\n"))
        } else { String::new() }
    } else { String::new() };

    // Memory recall: hybrid scoring (keyword + vector)
    let memory_context = if !user_query.is_empty() {
        let memories = state.api_keys.recall_memories_hybrid(
            &tenant.api_key, Some(&tenant_id), user_query, query_embedding.as_deref(), 3,
        );
        if !memories.is_empty() {
            state.events.log("memory_recall", "widget", &format!("tenant={} memories={}", tenant_id, memories.len()),
                serde_json::json!({"tenant": tenant_id, "query": &user_query[..user_query.len().min(100)], "memories_found": memories.len()}),
                None, true);
            let mem_texts: Vec<String> = memories.iter()
                .filter_map(|m| m["content"].as_str().map(|s| s.to_string()))
                .collect();
            format!("\n\nEerdere gesprekken/kennis:\n{}", mem_texts.join("\n---\n"))
        } else { String::new() }
    } else { String::new() };

    // Build tool-aware system prompt with RAG context + memory context
    let faq_plus_rag = format!("{}{}{}", tenant.faq, rag_context, memory_context);
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

const WIDGET_JS_TEMPLATE: &str = include_str!("../../static/widget.js");

/// Widget JS: GET /widget/:tenant_id
pub async fn widget_js(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
) -> axum::response::Response {
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
