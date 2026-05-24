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
use super::widget_api::WidgetMessage;

#[derive(Deserialize)]
pub struct DemoChatRequest {
    pub messages: Vec<WidgetMessage>,
    pub demo_template: Option<String>,
}

/// Demo chat: POST /api/v1/demo/chat
pub async fn demo_chat(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DemoChatRequest>,
) -> Result<axum::response::Response, (StatusCode, Json<serde_json::Value>)> {
    use crate::openai::ChatMessage;
    use crate::tools;

    let tmpl_id = req.demo_template.as_deref().unwrap_or("klantenservice");
    let templates = tools::bot_templates();
    let tmpl = templates.iter().find(|t| t.id == tmpl_id).unwrap_or(&templates[0]);

    let admin_key = state.api_keys.get_admin_key().await.ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({"error": "Demo niet beschikbaar"})),
    ))?;

    let enabled: Vec<String> = tmpl.tools.iter().map(|s| s.to_string()).collect();
    let system_content = tools::build_tool_prompt(tmpl.system_prompt, "", &enabled);

    let mut messages = vec![ChatMessage {
        role: "system".to_string(),
        content: system_content,
    }];
    let start = if req.messages.len() > 10 { req.messages.len() - 10 } else { 0 };
    for m in &req.messages[start..] {
        messages.push(ChatMessage {
            role: m.role.clone(),
            content: m.content.clone(),
        });
    }

    let output = run_inference(&state, &admin_key, "auto", &messages).await?;

    let tool_calls = tools::parse_tool_calls(&output);
    let config = serde_json::Value::Null;

    if tool_calls.is_empty() || enabled.is_empty() {
        return Ok(axum::Json(serde_json::json!({
            "response": output,
        })).into_response());
    }

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
