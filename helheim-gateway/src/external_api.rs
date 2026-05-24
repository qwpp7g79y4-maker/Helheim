use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Model info within a provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub context_window: u32,
    pub cost_per_1k_input: f64,
    pub cost_per_1k_output: f64,
    pub price_per_1k_input: f64,
    pub price_per_1k_output: f64,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub category: String,
}

fn default_true() -> bool { true }

/// External API provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub api_key: String,
    pub base_url: String,
    pub default_model: Option<String>,
    #[serde(default)]
    pub models: Vec<ModelInfo>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub priority: u32,
    #[serde(default)]
    pub fallback_to: Option<String>,
}

/// Runtime health status per provider
#[derive(Debug, Clone, Serialize)]
pub struct ProviderHealth {
    pub provider_id: String,
    pub healthy: bool,
    pub last_check: u64,
    pub last_latency_ms: u64,
    pub total_requests: u64,
    pub total_errors: u64,
    pub error_rate: f64,
    pub avg_latency_ms: f64,
}

/// Routing strategy
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoutingStrategy {
    Cheapest,
    Fastest,
    Best,
    Priority,
}

/// Feature flags for the platform
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlags {
    pub credits_enabled: bool,
    pub byok_enabled: bool,
    pub smart_routing_enabled: bool,
    pub caching_enabled: bool,
    pub web_search_enabled: bool,
    pub image_gen_enabled: bool,
    pub code_exec_enabled: bool,
    pub rag_enabled: bool,
    pub memory_enabled: bool,
    pub analytics_enabled: bool,
    pub demo_enabled: bool,
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self {
            credits_enabled: true,
            byok_enabled: true,
            smart_routing_enabled: true,
            caching_enabled: false,
            web_search_enabled: false,
            image_gen_enabled: false,
            code_exec_enabled: false,
            rag_enabled: true,
            memory_enabled: true,
            analytics_enabled: true,
            demo_enabled: true,
        }
    }
}

/// Manages external API providers (Groq, OpenAI, Google, Grok, etc.)
pub struct ExternalProviders {
    providers: Arc<RwLock<HashMap<String, ProviderConfig>>>,
    health: Arc<RwLock<HashMap<String, ProviderHealth>>>,
    features: Arc<RwLock<FeatureFlags>>,
    client: Client,
    config_path: String,
}

impl ExternalProviders {
    pub fn new() -> Self {
        let config_path = Self::find_config_path();
        let providers = Self::load_from_file(&config_path);
        let features = Self::load_features();

        let count = providers.len();
        if count > 0 {
            info!("[EXTERNAL] Loaded {} providers from {}", count, config_path);
            for (name, cfg) in &providers {
                let model_count = cfg.models.len();
                info!("[EXTERNAL]   {} -> {} ({} models, key: {}...)", name, cfg.base_url, model_count, &cfg.api_key[..cfg.api_key.len().min(12)]);
            }
        } else {
            info!("[EXTERNAL] No providers configured. Create {} to add external APIs.", config_path);
        }

        // Init health for all providers
        let mut health = HashMap::new();
        for (id, _) in &providers {
            health.insert(id.clone(), ProviderHealth {
                provider_id: id.clone(),
                healthy: true,
                last_check: 0,
                last_latency_ms: 0,
                total_requests: 0,
                total_errors: 0,
                error_rate: 0.0,
                avg_latency_ms: 0.0,
            });
        }

        Self {
            providers: Arc::new(RwLock::new(providers)),
            health: Arc::new(RwLock::new(health)),
            features: Arc::new(RwLock::new(features)),
            client: Client::new(),
            config_path,
        }
    }

    fn load_features() -> FeatureFlags {
        let paths = ["/etc/helheim/features.json", "features.json"];
        for p in &paths {
            if let Ok(content) = std::fs::read_to_string(p) {
                if let Ok(f) = serde_json::from_str::<FeatureFlags>(&content) {
                    info!("[FEATURES] Loaded from {}", p);
                    return f;
                }
            }
        }
        FeatureFlags::default()
    }

    pub async fn get_features(&self) -> FeatureFlags {
        self.features.read().await.clone()
    }

    pub async fn set_features(&self, flags: FeatureFlags) -> Result<(), String> {
        let json = serde_json::to_string_pretty(&flags).map_err(|e| e.to_string())?;
        // Try writable paths in order
        let paths = ["/etc/helheim/features.json", "features.json"];
        let mut saved = false;
        for path in &paths {
            match std::fs::write(path, &json) {
                Ok(_) => {
                    info!("[FEATURES] Saved to {}", path);
                    saved = true;
                    break;
                }
                Err(e) => {
                    tracing::warn!("[FEATURES] Cannot write {}: {}", path, e);
                }
            }
        }
        if !saved {
            return Err("Failed to write features.json to any location".to_string());
        }
        *self.features.write().await = flags;
        Ok(())
    }

    fn find_config_path() -> String {
        // Check multiple locations
        let paths = [
            "/etc/helheim/providers.json",
            "providers.json",
        ];
        for p in &paths {
            if std::path::Path::new(p).exists() {
                return p.to_string();
            }
        }
        // Default: create in /etc/helheim/ on server, local otherwise
        if std::path::Path::new("/etc/helheim").exists() {
            "/etc/helheim/providers.json".to_string()
        } else {
            "providers.json".to_string()
        }
    }

    fn load_from_file(path: &str) -> HashMap<String, ProviderConfig> {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                match serde_json::from_str::<HashMap<String, ProviderConfig>>(&content) {
                    Ok(providers) => providers,
                    Err(e) => {
                        warn!("[EXTERNAL] Failed to parse {}: {}", path, e);
                        HashMap::new()
                    }
                }
            }
            Err(_) => HashMap::new(),
        }
    }

    /// Reload providers from config file
    pub async fn reload(&self) {
        let providers = Self::load_from_file(&self.config_path);
        let count = providers.len();
        // Init health for new providers
        let mut health = self.health.write().await;
        for (id, _) in &providers {
            health.entry(id.clone()).or_insert(ProviderHealth {
                provider_id: id.clone(),
                healthy: true,
                last_check: 0,
                last_latency_ms: 0,
                total_requests: 0,
                total_errors: 0,
                error_rate: 0.0,
                avg_latency_ms: 0.0,
            });
        }
        drop(health);
        *self.providers.write().await = providers;
        info!("[EXTERNAL] Reloaded {} providers from {}", count, self.config_path);
    }

    /// Get health status for all providers
    pub async fn get_health(&self) -> Vec<ProviderHealth> {
        self.health.read().await.values().cloned().collect()
    }

    /// Record a request result for health tracking
    pub async fn record_request(&self, provider_id: &str, latency_ms: u64, success: bool) {
        let mut health = self.health.write().await;
        let h = health.entry(provider_id.to_string()).or_insert(ProviderHealth {
            provider_id: provider_id.to_string(),
            healthy: true,
            last_check: 0,
            last_latency_ms: 0,
            total_requests: 0,
            total_errors: 0,
            error_rate: 0.0,
            avg_latency_ms: 0.0,
        });
        h.total_requests += 1;
        if !success { h.total_errors += 1; }
        h.last_latency_ms = latency_ms;
        h.last_check = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
        h.error_rate = if h.total_requests > 0 { h.total_errors as f64 / h.total_requests as f64 } else { 0.0 };
        // Exponential moving average for latency
        if h.avg_latency_ms == 0.0 {
            h.avg_latency_ms = latency_ms as f64;
        } else {
            h.avg_latency_ms = h.avg_latency_ms * 0.8 + latency_ms as f64 * 0.2;
        }
        // Mark unhealthy if error rate > 50% over last 10+ requests
        h.healthy = !(h.total_requests >= 10 && h.error_rate > 0.5);
    }

    /// List all models across all providers (for model selector)
    pub async fn list_all_models(&self) -> Vec<serde_json::Value> {
        let providers = self.providers.read().await;
        let health = self.health.read().await;
        let mut models = Vec::new();
        for (id, cfg) in providers.iter() {
            if !cfg.enabled { continue; }
            let h = health.get(id);
            let healthy = h.map(|h| h.healthy).unwrap_or(true);
            for m in &cfg.models {
                if !m.enabled { continue; }
                models.push(serde_json::json!({
                    "id": format!("{}/{}", id, m.id),
                    "provider": id,
                    "provider_name": cfg.name,
                    "model": m.id,
                    "name": m.name,
                    "context_window": m.context_window,
                    "cost_per_1k_input": m.cost_per_1k_input,
                    "cost_per_1k_output": m.cost_per_1k_output,
                    "price_per_1k_input": m.price_per_1k_input,
                    "price_per_1k_output": m.price_per_1k_output,
                    "category": m.category,
                    "healthy": healthy,
                }));
            }
        }
        models
    }

    /// Smart route: find the best provider/model for a given strategy
    pub async fn smart_route(&self, model_hint: &str, strategy: RoutingStrategy) -> Option<(String, String)> {
        let providers = self.providers.read().await;
        let health = self.health.read().await;
        let mut candidates: Vec<(String, String, f64)> = Vec::new();

        for (id, cfg) in providers.iter() {
            if !cfg.enabled { continue; }
            let h = health.get(id);
            if let Some(h) = h {
                if !h.healthy { continue; }
            }
            for m in &cfg.models {
                if !m.enabled { continue; }
                // Match by model hint (partial match)
                if !model_hint.is_empty() && !m.id.contains(model_hint) && !m.name.to_lowercase().contains(&model_hint.to_lowercase()) {
                    continue;
                }
                let score = match strategy {
                    RoutingStrategy::Cheapest => -(m.cost_per_1k_input + m.cost_per_1k_output),
                    RoutingStrategy::Fastest => -(h.map(|h| h.avg_latency_ms).unwrap_or(1000.0)),
                    RoutingStrategy::Best => m.context_window as f64 / 1000.0 - (m.cost_per_1k_input + m.cost_per_1k_output) * 100.0,
                    RoutingStrategy::Priority => -(cfg.priority as f64),
                };
                candidates.push((id.clone(), m.id.clone(), score));
            }
        }

        candidates.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        candidates.first().map(|(p, m, _)| (p.clone(), m.clone()))
    }

    /// Get fallback provider for a given provider
    pub async fn get_fallback(&self, provider_id: &str) -> Option<String> {
        self.providers.read().await.get(provider_id).and_then(|c| c.fallback_to.clone())
    }

    /// Check if a model string targets an external provider (e.g. "groq/llama-3.3-70b")
    pub fn parse_external_model(model: &str) -> Option<(String, String)> {
        if let Some(idx) = model.find('/') {
            let provider = model[..idx].to_lowercase();
            let model_name = model[idx + 1..].to_string();
            if !model_name.is_empty() {
                return Some((provider, model_name));
            }
        }
        None
    }

    /// Get provider config by name
    pub async fn get_provider(&self, name: &str) -> Option<ProviderConfig> {
        self.providers.read().await.get(name).cloned()
    }

    /// List all configured providers
    pub async fn list_providers(&self) -> Vec<ProviderInfo> {
        self.providers.read().await.iter().map(|(name, cfg)| {
            ProviderInfo {
                name: name.clone(),
                base_url: cfg.base_url.clone(),
                default_model: cfg.default_model.clone(),
                has_key: !cfg.api_key.is_empty(),
            }
        }).collect()
    }

    /// Add or update a provider and persist to disk
    pub async fn save_provider(&self, id: &str, config: ProviderConfig) -> Result<(), String> {
        self.providers.write().await.insert(id.to_string(), config);
        self.save_to_file().await
    }

    /// Delete a provider and persist to disk
    pub async fn delete_provider(&self, id: &str) -> Result<bool, String> {
        let removed = self.providers.write().await.remove(id).is_some();
        if removed {
            self.save_to_file().await?;
        }
        Ok(removed)
    }

    /// Persist current providers to config file
    async fn save_to_file(&self) -> Result<(), String> {
        let providers = self.providers.read().await;
        let json = serde_json::to_string_pretty(&*providers)
            .map_err(|e| format!("Failed to serialize: {}", e))?;
        std::fs::write(&self.config_path, &json)
            .map_err(|e| format!("Failed to write {}: {}", self.config_path, e))?;
        info!("[EXTERNAL] Saved {} providers to {}", providers.len(), self.config_path);
        Ok(())
    }

    /// List providers with full config (for admin UI, keys masked)
    pub async fn list_providers_full(&self) -> Vec<ProviderFullInfo> {
        self.providers.read().await.iter().map(|(id, cfg)| {
            let masked_key = if cfg.api_key.len() > 8 {
                format!("{}...{}", &cfg.api_key[..4], &cfg.api_key[cfg.api_key.len()-4..])
            } else if cfg.api_key.is_empty() {
                String::new()
            } else {
                "****".to_string()
            };
            ProviderFullInfo {
                id: id.clone(),
                name: cfg.name.clone(),
                base_url: cfg.base_url.clone(),
                default_model: cfg.default_model.clone(),
                has_key: !cfg.api_key.is_empty(),
                masked_key,
            }
        }).collect()
    }

    /// Proxy using a user-provided API key (BYOK)
    pub async fn proxy_chat_completion_with_key(
        &self,
        provider_name: &str,
        model: &str,
        messages: &[serde_json::Value],
        max_tokens: u32,
        temperature: Option<f32>,
        user_api_key: &str,
    ) -> Result<ExternalResponse, String> {
        let provider = self.get_provider(provider_name).await
            .ok_or_else(|| format!("Provider '{}' not configured", provider_name))?;
        let url = format!("{}/chat/completions", provider.base_url.trim_end_matches('/'));
        self.do_proxy_request(provider_name, &url, model, messages, max_tokens, temperature, user_api_key).await
    }

    /// Proxy a chat completion request using the configured (admin) API key
    pub async fn proxy_chat_completion(
        &self,
        provider_name: &str,
        model: &str,
        messages: &[serde_json::Value],
        max_tokens: u32,
        temperature: Option<f32>,
    ) -> Result<ExternalResponse, String> {
        let provider = self.get_provider(provider_name).await
            .ok_or_else(|| format!("Provider '{}' not configured. Add it to {}", provider_name, self.config_path))?;
        let url = format!("{}/chat/completions", provider.base_url.trim_end_matches('/'));
        self.do_proxy_request(provider_name, &url, model, messages, max_tokens, temperature, &provider.api_key).await
    }

    /// Shared proxy implementation with health tracking
    fn do_proxy_request<'a>(
        &'a self,
        provider_name: &'a str,
        url: &'a str,
        model: &'a str,
        messages: &'a [serde_json::Value],
        max_tokens: u32,
        temperature: Option<f32>,
        bearer_key: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ExternalResponse, String>> + Send + 'a>> {
        Box::pin(async move {
        let mut body = serde_json::json!({
            "model": model,
            "messages": messages,
            "max_tokens": max_tokens,
        });

        if let Some(temp) = temperature {
            body["temperature"] = serde_json::json!(temp);
        }

        info!("[EXTERNAL] {} -> {} model={} tokens={}", provider_name, url, model, max_tokens);

        let start = std::time::Instant::now();

        let resp = self.client
            .post(url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", bearer_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                format!("HTTP request failed: {}", e)
            })?;

        let status = resp.status();
        let resp_body = resp.text().await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        let duration_ms = start.elapsed().as_millis() as u64;

        if !status.is_success() {
            self.record_request(provider_name, duration_ms, false).await;
            warn!("[EXTERNAL] {} returned {}: {}", provider_name, status, &resp_body[..resp_body.len().min(200)]);

            // Try fallback if available
            if let Some(fallback_id) = self.get_fallback(provider_name).await {
                info!("[EXTERNAL] Trying fallback: {} -> {}", provider_name, fallback_id);
                if let Some(fb_provider) = self.get_provider(&fallback_id).await {
                    let fb_url = format!("{}/chat/completions", fb_provider.base_url.trim_end_matches('/'));
                    return self.do_proxy_request(&fallback_id, &fb_url, model, messages, max_tokens, temperature, &fb_provider.api_key).await;
                }
            }

            return Err(format!("Provider {} returned {}: {}", provider_name, status, &resp_body[..resp_body.len().min(200)]));
        }

        self.record_request(provider_name, duration_ms, true).await;

        let parsed: serde_json::Value = serde_json::from_str(&resp_body)
            .map_err(|e| format!("Failed to parse response JSON: {}", e))?;

        let content = parsed["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let prompt_tokens = parsed["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32;
        let completion_tokens = parsed["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32;
        let total_tokens = parsed["usage"]["total_tokens"].as_u64().unwrap_or(0) as u32;
        let response_model = parsed["model"].as_str().unwrap_or(model).to_string();

        info!("[EXTERNAL] {} completed in {}ms: {} tokens, model={}", provider_name, duration_ms, total_tokens, response_model);

        Ok(ExternalResponse {
            content,
            model: response_model,
            prompt_tokens,
            completion_tokens,
            total_tokens,
            duration_ms,
            provider: provider_name.to_string(),
        })
        }) // end Box::pin(async move)
    }

    /// Generate embedding vector for text via an external provider's /v1/embeddings endpoint.
    /// Tries openai first, then groq, then any provider with an embeddings endpoint.
    /// Returns None if no embedding provider is available (graceful degradation).
    pub async fn generate_embedding(&self, text: &str) -> Option<Vec<f32>> {
        let providers = self.providers.read().await;

        // 1) Try Google Gemini native embedding API (free!)
        if let Some(cfg) = providers.get("google") {
            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models/text-embedding-004:embedContent?key={}",
                cfg.api_key
            );
            let body = serde_json::json!({
                "content": { "parts": [{ "text": text }] }
            });

            match self.client.post(&url)
                .header("Content-Type", "application/json")
                .json(&body)
                .timeout(std::time::Duration::from_secs(10))
                .send().await
            {
                Ok(r) if r.status().is_success() => {
                    if let Ok(json) = r.json::<serde_json::Value>().await {
                        if let Some(values) = json["embedding"]["values"].as_array() {
                            let vec: Vec<f32> = values.iter()
                                .filter_map(|v| v.as_f64().map(|f| f as f32))
                                .collect();
                            if !vec.is_empty() {
                                info!("[EMBEDDING] Generated via google ({} dims)", vec.len());
                                return Some(vec);
                            }
                        }
                    }
                }
                Ok(r) => { info!("[EMBEDDING] google returned {}, trying next", r.status()); }
                Err(e) => { info!("[EMBEDDING] google failed: {}, trying next", e); }
            }
        }

        // 2) Try OpenAI embeddings (standard /v1/embeddings endpoint)
        if let Some(cfg) = providers.get("openai") {
            let url = format!("{}/embeddings", cfg.base_url.trim_end_matches('/'));
            let body = serde_json::json!({
                "model": "text-embedding-3-small",
                "input": text,
            });

            match self.client.post(&url)
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", cfg.api_key))
                .json(&body)
                .timeout(std::time::Duration::from_secs(10))
                .send().await
            {
                Ok(r) if r.status().is_success() => {
                    if let Ok(json) = r.json::<serde_json::Value>().await {
                        if let Some(embedding) = json["data"][0]["embedding"].as_array() {
                            let vec: Vec<f32> = embedding.iter()
                                .filter_map(|v| v.as_f64().map(|f| f as f32))
                                .collect();
                            if !vec.is_empty() {
                                info!("[EMBEDDING] Generated via openai ({} dims)", vec.len());
                                return Some(vec);
                            }
                        }
                    }
                }
                Ok(r) => { info!("[EMBEDDING] openai returned {}, trying next", r.status()); }
                Err(e) => { info!("[EMBEDDING] openai failed: {}, trying next", e); }
            }
        }

        info!("[EMBEDDING] No embedding provider available, falling back to keyword-only");
        None
    }
}

/// Cosine similarity between two vectors. Returns 0.0 if either is empty or zero-length.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() { return 0.0; }
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom < 1e-10 { 0.0 } else { dot / denom }
}

/// Hybrid retrieval: combines keyword score with vector similarity.
/// Returns top chunks sorted by combined score.
pub fn retrieve_hybrid(
    query: &str,
    query_embedding: Option<&[f32]>,
    chunks: &[(String, Option<Vec<f32>>)],
    max_chunks: usize,
) -> Vec<String> {
    if chunks.is_empty() { return vec![]; }

    let query_words: Vec<String> = query.to_lowercase()
        .split_whitespace()
        .filter(|w| w.len() > 2)
        .map(|w| w.to_string())
        .collect();

    let mut scored: Vec<(f64, &str)> = chunks.iter().map(|(text, embedding)| {
        let text_lower = text.to_lowercase();

        // Keyword score (0..N)
        let keyword_score: f64 = query_words.iter()
            .filter(|w| text_lower.contains(w.as_str()))
            .count() as f64;
        let phrase_bonus = if !query.is_empty() && text_lower.contains(&query.to_lowercase()) { 3.0 } else { 0.0 };

        // Vector score (0..1) scaled to match keyword range
        let vector_score = match (query_embedding, embedding.as_ref()) {
            (Some(qe), Some(ce)) => cosine_similarity(qe, ce) as f64 * 5.0, // Scale to ~0-5 range
            _ => 0.0,
        };

        let total = keyword_score + phrase_bonus + vector_score;
        (total, text.as_str())
    }).collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.iter()
        .filter(|(score, _)| *score > 0.0)
        .take(max_chunks)
        .map(|(_, text)| text.to_string())
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct ExternalResponse {
    pub content: String,
    pub model: String,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    pub duration_ms: u64,
    pub provider: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderInfo {
    pub name: String,
    pub base_url: String,
    pub default_model: Option<String>,
    pub has_key: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderFullInfo {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub default_model: Option<String>,
    pub has_key: bool,
    pub masked_key: String,
}
