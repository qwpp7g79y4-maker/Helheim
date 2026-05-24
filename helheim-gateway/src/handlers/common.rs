use axum::{
    response::Html,
    Json,
    http::StatusCode,
};
use std::sync::Arc;
use crate::AppState;

/// Extract API key from cookie, or from Authorization header as fallback
pub fn extract_cookie_key(headers: &axum::http::HeaderMap) -> Option<String> {
    // 1. Try cookie first
    if let Some(key) = headers.get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|c| {
                let c = c.trim();
                if c.starts_with("helheim_session=") {
                    let k = c["helheim_session=".len()..].to_string();
                    if !k.is_empty() { Some(k) } else { None }
                } else {
                    None
                }
            })
        })
    {
        return Some(key);
    }
    // 2. Fallback: Authorization: Bearer header
    extract_api_key(headers)
}

/// Extract API key from Authorization: Bearer header
pub fn extract_api_key(headers: &axum::http::HeaderMap) -> Option<String> {
    headers.get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Redirect to login page
pub fn login_redirect(page: &str) -> Html<String> {
    Html(format!(
        "<!DOCTYPE html><html><head><meta http-equiv=\"refresh\" content=\"0;url=/login?redirect={}\"></head><body></body></html>",
        page
    ))
}

/// Set session cookie
pub fn set_session_cookie(key: &str) -> axum::http::HeaderValue {
    axum::http::HeaderValue::from_str(
        &format!("helheim_session={}; Path=/; HttpOnly; SameSite=Lax; Max-Age=2592000", key)
    ).unwrap_or_else(|_| axum::http::HeaderValue::from_static(""))
}

/// Helper: submit inference task and wait for result
pub async fn run_inference(
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

/// Simple text chunking: split text into overlapping chunks
pub fn chunk_text(text: &str, chunk_size: usize, overlap: usize) -> Vec<String> {
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
pub fn retrieve_relevant_chunks(query: &str, chunks: &[String], max_chunks: usize) -> Vec<String> {
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
