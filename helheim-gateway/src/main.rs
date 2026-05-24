use axum::{
    routing::{get, post},
    Router,
};
use tower_http::services::ServeDir;
use std::sync::Arc;
use helheim_taskqueue::TaskQueue;
use tracing::info;
use helheim_alchemie::network::DiscoveryService;
use helheim_alchemie::orchestra::Orchestrator;

pub mod auth;
mod dashboard;
pub mod eventlog;
pub mod external_api;
mod handlers;
mod openai;
mod ratelimit;
pub mod sessions;
mod stripe_pay;
pub mod tools;
pub mod pepai;

pub struct AppState {
    pub queue: TaskQueue,
    pub api_keys: auth::ApiKeyStore,
    pub cluster_secret: String,
    pub rate_limiter: ratelimit::RateLimiter,
    pub events: eventlog::EventLog,
    pub external: external_api::ExternalProviders,
    pub sessions: sessions::SessionTracker,
    pub orchestrator: Arc<Orchestrator>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();

    let port = std::env::args()
        .nth(1)
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8080);

    let cluster_secret = std::env::var("CLUSTER_SECRET").unwrap_or_else(|_| {
        let secret = format!("cls-{}", &blake3::hash(auth::generate_cluster_secret().as_bytes()).to_hex()[..32]);
        info!("[GATEWAY] Generated cluster secret: {} (set CLUSTER_SECRET env to persist)", secret);
        secret
    });
    info!("[GATEWAY] Cluster secret loaded (nodes must present this to register)");

    let rate_limiter = ratelimit::RateLimiter::new(60, 60);

    let external = external_api::ExternalProviders::new();

    let discovery = Arc::new(DiscoveryService::new());
    let orchestrator = Arc::new(Orchestrator::new(discovery.clone()));

    let state = Arc::new(AppState {
        queue: TaskQueue::new(),
        api_keys: auth::ApiKeyStore::new(),
        cluster_secret,
        rate_limiter,
        events: eventlog::EventLog::new(),
        external,
        sessions: sessions::SessionTracker::new(),
        orchestrator,
    });

    // Only generate admin key if none exists (keys are now persistent in SQLite)
    if !state.api_keys.has_admin_key().await {
        let admin_key = state.api_keys.create_key_with_role("admin", auth::KeyRole::Admin).await;
        info!("[GATEWAY] NEW Admin API key (KEEP SECRET): {}", admin_key);
    } else {
        let admin_key = state.api_keys.get_admin_key().await.unwrap_or_default();
        info!("[GATEWAY] Existing admin key loaded: {}", admin_key);
    }

    let app = Router::new()
        // Public
        .route("/", get(dashboard::landing_page))
        .route("/login", get(dashboard::login_page))
        .route("/dashboard", get(dashboard::dashboard_page))
        .route("/chat", get(dashboard::chat_page))
        .route("/tenants", get(dashboard::tenants_page))
        .route("/demo", get(dashboard::demo_page))
        .route("/docs", get(dashboard::docs_page))
        .route("/documentation", get(dashboard::docs_page))
        .route("/status", get(dashboard::status_page))
        .route("/debug", get(dashboard::debug_page))
        .route("/analytics", get(dashboard::analytics_page))
        .route("/logout", get(dashboard::logout))
        .route("/health", get(handlers::health::health))
        // Auth
        .route("/api/v1/register", post(dashboard::register_user))
        .route("/api/v1/login", post(dashboard::login_user))
        // Chat persistence
        .route("/api/v1/chats", post(dashboard::save_chat).get(dashboard::list_chats))
        .route("/api/v1/chats/search", get(dashboard::search_chats))
        .route("/api/v1/chats/:chat_id", get(dashboard::get_chat).delete(dashboard::delete_chat))
        .route("/api/v1/chats/:chat_id/pin", post(dashboard::pin_chat))
        .route("/api/v1/chats/:chat_id/tags", post(dashboard::tag_chat))
        // User management (admin)
        .route("/api/v1/users", get(dashboard::list_users))
        .route("/api/v1/users/:user_id", axum::routing::delete(dashboard::delete_user))
        // Tenant management
        .route("/api/v1/tenants", post(dashboard::create_tenant).get(dashboard::list_tenants))
        .route("/api/v1/tenants/:tenant_id", get(dashboard::get_tenant).put(dashboard::update_tenant).delete(dashboard::delete_tenant))
        // Demo (public)
        .route("/api/v1/demo/chat", post(dashboard::demo_chat))
        // Debug (admin)
        .route("/api/v1/debug/events", get(dashboard::debug_events))
        .route("/api/v1/debug/counters", get(dashboard::debug_counters))
        // Usage & Analytics
        .route("/api/v1/usage/stats", get(dashboard::usage_stats))
        .route("/api/v1/usage/tenants", get(dashboard::usage_by_tenant))
        .route("/api/v1/usage/users", get(dashboard::usage_by_user))
        .route("/api/v1/analytics", get(dashboard::get_analytics))
        // Profile & Billing
        .route("/api/v1/profile", get(dashboard::get_profile))
        .route("/api/v1/profile/providers", post(dashboard::save_user_provider))
        .route("/api/v1/profile/providers/:provider_id", axum::routing::delete(dashboard::delete_user_provider))
        .route("/api/v1/credits", get(dashboard::get_credits))
        .route("/api/v1/credits/add", post(dashboard::add_credits))
        .route("/api/v1/pricing", get(dashboard::get_pricing).post(dashboard::set_pricing))
        .route("/settings", get(dashboard::settings_page))
        .route("/usage", get(dashboard::usage_page))
        // Admin API
        .route("/api/v1/admin/status", get(dashboard::admin_status))
        .route("/api/v1/admin/features", post(dashboard::update_features))
        .route("/api/v1/admin/health", get(dashboard::provider_health))
        .route("/api/v1/admin/models", get(dashboard::list_external_models))
        .route("/api/v1/admin/reload", post(dashboard::reload_providers))
        .route("/api/v1/admin/users", get(dashboard::admin_users))
        .route("/api/v1/admin/users/:api_key/activity", get(dashboard::user_activity))
        .route("/api/v1/admin/healthcheck", get(dashboard::full_healthcheck))
        .route("/health-dashboard", get(dashboard::health_page))
        .route("/terms", get(dashboard::terms_page))
        .route("/privacy", get(dashboard::terms_page))
        .route("/feedback", get(dashboard::feedback_page))
        .route("/api/v1/feedback", post(dashboard::submit_feedback))
        .route("/api/v1/admin/feedback", get(dashboard::list_feedback))
        .route("/api/v1/admin/feedback/:feedback_id", post(dashboard::update_feedback))
        // Documents (RAG)
        .route("/api/v1/documents", post(dashboard::upload_document))
        .route("/api/v1/documents/:tenant_id", get(dashboard::list_documents))
        .route("/api/v1/documents/:tenant_id/:doc_id", axum::routing::delete(dashboard::delete_document))
        // Memory (long-term, PepAI-style)
        .route("/api/v1/memories", post(dashboard::store_memory).get(dashboard::list_memories))
        .route("/api/v1/memories/recall", post(dashboard::recall_memories))
        .route("/api/v1/memories/:memory_id", axum::routing::delete(dashboard::delete_memory))
        // Agent
        .route("/api/v1/agent", post(dashboard::run_agent))
        // Tools & templates
        .route("/api/v1/tools", get(dashboard::list_available_tools))
        .route("/api/v1/templates", get(dashboard::list_templates))
        // Widget (public)
        .route("/api/v1/widget/:tenant_id/chat", post(dashboard::widget_chat))
        .route("/widget/:tenant_id", get(dashboard::widget_js))
        // Task API
        .route("/api/v1/tasks", post(handlers::tasks::submit_task))
        .route("/api/v1/tasks/:task_id", get(handlers::tasks::get_task))
        .route("/api/v1/usage", get(handlers::usage::get_usage))
        // Cluster management
        .route("/api/v1/nodes", get(handlers::nodes::list_nodes))
        .route("/api/v1/nodes/register", post(handlers::nodes::register_node))
        .route("/api/v1/nodes/heartbeat", post(handlers::nodes::node_heartbeat))
        .route("/api/v1/node/:node_id/tasks", get(handlers::nodes::get_node_tasks))
        .route("/api/v1/task/:task_id/complete", post(handlers::tasks::complete_task))
        .route("/api/v1/stats", get(handlers::stats::get_stats))
        // Native Helheim AST Gateway
        .route("/api/execute", post(handlers::execute::execute_script))
        // OpenAI-compatible API
        .route("/v1/chat/completions", post(openai::chat_completions))
        .route("/v1/models", get(openai::list_models))
        .route("/api/v1/models", get(openai::model_registry))
        // External providers
        .route("/api/v1/providers", get(openai::list_providers).post(openai::save_provider))
        .route("/api/v1/providers/reload", post(openai::reload_providers))
        .route("/api/v1/providers/:provider_id", axum::routing::delete(openai::delete_provider))
        .route("/providers", get(openai::providers_page))
        // Stripe payments
        .route("/api/v1/checkout", post(stripe_pay::create_checkout))
        .route("/api/v1/checkout/success", get(stripe_pay::checkout_success))
        .route("/api/v1/stripe/webhook", post(stripe_pay::stripe_webhook))
        // CPU Compute
        .route("/api/v1/hash", post(handlers::compute::hash))
        .route("/api/v1/logs/analyze", post(handlers::compute::log_analysis))
        // Admin
        .route("/api/v1/keys", post(handlers::keys::create_api_key))
        // Static files (JS, CSS for modular chat UI)
        .nest_service("/static", ServeDir::new(static_dir()))
        .with_state(state);

    info!("[GATEWAY] Helheim API Gateway starting on port {}", port);
    info!("[GATEWAY] Static files: {}", static_dir());

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn static_dir() -> String {
    // 1. Dev: use CARGO_MANIFEST_DIR (set at compile time)
    let dev_path = concat!(env!("CARGO_MANIFEST_DIR"), "/static");
    if std::path::Path::new(dev_path).exists() {
        return dev_path.to_string();
    }
    // 2. Production: next to binary
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("static");
            if p.exists() { return p.to_string_lossy().to_string(); }
        }
    }
    // 3. Fallback: current dir
    "static".to_string()
}
