// =============================================================================
// dashboard.rs — Thin re-export layer
// Original monolith preserved in dashboard_legacy.rs
// All logic now lives in handlers/ modules (PepAI-style)
// =============================================================================

// Pages
pub use crate::handlers::pages::{
    landing_page, demo_page, docs_page, status_page, login_page, dashboard_page,
    chat_page, tenants_page, debug_page, analytics_page, settings_page, usage_page, health_page,
};

// Auth
pub use crate::handlers::auth_api::{
    register_user, login_user, logout, list_users, delete_user,
};

// Chat persistence
pub use crate::handlers::chat_api::{
    save_chat, list_chats, get_chat, delete_chat,
    search_chats, pin_chat, tag_chat,
};

// Tenants
pub use crate::handlers::tenant_api::{
    create_tenant, list_templates, list_tenants, get_tenant,
    update_tenant, delete_tenant, list_available_tools,
};

// Widget
pub use crate::handlers::widget_api::{widget_chat, widget_js};

// Debug
pub use crate::handlers::debug_api::{debug_events, debug_counters};

// Usage
pub use crate::handlers::usage_api::{usage_stats, usage_by_tenant, usage_by_user};

// RAG / Documents
pub use crate::handlers::rag_api::{upload_document, list_documents, delete_document};

// Agent
pub use crate::handlers::agent_api::run_agent;

// Demo
pub use crate::handlers::demo_api::demo_chat;

// Memory
pub use crate::handlers::memory_api::{store_memory, recall_memories, list_memories, delete_memory};

// Analytics
pub use crate::handlers::analytics_api::get_analytics;

// Billing & Profile
pub use crate::handlers::billing_api::{
    get_profile, save_user_provider, delete_user_provider,
    get_credits, add_credits, get_pricing, set_pricing,
};

// Health
pub use crate::handlers::health::full_healthcheck;

// Admin
pub use crate::handlers::admin_api::{
    admin_status, update_features, provider_health,
    list_external_models, reload_providers, admin_users,
    user_activity, terms_page, submit_feedback, list_feedback,
    update_feedback, feedback_page,
};
