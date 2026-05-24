use axum::{
    extract::State,
    response::IntoResponse,
    Json,
    http::StatusCode,
};
use serde::Deserialize;
use std::sync::Arc;
use crate::AppState;
use super::common::{extract_api_key, run_inference};

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

    // Step 1: Planning
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

    // Step 3: Synthesize
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

    state.api_keys.log_usage(&api_key, req.tenant_id.as_deref(), "agent", Some("auto"),
        req.task.len(), final_output.len(), 0, None, true);

    Ok(axum::Json(serde_json::json!({
        "response": final_output,
        "plan": plan_output,
        "steps": steps_log,
        "tools_used": all_results.iter().map(|r| &r.tool_id).collect::<Vec<_>>(),
    })).into_response())
}
