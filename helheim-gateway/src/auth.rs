use rusqlite::Connection;
use std::sync::Mutex;
use tracing::info;

/// Permission level for API keys
#[derive(Debug, Clone, PartialEq)]
pub enum KeyRole {
    /// Full access: all task types including Execute, Store, admin endpoints
    Admin,
    /// Safe tasks only: AiInference, Hash, MatMul, LogAnalysis, Retrieve, Custom
    Standard,
}

/// Persistent API key store backed by SQLite
pub struct ApiKeyStore {
    db: Mutex<Connection>,
}

#[derive(Debug, Clone)]
pub struct KeyInfo {
    pub name: String,
    pub created_at: u64,
    pub active: bool,
    pub role: KeyRole,
}

fn preferred_db_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("HELHEIM_KEYS_DB") {
        return std::path::PathBuf::from(p);
    }
    std::path::PathBuf::from("/var/lib/helheim/keys.db")
}

fn fallback_db_path() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.join("keys.db")))
        .filter(|p| p.parent().map_or(false, |d| d.exists()))
        .unwrap_or_else(|| std::path::PathBuf::from("keys.db"))
}

impl ApiKeyStore {
    pub fn new() -> Self {
        let preferred = preferred_db_path();
        if let Some(parent) = preferred.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let path = match Connection::open(&preferred) {
            Ok(_) => {
                info!("[AUTH] Using keys database: {}", preferred.display());
                preferred
            }
            Err(_) => {
                let fb = fallback_db_path();
                if let Some(parent) = fb.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                info!("[AUTH] Using keys database (fallback): {}", fb.display());
                fb
            }
        };
        Self::open(path.to_string_lossy().as_ref())
    }

    pub fn open(path: &str) -> Self {
        // Ensure parent directory exists
        if let Some(parent) = std::path::Path::new(path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path).expect("Failed to open keys database");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS api_keys (
                key TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                role TEXT NOT NULL DEFAULT 'standard',
                active INTEGER NOT NULL DEFAULT 1,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS users (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                email TEXT UNIQUE NOT NULL,
                password_hash TEXT NOT NULL,
                salt TEXT NOT NULL,
                api_key TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (api_key) REFERENCES api_keys(key)
            );
            CREATE TABLE IF NOT EXISTS chats (
                id TEXT PRIMARY KEY,
                api_key TEXT NOT NULL,
                title TEXT NOT NULL,
                messages TEXT NOT NULL,
                model TEXT,
                system_prompt TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                FOREIGN KEY (api_key) REFERENCES api_keys(key)
            );
            CREATE TABLE IF NOT EXISTS usage_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                api_key TEXT NOT NULL,
                tenant_id TEXT,
                source TEXT NOT NULL DEFAULT 'api',
                model TEXT,
                prompt_chars INTEGER NOT NULL DEFAULT 0,
                output_chars INTEGER NOT NULL DEFAULT 0,
                tokens_approx INTEGER NOT NULL DEFAULT 0,
                latency_ms INTEGER NOT NULL DEFAULT 0,
                tools_used TEXT,
                success INTEGER NOT NULL DEFAULT 1,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS documents (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                tenant_id TEXT NOT NULL,
                filename TEXT NOT NULL,
                content TEXT NOT NULL,
                chunks TEXT NOT NULL DEFAULT '[]',
                chunk_count INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (tenant_id) REFERENCES tenants(id)
            );
            CREATE TABLE IF NOT EXISTS memories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                api_key TEXT NOT NULL,
                tenant_id TEXT,
                name TEXT NOT NULL,
                content TEXT NOT NULL,
                memory_type TEXT NOT NULL DEFAULT 'FACT',
                keywords TEXT NOT NULL DEFAULT '',
                related_ids TEXT NOT NULL DEFAULT '[]',
                created_at INTEGER NOT NULL,
                FOREIGN KEY (api_key) REFERENCES api_keys(key)
            );
            CREATE INDEX IF NOT EXISTS idx_memories_api_key ON memories(api_key);
            CREATE INDEX IF NOT EXISTS idx_memories_tenant ON memories(tenant_id);
            CREATE TABLE IF NOT EXISTS tenants (
                id TEXT PRIMARY KEY,
                api_key TEXT NOT NULL,
                name TEXT NOT NULL,
                domain TEXT,
                faq TEXT NOT NULL DEFAULT '',
                system_prompt TEXT NOT NULL DEFAULT 'Je bent een behulpzame klantenservice medewerker. Beantwoord vragen op basis van de FAQ hieronder. Als je het antwoord niet weet, zeg dat eerlijk en verwijs de klant door naar de eigenaar.',
                welcome_message TEXT NOT NULL DEFAULT 'Hallo! Hoe kan ik u helpen?',
                model TEXT NOT NULL DEFAULT 'auto',
                color_primary TEXT NOT NULL DEFAULT '#6366f1',
                color_bg TEXT NOT NULL DEFAULT '#0a0a0f',
                color_text TEXT NOT NULL DEFAULT '#e2e8f0',
                bot_type TEXT NOT NULL DEFAULT 'custom',
                tools TEXT NOT NULL DEFAULT '[]',
                tool_config TEXT NOT NULL DEFAULT '{}',
                max_messages_per_hour INTEGER NOT NULL DEFAULT 60,
                active INTEGER NOT NULL DEFAULT 1,
                total_messages INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                FOREIGN KEY (api_key) REFERENCES api_keys(key)
            );"
        ).expect("Failed to create tables");
        // Migrate: chats columns
        for col in &[
            ("pinned", "INTEGER NOT NULL DEFAULT 0"),
            ("tags", "TEXT NOT NULL DEFAULT '[]'"),
            ("message_count", "INTEGER NOT NULL DEFAULT 0"),
            ("preview", "TEXT NOT NULL DEFAULT ''"),
        ] {
            let _ = conn.execute(&format!("ALTER TABLE chats ADD COLUMN {} {}", col.0, col.1), []);
        }
        let _ = conn.execute("CREATE INDEX IF NOT EXISTS idx_chats_api_key ON chats(api_key, updated_at DESC)", []);
        // FTS5 full-text search on chats
        let _ = conn.execute_batch(
            "CREATE VIRTUAL TABLE IF NOT EXISTS chats_fts USING fts5(chat_id, title, content, tokenize='unicode61');"
        );
        // Migrate: add new columns if they don't exist
        for col in &[
            ("bot_type", "TEXT NOT NULL DEFAULT 'custom'"),
            ("tools", "TEXT NOT NULL DEFAULT '[]'"),
            ("tool_config", "TEXT NOT NULL DEFAULT '{}'"),
        ] {
            let _ = conn.execute(&format!("ALTER TABLE tenants ADD COLUMN {} {}", col.0, col.1), []);
        }
        // Migrate: embedding columns for hybrid RAG
        for (table, col, coltype) in &[
            ("documents", "chunk_embeddings", "TEXT NOT NULL DEFAULT '[]'"),
            ("memories", "embedding", "BLOB"),
            ("memories", "classification", "TEXT NOT NULL DEFAULT 'GENERAL'"),
        ] {
            let _ = conn.execute(&format!("ALTER TABLE {} ADD COLUMN {} {}", table, col, coltype), []);
        }
        // Migrate: user credits, plan, and provider keys
        for col in &[
            ("credits", "INTEGER NOT NULL DEFAULT 0"),
            ("plan", "TEXT NOT NULL DEFAULT 'free'"),
            ("stripe_customer_id", "TEXT"),
        ] {
            let _ = conn.execute(&format!("ALTER TABLE users ADD COLUMN {} {}", col.0, col.1), []);
        }
        // Per-user provider keys (BYOK)
        let _ = conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS user_providers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                api_key TEXT NOT NULL,
                provider_id TEXT NOT NULL,
                provider_api_key TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (api_key) REFERENCES api_keys(key),
                UNIQUE(api_key, provider_id)
            );
            CREATE TABLE IF NOT EXISTS credit_transactions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                api_key TEXT NOT NULL,
                amount INTEGER NOT NULL,
                balance_after INTEGER NOT NULL,
                reason TEXT NOT NULL,
                model TEXT,
                stripe_payment_id TEXT,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (api_key) REFERENCES api_keys(key)
            );
            CREATE INDEX IF NOT EXISTS idx_credit_tx_key ON credit_transactions(api_key);
            CREATE TABLE IF NOT EXISTS feedback (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                api_key TEXT,
                email TEXT,
                category TEXT NOT NULL DEFAULT 'general',
                message TEXT NOT NULL,
                page TEXT,
                user_agent TEXT,
                status TEXT NOT NULL DEFAULT 'new',
                admin_note TEXT,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS pricing (
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                cost_per_1k_tokens REAL NOT NULL DEFAULT 0.001,
                price_per_1k_tokens REAL NOT NULL DEFAULT 0.003,
                active INTEGER NOT NULL DEFAULT 1,
                PRIMARY KEY (provider, model)
            );
            CREATE TABLE IF NOT EXISTS pepai_state (
                api_key TEXT PRIMARY KEY,
                state_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL,
                FOREIGN KEY (api_key) REFERENCES api_keys(key)
            );"
        );
        info!("[AUTH] Key store opened: {}", path);
        Self { db: Mutex::new(conn) }
    }

    /// Register a new user with email + password. Returns (api_key, error_msg)
    pub fn register_user(&self, email: &str, password: &str) -> Result<String, String> {
        let email = email.trim().to_lowercase();
        if email.is_empty() || !email.contains('@') {
            return Err("Invalid email address".to_string());
        }
        if password.len() < 6 {
            return Err("Password must be at least 6 characters".to_string());
        }

        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());

        // Check if email already exists
        let exists: bool = db.query_row(
            "SELECT COUNT(*) > 0 FROM users WHERE email = ?1",
            rusqlite::params![email],
            |row| row.get(0),
        ).unwrap_or(false);
        if exists {
            return Err("Email already registered".to_string());
        }

        // Hash password with salt
        let salt = hex::encode(rand_bytes());
        let password_hash = hash_password(password, &salt);

        // Create API key for this user
        let api_key = generate_api_key();
        let ts = now();

        db.execute(
            "INSERT INTO api_keys (key, name, role, active, created_at) VALUES (?1, ?2, 'standard', 1, ?3)",
            rusqlite::params![api_key, email, ts],
        ).map_err(|e| format!("Failed to create API key: {}", e))?;

        db.execute(
            "INSERT INTO users (email, password_hash, salt, api_key, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![email, password_hash, salt, api_key, ts],
        ).map_err(|e| format!("Failed to create user: {}", e))?;

        // Grant trial credits
        db.execute(
            "UPDATE users SET credits = 50 WHERE api_key = ?1",
            rusqlite::params![api_key],
        ).ok();
        db.execute(
            "INSERT INTO credit_transactions (api_key, amount, balance_after, reason, model, stripe_payment_id, created_at) VALUES (?1, 50, 50, 'signup_trial', NULL, NULL, ?2)",
            rusqlite::params![api_key, ts],
        ).ok();

        info!("[AUTH] User registered: {} -> {} (+50 trial credits)", email, api_key);
        Ok(api_key)
    }

    /// Login with email + password. Returns api_key on success.
    pub fn login_user(&self, email: &str, password: &str) -> Result<String, String> {
        let email = email.trim().to_lowercase();
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());

        let result: Option<(String, String, String)> = db.query_row(
            "SELECT password_hash, salt, api_key FROM users WHERE email = ?1",
            rusqlite::params![email],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).ok();

        match result {
            Some((stored_hash, salt, api_key)) => {
                let attempt = hash_password(password, &salt);
                if attempt == stored_hash {
                    info!("[AUTH] User login: {}", email);
                    Ok(api_key)
                } else {
                    Err("Invalid password".to_string())
                }
            }
            None => Err("Email not found".to_string()),
        }
    }

    // === Chat persistence ===

    pub fn save_chat(&self, api_key: &str, chat_id: &str, title: &str, messages: &str, model: Option<&str>, system_prompt: Option<&str>) -> Result<(), String> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let ts = now();
        // Extract preview and message count from messages JSON
        let (preview, message_count) = Self::extract_chat_meta(messages);
        db.execute(
            "INSERT INTO chats (id, api_key, title, messages, model, system_prompt, preview, message_count, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
             ON CONFLICT(id) DO UPDATE SET title=?3, messages=?4, model=?5, system_prompt=?6, preview=?7, message_count=?8, updated_at=?9",
            rusqlite::params![chat_id, api_key, title, messages, model, system_prompt, preview, message_count, ts],
        ).map_err(|e| format!("Failed to save chat: {}", e))?;
        // Update FTS index
        let fts_content = Self::extract_fts_content(messages);
        let _ = db.execute("DELETE FROM chats_fts WHERE chat_id = ?1", rusqlite::params![chat_id]);
        let _ = db.execute(
            "INSERT INTO chats_fts (chat_id, title, content) VALUES (?1, ?2, ?3)",
            rusqlite::params![chat_id, title, fts_content],
        );
        Ok(())
    }

    pub fn list_chats(&self, api_key: &str) -> Vec<serde_json::Value> {
        self.list_chats_paginated(api_key, 0, 100)
    }

    pub fn list_chats_paginated(&self, api_key: &str, offset: u64, limit: u64) -> Vec<serde_json::Value> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = db.prepare(
            "SELECT id, title, model, created_at, updated_at, pinned, tags, message_count, preview \
             FROM chats WHERE api_key = ?1 \
             ORDER BY pinned DESC, updated_at DESC \
             LIMIT ?2 OFFSET ?3"
        ).unwrap();
        stmt.query_map(rusqlite::params![api_key, limit, offset], |row| {
            let tags_str: String = row.get::<_, String>(6).unwrap_or_else(|_| "[]".to_string());
            let tags: serde_json::Value = serde_json::from_str(&tags_str).unwrap_or(serde_json::json!([]));
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "title": row.get::<_, String>(1)?,
                "model": row.get::<_, Option<String>>(2)?,
                "created_at": row.get::<_, u64>(3)?,
                "updated_at": row.get::<_, u64>(4)?,
                "pinned": row.get::<_, bool>(5).unwrap_or(false),
                "tags": tags,
                "message_count": row.get::<_, i64>(7).unwrap_or(0),
                "preview": row.get::<_, String>(8).unwrap_or_default(),
            }))
        }).unwrap().filter_map(|r| r.ok()).collect()
    }

    pub fn search_chats(&self, api_key: &str, query: &str) -> Vec<serde_json::Value> {
        if query.trim().is_empty() { return self.list_chats(api_key); }
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        // FTS5 search with ranking
        let fts_query = query.split_whitespace()
            .map(|w| format!("\"{}\"*", w.replace('"', "")))
            .collect::<Vec<_>>()
            .join(" ");
        let mut stmt = match db.prepare(
            "SELECT c.id, c.title, c.model, c.created_at, c.updated_at, c.pinned, c.tags, c.message_count, c.preview, \
             snippet(chats_fts, 2, '<mark>', '</mark>', '...', 40) as snippet \
             FROM chats_fts f \
             JOIN chats c ON c.id = f.chat_id \
             WHERE chats_fts MATCH ?1 AND c.api_key = ?2 \
             ORDER BY rank \
             LIMIT 50"
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let rows = match stmt.query_map(rusqlite::params![fts_query, api_key], |row| {
            let tags_str: String = row.get::<_, String>(6).unwrap_or_else(|_| "[]".to_string());
            let tags: serde_json::Value = serde_json::from_str(&tags_str).unwrap_or(serde_json::json!([]));
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "title": row.get::<_, String>(1)?,
                "model": row.get::<_, Option<String>>(2)?,
                "created_at": row.get::<_, u64>(3)?,
                "updated_at": row.get::<_, u64>(4)?,
                "pinned": row.get::<_, bool>(5).unwrap_or(false),
                "tags": tags,
                "message_count": row.get::<_, i64>(7).unwrap_or(0),
                "preview": row.get::<_, String>(8).unwrap_or_default(),
                "snippet": row.get::<_, String>(9).unwrap_or_default(),
            }))
        }) {
            Ok(r) => r,
            Err(_) => return vec![],
        };
        rows.filter_map(|r| r.ok()).collect()
    }

    pub fn get_chat(&self, api_key: &str, chat_id: &str) -> Option<serde_json::Value> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.query_row(
            "SELECT id, title, messages, model, system_prompt, created_at, updated_at, pinned, tags FROM chats WHERE id = ?1 AND api_key = ?2",
            rusqlite::params![chat_id, api_key],
            |row| {
                let messages_str: String = row.get(2)?;
                let messages: serde_json::Value = serde_json::from_str(&messages_str).unwrap_or_default();
                let tags_str: String = row.get::<_, String>(8).unwrap_or_else(|_| "[]".to_string());
                let tags: serde_json::Value = serde_json::from_str(&tags_str).unwrap_or(serde_json::json!([]));
                Ok(serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "title": row.get::<_, String>(1)?,
                    "messages": messages,
                    "model": row.get::<_, Option<String>>(3)?,
                    "system_prompt": row.get::<_, Option<String>>(4)?,
                    "created_at": row.get::<_, u64>(5)?,
                    "updated_at": row.get::<_, u64>(6)?,
                    "pinned": row.get::<_, bool>(7).unwrap_or(false),
                    "tags": tags,
                }))
            }
        ).ok()
    }

    pub fn delete_chat(&self, api_key: &str, chat_id: &str) -> bool {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let rows = db.execute(
            "DELETE FROM chats WHERE id = ?1 AND api_key = ?2",
            rusqlite::params![chat_id, api_key],
        ).unwrap_or(0);
        if rows > 0 {
            let _ = db.execute("DELETE FROM chats_fts WHERE chat_id = ?1", rusqlite::params![chat_id]);
        }
        rows > 0
    }

    pub fn pin_chat(&self, api_key: &str, chat_id: &str, pinned: bool) -> bool {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.execute(
            "UPDATE chats SET pinned = ?1 WHERE id = ?2 AND api_key = ?3",
            rusqlite::params![pinned as i32, chat_id, api_key],
        ).unwrap_or(0) > 0
    }

    pub fn tag_chat(&self, api_key: &str, chat_id: &str, tags: &[String]) -> bool {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let tags_json = serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string());
        db.execute(
            "UPDATE chats SET tags = ?1 WHERE id = ?2 AND api_key = ?3",
            rusqlite::params![tags_json, chat_id, api_key],
        ).unwrap_or(0) > 0
    }

    pub fn count_chats(&self, api_key: &str) -> i64 {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.query_row(
            "SELECT COUNT(*) FROM chats WHERE api_key = ?1",
            rusqlite::params![api_key],
            |row| row.get(0),
        ).unwrap_or(0)
    }

    // Extract preview text and message count from messages JSON
    fn extract_chat_meta(messages_json: &str) -> (String, i64) {
        let msgs: Vec<serde_json::Value> = serde_json::from_str(messages_json).unwrap_or_default();
        let count = msgs.len() as i64;
        // Get last assistant message as preview
        let preview = msgs.iter().rev()
            .find(|m| m["role"].as_str() == Some("assistant"))
            .and_then(|m| m["content"].as_str())
            .unwrap_or("")
            .chars().take(120).collect::<String>();
        (preview, count)
    }

    // Extract all text content for FTS indexing
    fn extract_fts_content(messages_json: &str) -> String {
        let msgs: Vec<serde_json::Value> = serde_json::from_str(messages_json).unwrap_or_default();
        msgs.iter()
            .filter_map(|m| m["content"].as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    // === User management (admin) ===

    pub fn list_users(&self) -> Vec<serde_json::Value> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = db.prepare(
            "SELECT u.id, u.email, u.api_key, u.created_at, k.active, k.role FROM users u LEFT JOIN api_keys k ON u.api_key = k.key ORDER BY u.created_at DESC"
        ).unwrap();
        stmt.query_map([], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, i64>(0)?,
                "email": row.get::<_, String>(1)?,
                "api_key": row.get::<_, String>(2)?,
                "created_at": row.get::<_, u64>(3)?,
                "active": row.get::<_, bool>(4).unwrap_or(true),
                "role": row.get::<_, String>(5).unwrap_or_else(|_| "standard".to_string()),
            }))
        }).unwrap().filter_map(|r| r.ok()).collect()
    }

    pub fn delete_user(&self, user_id: i64) -> bool {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        // Get api_key first to also deactivate it
        let api_key: Option<String> = db.query_row(
            "SELECT api_key FROM users WHERE id = ?1", rusqlite::params![user_id], |row| row.get(0)
        ).ok();
        if let Some(key) = api_key {
            db.execute("UPDATE api_keys SET active = 0 WHERE key = ?1", rusqlite::params![key]).ok();
        }
        let rows = db.execute("DELETE FROM users WHERE id = ?1", rusqlite::params![user_id]).unwrap_or(0);
        rows > 0
    }

    // === Tenant management ===

    pub fn create_tenant(&self, api_key: &str, name: &str) -> Result<String, String> {
        let name = name.trim();
        if name.is_empty() { return Err("Tenant name is required".to_string()); }
        let id = generate_tenant_id();
        let ts = now();
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.execute(
            "INSERT INTO tenants (id, api_key, name, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
            rusqlite::params![id, api_key, name, ts],
        ).map_err(|e| format!("Failed to create tenant: {}", e))?;
        Ok(id)
    }

    pub fn update_tenant(&self, api_key: &str, tenant_id: &str, updates: &TenantUpdate) -> Result<(), String> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        // Verify ownership
        let owner: Option<String> = db.query_row(
            "SELECT api_key FROM tenants WHERE id = ?1", rusqlite::params![tenant_id], |row| row.get(0)
        ).ok();
        if owner.as_deref() != Some(api_key) { return Err("Tenant not found or not owned by you".to_string()); }
        let ts = now();
        if let Some(ref v) = updates.name { db.execute("UPDATE tenants SET name=?1, updated_at=?2 WHERE id=?3", rusqlite::params![v, ts, tenant_id]).ok(); }
        if let Some(ref v) = updates.domain { db.execute("UPDATE tenants SET domain=?1, updated_at=?2 WHERE id=?3", rusqlite::params![v, ts, tenant_id]).ok(); }
        if let Some(ref v) = updates.faq { db.execute("UPDATE tenants SET faq=?1, updated_at=?2 WHERE id=?3", rusqlite::params![v, ts, tenant_id]).ok(); }
        if let Some(ref v) = updates.system_prompt { db.execute("UPDATE tenants SET system_prompt=?1, updated_at=?2 WHERE id=?3", rusqlite::params![v, ts, tenant_id]).ok(); }
        if let Some(ref v) = updates.welcome_message { db.execute("UPDATE tenants SET welcome_message=?1, updated_at=?2 WHERE id=?3", rusqlite::params![v, ts, tenant_id]).ok(); }
        if let Some(ref v) = updates.model { db.execute("UPDATE tenants SET model=?1, updated_at=?2 WHERE id=?3", rusqlite::params![v, ts, tenant_id]).ok(); }
        if let Some(ref v) = updates.color_primary { db.execute("UPDATE tenants SET color_primary=?1, updated_at=?2 WHERE id=?3", rusqlite::params![v, ts, tenant_id]).ok(); }
        if let Some(ref v) = updates.color_bg { db.execute("UPDATE tenants SET color_bg=?1, updated_at=?2 WHERE id=?3", rusqlite::params![v, ts, tenant_id]).ok(); }
        if let Some(ref v) = updates.color_text { db.execute("UPDATE tenants SET color_text=?1, updated_at=?2 WHERE id=?3", rusqlite::params![v, ts, tenant_id]).ok(); }
        if let Some(ref v) = updates.bot_type { db.execute("UPDATE tenants SET bot_type=?1, updated_at=?2 WHERE id=?3", rusqlite::params![v, ts, tenant_id]).ok(); }
        if let Some(ref v) = updates.tools { db.execute("UPDATE tenants SET tools=?1, updated_at=?2 WHERE id=?3", rusqlite::params![v, ts, tenant_id]).ok(); }
        if let Some(ref v) = updates.tool_config { db.execute("UPDATE tenants SET tool_config=?1, updated_at=?2 WHERE id=?3", rusqlite::params![v, ts, tenant_id]).ok(); }
        if let Some(v) = updates.active { db.execute("UPDATE tenants SET active=?1, updated_at=?2 WHERE id=?3", rusqlite::params![v as i32, ts, tenant_id]).ok(); }
        Ok(())
    }

    pub fn list_tenants(&self, api_key: &str) -> Vec<serde_json::Value> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = db.prepare(
            "SELECT id, name, domain, active, total_messages, created_at, updated_at FROM tenants WHERE api_key = ?1 ORDER BY created_at DESC"
        ).unwrap();
        stmt.query_map(rusqlite::params![api_key], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "name": row.get::<_, String>(1)?,
                "domain": row.get::<_, Option<String>>(2)?,
                "active": row.get::<_, bool>(3)?,
                "total_messages": row.get::<_, i64>(4)?,
                "created_at": row.get::<_, u64>(5)?,
                "updated_at": row.get::<_, u64>(6)?,
            }))
        }).unwrap().filter_map(|r| r.ok()).collect()
    }

    pub fn get_tenant(&self, api_key: &str, tenant_id: &str) -> Option<serde_json::Value> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.query_row(
            "SELECT id, name, domain, faq, system_prompt, welcome_message, model, color_primary, color_bg, color_text, bot_type, tools, tool_config, max_messages_per_hour, active, total_messages, created_at, updated_at FROM tenants WHERE id = ?1 AND api_key = ?2",
            rusqlite::params![tenant_id, api_key],
            |row| Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "name": row.get::<_, String>(1)?,
                "domain": row.get::<_, Option<String>>(2)?,
                "faq": row.get::<_, String>(3)?,
                "system_prompt": row.get::<_, String>(4)?,
                "welcome_message": row.get::<_, String>(5)?,
                "model": row.get::<_, String>(6)?,
                "color_primary": row.get::<_, String>(7)?,
                "color_bg": row.get::<_, String>(8)?,
                "color_text": row.get::<_, String>(9)?,
                "bot_type": row.get::<_, String>(10)?,
                "tools": row.get::<_, String>(11)?,
                "tool_config": row.get::<_, String>(12)?,
                "max_messages_per_hour": row.get::<_, i64>(13)?,
                "active": row.get::<_, bool>(14)?,
                "total_messages": row.get::<_, i64>(15)?,
                "created_at": row.get::<_, u64>(16)?,
                "updated_at": row.get::<_, u64>(17)?,
            }))
        ).ok()
    }

    pub fn get_tenant_public(&self, tenant_id: &str) -> Option<TenantPublic> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.query_row(
            "SELECT id, api_key, name, faq, system_prompt, welcome_message, model, color_primary, color_bg, color_text, bot_type, tools, tool_config, max_messages_per_hour, active FROM tenants WHERE id = ?1",
            rusqlite::params![tenant_id],
            |row| Ok(TenantPublic {
                id: row.get(0)?,
                api_key: row.get(1)?,
                name: row.get(2)?,
                faq: row.get(3)?,
                system_prompt: row.get(4)?,
                welcome_message: row.get(5)?,
                model: row.get(6)?,
                color_primary: row.get(7)?,
                color_bg: row.get(8)?,
                color_text: row.get(9)?,
                bot_type: row.get(10)?,
                tools: row.get(11)?,
                tool_config: row.get(12)?,
                max_messages_per_hour: row.get(13)?,
                active: row.get(14)?,
            })
        ).ok()
    }

    pub fn delete_tenant(&self, api_key: &str, tenant_id: &str) -> bool {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let rows = db.execute(
            "DELETE FROM tenants WHERE id = ?1 AND api_key = ?2",
            rusqlite::params![tenant_id, api_key],
        ).unwrap_or(0);
        rows > 0
    }

    pub fn increment_tenant_messages(&self, tenant_id: &str) {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.execute(
            "UPDATE tenants SET total_messages = total_messages + 1 WHERE id = ?1",
            rusqlite::params![tenant_id],
        ).ok();
    }

    // === Usage logging (persistent) ===

    pub fn log_usage(&self, api_key: &str, tenant_id: Option<&str>, source: &str, model: Option<&str>,
                     prompt_chars: usize, output_chars: usize, latency_ms: u64, tools_used: Option<&str>, success: bool) {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let tokens_approx = (prompt_chars + output_chars) / 4;
        db.execute(
            "INSERT INTO usage_log (api_key, tenant_id, source, model, prompt_chars, output_chars, tokens_approx, latency_ms, tools_used, success, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![api_key, tenant_id, source, model, prompt_chars as i64, output_chars as i64,
                tokens_approx as i64, latency_ms as i64, tools_used, success as i32, now()],
        ).ok();
    }

    /// Get usage stats for a specific API key (or all if admin)
    pub fn get_usage_stats(&self, api_key: Option<&str>) -> serde_json::Value {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let (where_clause, params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = match api_key {
            Some(k) => ("WHERE api_key = ?1", vec![Box::new(k.to_string())]),
            None => ("", vec![]),
        };

        // Total stats
        let total: (i64, i64, i64, i64) = db.query_row(
            &format!("SELECT COALESCE(SUM(tokens_approx),0), COALESCE(SUM(prompt_chars),0), COALESCE(SUM(output_chars),0), COUNT(*) FROM usage_log {}", where_clause),
            rusqlite::params_from_iter(&params),
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        ).unwrap_or((0, 0, 0, 0));

        // Average latency
        let avg_latency: f64 = db.query_row(
            &format!("SELECT COALESCE(AVG(latency_ms),0) FROM usage_log {} AND success = 1", if where_clause.is_empty() { "WHERE 1=1" } else { where_clause }),
            rusqlite::params_from_iter(&params),
            |row| row.get(0),
        ).unwrap_or(0.0);

        // Error count
        let errors: i64 = db.query_row(
            &format!("SELECT COUNT(*) FROM usage_log {} {} success = 0",
                if where_clause.is_empty() { "WHERE" } else { where_clause }, if where_clause.is_empty() { "" } else { "AND" }),
            rusqlite::params_from_iter(&params),
            |row| row.get(0),
        ).unwrap_or(0);

        // Per-model breakdown
        let mut stmt = db.prepare(
            &format!("SELECT model, COUNT(*), COALESCE(SUM(tokens_approx),0), COALESCE(AVG(latency_ms),0) FROM usage_log {} GROUP BY model ORDER BY COUNT(*) DESC LIMIT 20", where_clause)
        ).unwrap();
        let models: Vec<serde_json::Value> = stmt.query_map(
            rusqlite::params_from_iter(&params),
            |row| Ok(serde_json::json!({
                "model": row.get::<_, Option<String>>(0)?,
                "count": row.get::<_, i64>(1)?,
                "tokens": row.get::<_, i64>(2)?,
                "avg_latency_ms": row.get::<_, f64>(3)?,
            }))
        ).unwrap().filter_map(|r| r.ok()).collect();

        // Per-source breakdown
        let mut stmt2 = db.prepare(
            &format!("SELECT source, COUNT(*), COALESCE(SUM(tokens_approx),0) FROM usage_log {} GROUP BY source", where_clause)
        ).unwrap();
        let sources: Vec<serde_json::Value> = stmt2.query_map(
            rusqlite::params_from_iter(&params),
            |row| Ok(serde_json::json!({
                "source": row.get::<_, String>(0)?,
                "count": row.get::<_, i64>(1)?,
                "tokens": row.get::<_, i64>(2)?,
            }))
        ).unwrap().filter_map(|r| r.ok()).collect();

        // Timeline: last 24h in hourly buckets
        let cutoff = now().saturating_sub(86400);
        let _stmt3 = db.prepare(
            &format!("SELECT (created_at / 3600) * 3600 as hour, COUNT(*), COALESCE(SUM(tokens_approx),0) FROM usage_log {} {} created_at >= ?{} GROUP BY hour ORDER BY hour",
                if where_clause.is_empty() { "WHERE" } else { where_clause },
                if where_clause.is_empty() { "" } else { "AND" },
                if api_key.is_some() { "2" } else { "1" })
        ).unwrap();
        let _timeline_params: Vec<Box<dyn rusqlite::types::ToSql>> = params.iter().map(|p| {
            // Re-box the params
            Box::new(p.to_sql().unwrap()) as Box<dyn rusqlite::types::ToSql>
        }).collect();
        // This is getting complex with dynamic params, let's simplify
        drop(_stmt3);

        let timeline: Vec<serde_json::Value> = if let Some(k) = api_key {
            let mut s = db.prepare(
                "SELECT (created_at / 3600) * 3600 as hour, COUNT(*), COALESCE(SUM(tokens_approx),0) FROM usage_log WHERE api_key = ?1 AND created_at >= ?2 GROUP BY hour ORDER BY hour"
            ).unwrap();
            s.query_map(rusqlite::params![k, cutoff], |row| Ok(serde_json::json!({
                "hour": row.get::<_, i64>(0)?,
                "count": row.get::<_, i64>(1)?,
                "tokens": row.get::<_, i64>(2)?,
            }))).unwrap().filter_map(|r| r.ok()).collect()
        } else {
            let mut s = db.prepare(
                "SELECT (created_at / 3600) * 3600 as hour, COUNT(*), COALESCE(SUM(tokens_approx),0) FROM usage_log WHERE created_at >= ?1 GROUP BY hour ORDER BY hour"
            ).unwrap();
            s.query_map(rusqlite::params![cutoff], |row| Ok(serde_json::json!({
                "hour": row.get::<_, i64>(0)?,
                "count": row.get::<_, i64>(1)?,
                "tokens": row.get::<_, i64>(2)?,
            }))).unwrap().filter_map(|r| r.ok()).collect()
        };

        serde_json::json!({
            "total_tokens": total.0,
            "total_prompt_chars": total.1,
            "total_output_chars": total.2,
            "total_requests": total.3,
            "avg_latency_ms": avg_latency,
            "total_errors": errors,
            "by_model": models,
            "by_source": sources,
            "timeline_24h": timeline,
        })
    }

    /// Get per-tenant usage stats
    pub fn get_tenant_usage(&self, api_key: &str) -> Vec<serde_json::Value> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = db.prepare(
            "SELECT u.tenant_id, t.name, COUNT(*), COALESCE(SUM(u.tokens_approx),0), COALESCE(AVG(u.latency_ms),0)
             FROM usage_log u LEFT JOIN tenants t ON u.tenant_id = t.id
             WHERE u.api_key = ?1 AND u.tenant_id IS NOT NULL
             GROUP BY u.tenant_id ORDER BY COUNT(*) DESC"
        ).unwrap();
        stmt.query_map(rusqlite::params![api_key], |row| Ok(serde_json::json!({
            "tenant_id": row.get::<_, String>(0)?,
            "name": row.get::<_, Option<String>>(1)?,
            "requests": row.get::<_, i64>(2)?,
            "tokens": row.get::<_, i64>(3)?,
            "avg_latency_ms": row.get::<_, f64>(4)?,
        }))).unwrap().filter_map(|r| r.ok()).collect()
    }

    /// Per-user activity stats (last 24h) for analytics dashboard
    pub fn get_user_activity_stats(&self) -> Vec<serde_json::Value> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let cutoff = now().saturating_sub(86400);
        let mut stmt = db.prepare(
            "SELECT u.api_key, COALESCE(usr.email, u.api_key) as email,
                    COUNT(*) as requests,
                    COALESCE(SUM(u.tokens_approx), 0) as tokens,
                    COALESCE(AVG(u.latency_ms), 0) as avg_latency,
                    MIN(u.created_at) as first_request,
                    MAX(u.created_at) as last_request,
                    SUM(CASE WHEN u.success = 0 THEN 1 ELSE 0 END) as errors
             FROM usage_log u
             LEFT JOIN users usr ON u.api_key = usr.api_key
             WHERE u.created_at >= ?1
             GROUP BY u.api_key
             ORDER BY last_request DESC"
        ).unwrap();
        stmt.query_map(rusqlite::params![cutoff], |row| Ok(serde_json::json!({
            "api_key": row.get::<_, String>(0)?,
            "email": row.get::<_, String>(1)?,
            "requests": row.get::<_, i64>(2)?,
            "tokens": row.get::<_, i64>(3)?,
            "avg_latency_ms": row.get::<_, f64>(4)?,
            "first_request": row.get::<_, u64>(5)?,
            "last_request": row.get::<_, u64>(6)?,
            "errors": row.get::<_, i64>(7)?,
        }))).unwrap().filter_map(|r| r.ok()).collect()
    }

    /// Get email for an API key
    pub fn get_email_for_key(&self, api_key: &str) -> Option<String> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.query_row(
            "SELECT email FROM users WHERE api_key = ?1",
            rusqlite::params![api_key],
            |row| row.get(0),
        ).ok()
    }

    /// Find a user's API key by their email address
    pub fn get_api_key_for_email(&self, email: &str) -> Option<String> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.query_row(
            "SELECT api_key FROM users WHERE email = ?1",
            rusqlite::params![email.trim().to_lowercase()],
            |row| row.get(0),
        ).ok()
    }

    /// Set user's plan (free, developer, business)
    pub fn set_plan(&self, api_key: &str, plan: &str) {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.execute(
            "UPDATE users SET plan = ?1 WHERE api_key = ?2",
            rusqlite::params![plan, api_key],
        ).ok();
    }

    /// Set user's stripe_customer_id
    pub fn set_stripe_customer_id(&self, api_key: &str, customer_id: &str) {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.execute(
            "UPDATE users SET stripe_customer_id = ?1 WHERE api_key = ?2",
            rusqlite::params![customer_id, api_key],
        ).ok();
    }

    // === Credits & Billing ===

    /// Get user's current credits balance
    pub fn get_credits(&self, api_key: &str) -> i64 {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.query_row(
            "SELECT COALESCE(u.credits, 0) FROM users u WHERE u.api_key = ?1",
            rusqlite::params![api_key],
            |row| row.get(0),
        ).unwrap_or(0)
    }

    /// Get user's plan (free, pro, etc.)
    pub fn get_plan(&self, api_key: &str) -> String {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.query_row(
            "SELECT COALESCE(u.plan, 'free') FROM users u WHERE u.api_key = ?1",
            rusqlite::params![api_key],
            |row| row.get(0),
        ).unwrap_or_else(|_| "free".to_string())
    }

    /// Add credits (positive) or deduct credits (negative). Returns new balance.
    pub fn adjust_credits(&self, api_key: &str, amount: i64, reason: &str, model: Option<&str>, stripe_payment_id: Option<&str>) -> Result<i64, String> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        // Get current balance
        let current: i64 = db.query_row(
            "SELECT COALESCE(credits, 0) FROM users WHERE api_key = ?1",
            rusqlite::params![api_key],
            |row| row.get(0),
        ).map_err(|_| "User not found".to_string())?;

        let new_balance = current + amount;
        if new_balance < 0 {
            return Err(format!("Insufficient credits: have {}, need {}", current, -amount));
        }

        db.execute(
            "UPDATE users SET credits = ?1 WHERE api_key = ?2",
            rusqlite::params![new_balance, api_key],
        ).map_err(|e| format!("Failed to update credits: {}", e))?;

        // Log transaction
        db.execute(
            "INSERT INTO credit_transactions (api_key, amount, balance_after, reason, model, stripe_payment_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![api_key, amount, new_balance, reason, model, stripe_payment_id, now()],
        ).ok();

        Ok(new_balance)
    }

    /// Get credit transaction history
    pub fn get_credit_history(&self, api_key: &str, limit: usize) -> Vec<serde_json::Value> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = db.prepare(
            "SELECT amount, balance_after, reason, model, stripe_payment_id, created_at FROM credit_transactions WHERE api_key = ?1 ORDER BY created_at DESC LIMIT ?2"
        ).unwrap();
        stmt.query_map(rusqlite::params![api_key, limit as i64], |row| Ok(serde_json::json!({
            "amount": row.get::<_, i64>(0)?,
            "balance_after": row.get::<_, i64>(1)?,
            "reason": row.get::<_, String>(2)?,
            "model": row.get::<_, Option<String>>(3)?,
            "stripe_payment_id": row.get::<_, Option<String>>(4)?,
            "created_at": row.get::<_, u64>(5)?,
        }))).unwrap().filter_map(|r| r.ok()).collect()
    }

    /// Get full user profile (email, plan, credits, created_at)
    pub fn get_user_profile(&self, api_key: &str) -> Option<serde_json::Value> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.query_row(
            "SELECT u.email, COALESCE(u.credits, 0), COALESCE(u.plan, 'free'), u.created_at, k.role FROM users u JOIN api_keys k ON u.api_key = k.key WHERE u.api_key = ?1",
            rusqlite::params![api_key],
            |row| Ok(serde_json::json!({
                "email": row.get::<_, String>(0)?,
                "credits": row.get::<_, i64>(1)?,
                "plan": row.get::<_, String>(2)?,
                "created_at": row.get::<_, u64>(3)?,
                "role": row.get::<_, String>(4)?,
            }))
        ).ok()
    }

    // === BYOK (Bring Your Own Key) ===

    /// Save a user's own provider API key
    pub fn save_user_provider(&self, api_key: &str, provider_id: &str, provider_api_key: &str) -> Result<(), String> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.execute(
            "INSERT INTO user_providers (api_key, provider_id, provider_api_key, created_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(api_key, provider_id) DO UPDATE SET provider_api_key = ?3",
            rusqlite::params![api_key, provider_id, provider_api_key, now()],
        ).map_err(|e| format!("Failed to save provider key: {}", e))?;
        Ok(())
    }

    /// Get a user's provider API key (for BYOK)
    pub fn get_user_provider_key(&self, api_key: &str, provider_id: &str) -> Option<String> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.query_row(
            "SELECT provider_api_key FROM user_providers WHERE api_key = ?1 AND provider_id = ?2",
            rusqlite::params![api_key, provider_id],
            |row| row.get(0),
        ).ok()
    }

    /// List all user's provider keys (masked)
    pub fn list_user_providers(&self, api_key: &str) -> Vec<serde_json::Value> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = db.prepare(
            "SELECT provider_id, provider_api_key, created_at FROM user_providers WHERE api_key = ?1"
        ).unwrap();
        stmt.query_map(rusqlite::params![api_key], |row| {
            let key: String = row.get(1)?;
            let masked = if key.len() > 8 {
                format!("{}...{}", &key[..4], &key[key.len()-4..])
            } else {
                "****".to_string()
            };
            Ok(serde_json::json!({
                "provider_id": row.get::<_, String>(0)?,
                "masked_key": masked,
                "created_at": row.get::<_, u64>(2)?,
            }))
        }).unwrap().filter_map(|r| r.ok()).collect()
    }

    /// Delete a user's provider key
    pub fn delete_user_provider(&self, api_key: &str, provider_id: &str) -> bool {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.execute(
            "DELETE FROM user_providers WHERE api_key = ?1 AND provider_id = ?2",
            rusqlite::params![api_key, provider_id],
        ).unwrap_or(0) > 0
    }

    // === Pricing ===

    /// Get price for a model (returns cost_per_1k_tokens, price_per_1k_tokens)
    pub fn get_pricing(&self, provider: &str, model: &str) -> (f64, f64) {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.query_row(
            "SELECT cost_per_1k_tokens, price_per_1k_tokens FROM pricing WHERE provider = ?1 AND model = ?2 AND active = 1",
            rusqlite::params![provider, model],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap_or_else(|_| {
            // Default pricing if not configured
            db.query_row(
                "SELECT cost_per_1k_tokens, price_per_1k_tokens FROM pricing WHERE provider = ?1 AND model = 'default' AND active = 1",
                rusqlite::params![provider],
                |row| Ok((row.get(0)?, row.get(1)?)),
            ).unwrap_or((0.001, 0.003)) // fallback: $0.001 cost, $0.003 price
        })
    }

    /// Calculate credit cost for a request (1 credit = $0.001 = 0.1 cent)
    pub fn calculate_credit_cost(&self, provider: &str, model: &str, tokens: u32) -> i64 {
        let (_cost, price) = self.get_pricing(provider, model);
        // price_per_1k_tokens * tokens / 1000, converted to credits (1 credit = $0.001)
        let cost_dollars = price * (tokens as f64) / 1000.0;
        let credits = (cost_dollars / 0.001).ceil() as i64;
        credits.max(1) // minimum 1 credit per request
    }

    /// Set pricing for a model
    pub fn set_pricing(&self, provider: &str, model: &str, cost: f64, price: f64) -> Result<(), String> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.execute(
            "INSERT INTO pricing (provider, model, cost_per_1k_tokens, price_per_1k_tokens) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(provider, model) DO UPDATE SET cost_per_1k_tokens = ?3, price_per_1k_tokens = ?4",
            rusqlite::params![provider, model, cost, price],
        ).map_err(|e| format!("Failed to set pricing: {}", e))?;
        Ok(())
    }

    /// List all pricing
    pub fn list_pricing(&self) -> Vec<serde_json::Value> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = db.prepare(
            "SELECT provider, model, cost_per_1k_tokens, price_per_1k_tokens, active FROM pricing ORDER BY provider, model"
        ).unwrap();
        stmt.query_map([], |row| Ok(serde_json::json!({
            "provider": row.get::<_, String>(0)?,
            "model": row.get::<_, String>(1)?,
            "cost_per_1k_tokens": row.get::<_, f64>(2)?,
            "price_per_1k_tokens": row.get::<_, f64>(3)?,
            "active": row.get::<_, bool>(4)?,
        }))).unwrap().filter_map(|r| r.ok()).collect()
    }

    // === Document management (RAG) ===

    pub fn add_document(&self, tenant_id: &str, filename: &str, content: &str, chunks: &str, chunk_count: usize) -> Result<i64, String> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.execute(
            "INSERT INTO documents (tenant_id, filename, content, chunks, chunk_count, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![tenant_id, filename, content, chunks, chunk_count as i64, now()],
        ).map_err(|e| format!("Failed to add document: {}", e))?;
        Ok(db.last_insert_rowid())
    }

    pub fn list_documents(&self, tenant_id: &str) -> Vec<serde_json::Value> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = db.prepare(
            "SELECT id, filename, chunk_count, created_at FROM documents WHERE tenant_id = ?1 ORDER BY created_at DESC"
        ).unwrap();
        stmt.query_map(rusqlite::params![tenant_id], |row| Ok(serde_json::json!({
            "id": row.get::<_, i64>(0)?,
            "filename": row.get::<_, String>(1)?,
            "chunk_count": row.get::<_, i64>(2)?,
            "created_at": row.get::<_, u64>(3)?,
        }))).unwrap().filter_map(|r| r.ok()).collect()
    }

    pub fn get_document_chunks(&self, tenant_id: &str, doc_id: i64) -> Option<String> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.query_row(
            "SELECT chunks FROM documents WHERE id = ?1 AND tenant_id = ?2",
            rusqlite::params![doc_id, tenant_id],
            |row| row.get(0),
        ).ok()
    }

    pub fn get_all_chunks(&self, tenant_id: &str) -> Vec<String> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = db.prepare(
            "SELECT chunks FROM documents WHERE tenant_id = ?1"
        ).unwrap();
        let chunks_strs: Vec<String> = stmt.query_map(rusqlite::params![tenant_id], |row| {
            row.get::<_, String>(0)
        }).unwrap().filter_map(|r| r.ok()).collect();

        let mut all_chunks = Vec::new();
        for cs in chunks_strs {
            if let Ok(arr) = serde_json::from_str::<Vec<String>>(&cs) {
                all_chunks.extend(arr);
            }
        }
        all_chunks
    }

    /// Get all chunks with their embeddings for hybrid RAG retrieval
    pub fn get_all_chunks_with_embeddings(&self, tenant_id: &str) -> Vec<(String, Option<Vec<f32>>)> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = db.prepare(
            "SELECT chunks, chunk_embeddings FROM documents WHERE tenant_id = ?1"
        ).unwrap();
        let rows: Vec<(String, String)> = stmt.query_map(rusqlite::params![tenant_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1).unwrap_or_else(|_| "[]".to_string())))
        }).unwrap().filter_map(|r| r.ok()).collect();

        let mut result = Vec::new();
        for (chunks_json, embeddings_json) in rows {
            let chunks: Vec<String> = serde_json::from_str(&chunks_json).unwrap_or_default();
            let embeddings: Vec<Vec<f32>> = serde_json::from_str(&embeddings_json).unwrap_or_default();
            for (i, chunk) in chunks.into_iter().enumerate() {
                let emb = embeddings.get(i).cloned();
                result.push((chunk, emb));
            }
        }
        result
    }

    /// Save chunk embeddings for a document (called after generating embeddings)
    pub fn save_chunk_embeddings(&self, doc_id: i64, embeddings: &[Vec<f32>]) -> Result<(), String> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let json = serde_json::to_string(embeddings).unwrap_or_else(|_| "[]".to_string());
        db.execute(
            "UPDATE documents SET chunk_embeddings = ?1 WHERE id = ?2",
            rusqlite::params![json, doc_id],
        ).map_err(|e| format!("Failed to save embeddings: {}", e))?;
        Ok(())
    }

    /// Save memory with embedding and classification
    pub fn save_memory_with_embedding(
        &self, api_key: &str, tenant_id: Option<&str>, name: &str, content: &str,
        memory_type: &str, keywords: &str, embedding: Option<&[f32]>, classification: &str,
    ) -> Result<i64, String> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let emb_blob: Option<Vec<u8>> = embedding.map(|e| {
            e.iter().flat_map(|f| f.to_le_bytes()).collect()
        });
        db.execute(
            "INSERT INTO memories (api_key, tenant_id, name, content, memory_type, keywords, related_ids, embedding, classification, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, '[]', ?7, ?8, ?9)",
            rusqlite::params![api_key, tenant_id, name, content, memory_type, keywords, emb_blob, classification, now()],
        ).map_err(|e| format!("Failed to save memory: {}", e))?;
        Ok(db.last_insert_rowid())
    }

    /// Recall memories with hybrid scoring (keyword + vector)
    pub fn recall_memories_hybrid(
        &self, api_key: &str, tenant_id: Option<&str>, query: &str,
        query_embedding: Option<&[f32]>, max_results: usize,
    ) -> Vec<serde_json::Value> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let query_keywords = extract_keywords(query);

        let (sql, params) = if let Some(tid) = tenant_id {
            (
                "SELECT id, name, content, memory_type, keywords, related_ids, created_at, embedding, classification FROM memories WHERE api_key = ?1 AND tenant_id = ?2 ORDER BY created_at DESC LIMIT 500",
                vec![api_key.to_string(), tid.to_string()],
            )
        } else {
            (
                "SELECT id, name, content, memory_type, keywords, related_ids, created_at, embedding, classification FROM memories WHERE api_key = ?1 AND tenant_id IS NULL ORDER BY created_at DESC LIMIT 500",
                vec![api_key.to_string()],
            )
        };

        let mut stmt = match db.prepare(sql) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let params_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p as &dyn rusqlite::types::ToSql).collect();

        let rows: Vec<(i64, String, String, String, String, String, u64, Option<Vec<u8>>, String)> = stmt.query_map(
            params_refs.as_slice(),
            |row| Ok((
                row.get(0)?, row.get(1)?, row.get(2)?,
                row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?,
                row.get::<_, Option<Vec<u8>>>(7)?,
                row.get::<_, String>(8).unwrap_or_else(|_| "GENERAL".to_string()),
            ))
        ).unwrap_or_else(|_| panic!("query failed"))
        .filter_map(|r| r.ok())
        .collect();

        let mut scored: Vec<(f64, serde_json::Value)> = rows.iter().map(|(id, name, content, mtype, kw_str, related, created_at, emb_blob, classification)| {
            let mem_keywords: Vec<&str> = kw_str.split(',').filter(|s| !s.is_empty()).collect();
            let name_lower = name.to_lowercase();

            // Keyword score
            let mut score: f64 = 0.0;
            for qk in &query_keywords {
                if mem_keywords.iter().any(|mk| mk == qk) { score += 1.0; }
                if name_lower.contains(qk) { score += 2.0; }
            }
            if content.to_lowercase().contains(&query.to_lowercase()) { score += 3.0; }

            // Vector score
            if let (Some(qe), Some(blob)) = (query_embedding, emb_blob.as_ref()) {
                if blob.len() == qe.len() * 4 {
                    let mem_vec: Vec<f32> = blob.chunks_exact(4)
                        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                        .collect();
                    let sim = crate::external_api::cosine_similarity(qe, &mem_vec);
                    score += sim as f64 * 5.0;
                }
            }

            // Recency bonus
            let age_hours = (now().saturating_sub(*created_at)) / 3600;
            if age_hours < 24 { score += 0.5; }

            // Classification boost: KERN memories are more important
            if classification == "KERN" { score += 1.0; }

            (score, serde_json::json!({
                "id": id, "name": name, "content": content,
                "memory_type": mtype, "classification": classification,
                "related_ids": serde_json::from_str::<Vec<i64>>(related).unwrap_or_default(),
                "created_at": created_at,
            }))
        }).collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        let mut results: Vec<serde_json::Value> = scored.into_iter()
            .filter(|(score, _)| *score > 0.0)
            .take(max_results)
            .map(|(_, v)| v)
            .collect();

        if results.is_empty() {
            results = self.recent_memories_inner(&db, api_key, tenant_id, max_results.min(3));
        }
        results
    }

    pub fn delete_document(&self, tenant_id: &str, doc_id: i64) -> bool {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let rows = db.execute(
            "DELETE FROM documents WHERE id = ?1 AND tenant_id = ?2",
            rusqlite::params![doc_id, tenant_id],
        ).unwrap_or(0);
        rows > 0
    }

    /// Count total registered users
    pub fn count_users(&self) -> i64 {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0)).unwrap_or(0)
    }

    /// Detailed user list for admin (includes credits, plan, provider count)
    pub fn list_users_detailed(&self) -> Vec<serde_json::Value> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = db.prepare(
            "SELECT u.id, u.email, u.api_key, COALESCE(u.credits, 0), COALESCE(u.plan, 'free'), u.created_at, k.role, k.active,
                    (SELECT COUNT(*) FROM user_providers up WHERE up.api_key = u.api_key) as provider_count,
                    (SELECT COUNT(*) FROM usage_log ul WHERE ul.api_key = u.api_key) as request_count,
                    (SELECT COALESCE(SUM(ul.tokens_approx), 0) FROM usage_log ul WHERE ul.api_key = u.api_key) as total_tokens
             FROM users u JOIN api_keys k ON u.api_key = k.key ORDER BY u.created_at DESC"
        ).unwrap();
        stmt.query_map([], |row| Ok(serde_json::json!({
            "id": row.get::<_, i64>(0)?,
            "email": row.get::<_, String>(1)?,
            "api_key": row.get::<_, String>(2)?,
            "credits": row.get::<_, i64>(3)?,
            "plan": row.get::<_, String>(4)?,
            "created_at": row.get::<_, u64>(5)?,
            "role": row.get::<_, String>(6)?,
            "active": row.get::<_, bool>(7)?,
            "provider_count": row.get::<_, i64>(8)?,
            "request_count": row.get::<_, i64>(9)?,
            "total_tokens": row.get::<_, i64>(10)?,
        }))).unwrap().filter_map(|r| r.ok()).collect()
    }

    /// Usage stats summary for admin dashboard
    pub fn get_usage_stats_summary(&self) -> serde_json::Value {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let total_requests: i64 = db.query_row("SELECT COUNT(*) FROM usage_log", [], |row| row.get(0)).unwrap_or(0);
        let total_tokens: i64 = db.query_row("SELECT COALESCE(SUM(tokens_approx), 0) FROM usage_log", [], |row| row.get(0)).unwrap_or(0);
        let total_credits_spent: i64 = db.query_row(
            "SELECT COALESCE(SUM(ABS(amount)), 0) FROM credit_transactions WHERE amount < 0", [], |row| row.get(0)
        ).unwrap_or(0);
        let total_credits_purchased: i64 = db.query_row(
            "SELECT COALESCE(SUM(amount), 0) FROM credit_transactions WHERE amount > 0", [], |row| row.get(0)
        ).unwrap_or(0);
        let requests_24h: i64 = db.query_row(
            "SELECT COUNT(*) FROM usage_log WHERE created_at > ?1",
            rusqlite::params![now() - 86400], |row| row.get(0)
        ).unwrap_or(0);

        serde_json::json!({
            "total_requests": total_requests,
            "total_tokens": total_tokens,
            "total_credits_spent": total_credits_spent,
            "total_credits_purchased": total_credits_purchased,
            "requests_24h": requests_24h,
        })
    }

    // === Feedback ===

    /// Submit feedback from a user
    pub fn submit_feedback(&self, api_key: Option<&str>, email: Option<&str>, category: &str, message: &str, page: Option<&str>, user_agent: Option<&str>) -> Result<i64, String> {
        if message.trim().is_empty() {
            return Err("Message cannot be empty".to_string());
        }
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.execute(
            "INSERT INTO feedback (api_key, email, category, message, page, user_agent, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'new', ?7)",
            rusqlite::params![api_key, email, category, message.trim(), page, user_agent, now()],
        ).map_err(|e| format!("Failed to submit feedback: {}", e))?;
        Ok(db.last_insert_rowid())
    }

    /// List all feedback (admin)
    pub fn list_feedback(&self, status_filter: Option<&str>) -> Vec<serde_json::Value> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let (sql, params): (String, Vec<String>) = match status_filter {
            Some(s) => (
                "SELECT id, api_key, email, category, message, page, user_agent, status, admin_note, created_at FROM feedback WHERE status = ?1 ORDER BY created_at DESC LIMIT 100".to_string(),
                vec![s.to_string()],
            ),
            None => (
                "SELECT id, api_key, email, category, message, page, user_agent, status, admin_note, created_at FROM feedback ORDER BY created_at DESC LIMIT 100".to_string(),
                vec![],
            ),
        };
        let mut stmt = db.prepare(&sql).unwrap();
        let params_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p as &dyn rusqlite::types::ToSql).collect();
        stmt.query_map(params_refs.as_slice(), |row| Ok(serde_json::json!({
            "id": row.get::<_, i64>(0)?,
            "api_key": row.get::<_, Option<String>>(1)?,
            "email": row.get::<_, Option<String>>(2)?,
            "category": row.get::<_, String>(3)?,
            "message": row.get::<_, String>(4)?,
            "page": row.get::<_, Option<String>>(5)?,
            "user_agent": row.get::<_, Option<String>>(6)?,
            "status": row.get::<_, String>(7)?,
            "admin_note": row.get::<_, Option<String>>(8)?,
            "created_at": row.get::<_, u64>(9)?,
        }))).unwrap().filter_map(|r| r.ok()).collect()
    }

    /// Update feedback status (admin)
    pub fn update_feedback(&self, feedback_id: i64, status: &str, admin_note: Option<&str>) -> bool {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.execute(
            "UPDATE feedback SET status = ?1, admin_note = ?2 WHERE id = ?3",
            rusqlite::params![status, admin_note, feedback_id],
        ).unwrap_or(0) > 0
    }

    /// Count new feedback (for admin badge)
    pub fn count_new_feedback(&self) -> i64 {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.query_row("SELECT COUNT(*) FROM feedback WHERE status = 'new'", [], |row| row.get(0)).unwrap_or(0)
    }

    /// Get detailed activity for a specific user (admin view)
    pub fn get_user_activity(&self, target_api_key: &str) -> serde_json::Value {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());

        // Recent requests
        let mut stmt = db.prepare(
            "SELECT source, model, tokens_approx, latency_ms, success, created_at FROM usage_log WHERE api_key = ?1 ORDER BY created_at DESC LIMIT 50"
        ).unwrap();
        let requests: Vec<serde_json::Value> = stmt.query_map(rusqlite::params![target_api_key], |row| Ok(serde_json::json!({
            "source": row.get::<_, String>(0)?,
            "model": row.get::<_, Option<String>>(1)?,
            "tokens": row.get::<_, i64>(2)?,
            "latency_ms": row.get::<_, i64>(3)?,
            "success": row.get::<_, bool>(4)?,
            "created_at": row.get::<_, u64>(5)?,
        }))).unwrap().filter_map(|r| r.ok()).collect();

        // Chats
        let mut stmt2 = db.prepare(
            "SELECT id, title, model, created_at, updated_at FROM chats WHERE api_key = ?1 ORDER BY updated_at DESC LIMIT 20"
        ).unwrap();
        let chats: Vec<serde_json::Value> = stmt2.query_map(rusqlite::params![target_api_key], |row| Ok(serde_json::json!({
            "id": row.get::<_, String>(0)?,
            "title": row.get::<_, String>(1)?,
            "model": row.get::<_, Option<String>>(2)?,
            "created_at": row.get::<_, u64>(3)?,
            "updated_at": row.get::<_, u64>(4)?,
        }))).unwrap().filter_map(|r| r.ok()).collect();

        // Memories
        let mut stmt3 = db.prepare(
            "SELECT id, name, memory_type, created_at FROM memories WHERE api_key = ?1 ORDER BY created_at DESC LIMIT 20"
        ).unwrap();
        let memories: Vec<serde_json::Value> = stmt3.query_map(rusqlite::params![target_api_key], |row| Ok(serde_json::json!({
            "id": row.get::<_, i64>(0)?,
            "name": row.get::<_, String>(1)?,
            "type": row.get::<_, String>(2)?,
            "created_at": row.get::<_, u64>(3)?,
        }))).unwrap().filter_map(|r| r.ok()).collect();

        // Profile
        let profile = self.get_user_profile(target_api_key);

        // Provider keys (BYOK)
        let providers = self.list_user_providers(target_api_key);

        serde_json::json!({
            "profile": profile,
            "recent_requests": requests,
            "request_count": requests.len(),
            "chats": chats,
            "chat_count": chats.len(),
            "memories": memories,
            "memory_count": memories.len(),
            "providers": providers,
        })
    }

    /// Create a new API key with Standard role
    pub async fn create_key(&self, name: &str) -> String {
        self.create_key_with_role(name, KeyRole::Standard).await
    }

    /// Create a new API key with a specific role
    pub async fn create_key_with_role(&self, name: &str, role: KeyRole) -> String {
        let key = generate_api_key();
        let role_str = match role { KeyRole::Admin => "admin", KeyRole::Standard => "standard" };
        let ts = now();
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.execute(
            "INSERT INTO api_keys (key, name, role, active, created_at) VALUES (?1, ?2, ?3, 1, ?4)",
            rusqlite::params![key, name, role_str, ts],
        ).expect("Failed to insert key");
        info!("[AUTH] Key created: {} (role: {}, name: {})", key, role_str, name);
        key
    }

    /// Check if an admin key already exists (to avoid regenerating on every restart)
    pub async fn has_admin_key(&self) -> bool {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let count: i64 = db.query_row(
            "SELECT COUNT(*) FROM api_keys WHERE role = 'admin' AND active = 1",
            [],
            |row| row.get(0),
        ).unwrap_or(0);
        count > 0
    }

    /// Get the existing admin key
    pub async fn get_admin_key(&self) -> Option<String> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.query_row(
            "SELECT key FROM api_keys WHERE role = 'admin' AND active = 1 LIMIT 1",
            [],
            |row| row.get(0),
        ).ok()
    }

    /// Validate an API key
    pub async fn validate(&self, key: &str) -> bool {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let active: Option<bool> = db.query_row(
            "SELECT active FROM api_keys WHERE key = ?1",
            rusqlite::params![key],
            |row| row.get(0),
        ).ok();
        active.unwrap_or(false)
    }

    /// Check if a key has admin role
    pub async fn is_admin(&self, key: &str) -> bool {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let result: Option<(bool, String)> = db.query_row(
            "SELECT active, role FROM api_keys WHERE key = ?1",
            rusqlite::params![key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).ok();
        match result {
            Some((true, role)) => role == "admin",
            _ => false,
        }
    }

    /// Revoke an API key
    pub async fn revoke(&self, key: &str) -> bool {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let rows = db.execute(
            "UPDATE api_keys SET active = 0 WHERE key = ?1",
            rusqlite::params![key],
        ).unwrap_or(0);
        rows > 0
    }

    /// Get existing key by name, or create a new one (idempotent, sync)
    pub fn get_or_create_key_sync(&self, name: &str) -> String {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let existing: Option<String> = db.query_row(
            "SELECT key FROM api_keys WHERE name = ?1 AND active = 1 LIMIT 1",
            rusqlite::params![name],
            |row| row.get(0),
        ).ok();
        if let Some(key) = existing {
            info!("[AUTH] Existing key found for '{}': {}", name, key);
            return key;
        }
        // Create new key inline (all SQLite ops are sync)
        let key = generate_api_key();
        let ts = now();
        db.execute(
            "INSERT INTO api_keys (key, name, role, active, created_at) VALUES (?1, ?2, 'standard', 1, ?3)",
            rusqlite::params![key, name, ts],
        ).expect("Failed to insert key");
        info!("[AUTH] Key created: {} (role: standard, name: {})", key, name);
        key
    }

    /// List all active keys (for admin dashboard)
    pub async fn list_keys(&self) -> Vec<KeyInfo> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = db.prepare(
            "SELECT name, created_at, active, role FROM api_keys WHERE active = 1 ORDER BY created_at DESC"
        ).unwrap();
        stmt.query_map([], |row| {
            let role_str: String = row.get(3)?;
            Ok(KeyInfo {
                name: row.get(0)?,
                created_at: row.get(1)?,
                active: row.get(2)?,
                role: if role_str == "admin" { KeyRole::Admin } else { KeyRole::Standard },
            })
        }).unwrap().filter_map(|r| r.ok()).collect()
    }

    // =========================================================================
    // Long-term Memory (PepAI-style)
    // =========================================================================

    /// Store a memory (FACT or SESSION)
    pub fn store_memory(
        &self,
        api_key: &str,
        tenant_id: Option<&str>,
        name: &str,
        content: &str,
        memory_type: &str,
    ) -> Result<i64, String> {
        if content.trim().is_empty() {
            return Err("Content cannot be empty".to_string());
        }
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let ts = now();

        // Extract keywords for retrieval
        let keywords = extract_keywords(content);
        let keywords_str = keywords.join(",");

        // Find related memories (keyword overlap, top 3)
        let related_ids = self.find_related_ids_inner(&db, api_key, tenant_id, &keywords, 3);
        let related_json = serde_json::to_string(&related_ids).unwrap_or_else(|_| "[]".to_string());

        db.execute(
            "INSERT INTO memories (api_key, tenant_id, name, content, memory_type, keywords, related_ids, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![api_key, tenant_id, name, content, memory_type, keywords_str, related_json, ts],
        ).map_err(|e| format!("Failed to store memory: {}", e))?;

        let id = db.last_insert_rowid();
        info!("[MEMORY] Stored: '{}' (type={}, keywords={}, related={:?})", name, memory_type, keywords.len(), related_ids);
        Ok(id)
    }

    /// Recall relevant memories using keyword matching (hybrid: name + content)
    pub fn recall_memories(
        &self,
        api_key: &str,
        tenant_id: Option<&str>,
        query: &str,
        max_results: usize,
    ) -> Vec<serde_json::Value> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let query_keywords = extract_keywords(query);
        if query_keywords.is_empty() {
            // Fallback: return most recent memories
            return self.recent_memories_inner(&db, api_key, tenant_id, max_results);
        }

        // Fetch all memories for this scope
        let (sql, params) = if let Some(tid) = tenant_id {
            (
                "SELECT id, name, content, memory_type, keywords, related_ids, created_at FROM memories WHERE api_key = ?1 AND tenant_id = ?2 ORDER BY created_at DESC LIMIT 500",
                vec![api_key.to_string(), tid.to_string()],
            )
        } else {
            (
                "SELECT id, name, content, memory_type, keywords, related_ids, created_at FROM memories WHERE api_key = ?1 AND tenant_id IS NULL ORDER BY created_at DESC LIMIT 500",
                vec![api_key.to_string()],
            )
        };

        let mut stmt = match db.prepare(sql) {
            Ok(s) => s,
            Err(_) => return vec![],
        };

        let params_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p as &dyn rusqlite::types::ToSql).collect();

        let rows: Vec<(i64, String, String, String, String, String, u64)> = stmt.query_map(
            params_refs.as_slice(),
            |row| Ok((
                row.get(0)?, row.get(1)?, row.get(2)?,
                row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?,
            ))
        ).unwrap_or_else(|_| panic!("query failed"))
        .filter_map(|r| r.ok())
        .collect();

        // Score by keyword overlap
        let mut scored: Vec<(f64, serde_json::Value)> = rows.iter().map(|(id, name, content, mtype, kw_str, related, created_at)| {
            let mem_keywords: Vec<&str> = kw_str.split(',').filter(|s| !s.is_empty()).collect();
            let name_lower = name.to_lowercase();

            let mut score: f64 = 0.0;
            for qk in &query_keywords {
                if mem_keywords.iter().any(|mk| mk == qk) { score += 1.0; }
                if name_lower.contains(qk) { score += 2.0; }
            }
            // Exact phrase bonus
            if content.to_lowercase().contains(&query.to_lowercase()) { score += 3.0; }
            // Recency bonus (small)
            let age_hours = (now().saturating_sub(*created_at)) / 3600;
            if age_hours < 24 { score += 0.5; }

            (score, serde_json::json!({
                "id": id,
                "name": name,
                "content": content,
                "memory_type": mtype,
                "related_ids": serde_json::from_str::<Vec<i64>>(related).unwrap_or_default(),
                "created_at": created_at,
            }))
        }).collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        let mut results: Vec<serde_json::Value> = scored.into_iter()
            .filter(|(score, _)| *score > 0.0)
            .take(max_results)
            .map(|(_, v)| v)
            .collect();

        // Fallback: if no keyword matches, return most recent
        if results.is_empty() {
            results = self.recent_memories_inner(&db, api_key, tenant_id, max_results.min(3));
        }

        results
    }

    /// List all memories for an API key (optionally filtered by tenant)
    pub fn list_memories(&self, api_key: &str, tenant_id: Option<&str>) -> Vec<serde_json::Value> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let (sql, params): (&str, Vec<String>) = if let Some(tid) = tenant_id {
            (
                "SELECT id, name, content, memory_type, keywords, related_ids, created_at FROM memories WHERE api_key = ?1 AND tenant_id = ?2 ORDER BY created_at DESC LIMIT 100",
                vec![api_key.to_string(), tid.to_string()],
            )
        } else {
            (
                "SELECT id, name, content, memory_type, keywords, related_ids, created_at FROM memories WHERE api_key = ?1 ORDER BY created_at DESC LIMIT 100",
                vec![api_key.to_string()],
            )
        };

        let mut stmt = match db.prepare(sql) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let params_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p as &dyn rusqlite::types::ToSql).collect();
        stmt.query_map(params_refs.as_slice(), |row| {
            let related_str: String = row.get(5)?;
            Ok(serde_json::json!({
                "id": row.get::<_, i64>(0)?,
                "name": row.get::<_, String>(1)?,
                "content": row.get::<_, String>(2)?,
                "memory_type": row.get::<_, String>(3)?,
                "keywords": row.get::<_, String>(4)?,
                "related_ids": serde_json::from_str::<Vec<i64>>(&related_str).unwrap_or_default(),
                "created_at": row.get::<_, u64>(6)?,
            }))
        }).unwrap_or_else(|_| panic!("query failed"))
        .filter_map(|r| r.ok())
        .collect()
    }

    /// Delete a specific memory
    pub fn delete_memory(&self, api_key: &str, memory_id: i64) -> bool {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let rows = db.execute(
            "DELETE FROM memories WHERE id = ?1 AND api_key = ?2",
            rusqlite::params![memory_id, api_key],
        ).unwrap_or(0);
        rows > 0
    }

    /// Count memories for an API key
    pub fn count_memories(&self, api_key: &str, tenant_id: Option<&str>) -> i64 {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(tid) = tenant_id {
            db.query_row(
                "SELECT COUNT(*) FROM memories WHERE api_key = ?1 AND tenant_id = ?2",
                rusqlite::params![api_key, tid],
                |row| row.get(0),
            ).unwrap_or(0)
        } else {
            db.query_row(
                "SELECT COUNT(*) FROM memories WHERE api_key = ?1",
                rusqlite::params![api_key],
                |row| row.get(0),
            ).unwrap_or(0)
        }
    }

    /// Auto-archive: store a chat session as a SESSION memory
    pub fn auto_archive_session(
        &self,
        api_key: &str,
        tenant_id: Option<&str>,
        messages: &[serde_json::Value],
    ) {
        if messages.is_empty() { return; }
        // Compress messages into a summary string (skip system messages to prevent feedback loops)
        let content: String = messages.iter()
            .filter_map(|m| {
                let role = m["role"].as_str()?;
                if role == "system" { return None; }
                let text = m["content"].as_str()?;
                if text.len() > 10 {
                    Some(format!("{}: {}", role, text))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        if content.is_empty() { return; }

        let name = format!("SESSION_{}", now());
        let _ = self.store_memory(api_key, tenant_id, &name, &content, "SESSION");
    }

    // --- Internal helpers ---

    fn find_related_ids_inner(
        &self,
        db: &Connection,
        api_key: &str,
        tenant_id: Option<&str>,
        keywords: &[String],
        max: usize,
    ) -> Vec<i64> {
        if keywords.is_empty() { return vec![]; }
        let (sql, params): (&str, Vec<String>) = if let Some(tid) = tenant_id {
            (
                "SELECT id, keywords FROM memories WHERE api_key = ?1 AND tenant_id = ?2 ORDER BY created_at DESC LIMIT 200",
                vec![api_key.to_string(), tid.to_string()],
            )
        } else {
            (
                "SELECT id, keywords FROM memories WHERE api_key = ?1 AND tenant_id IS NULL ORDER BY created_at DESC LIMIT 200",
                vec![api_key.to_string()],
            )
        };

        let mut stmt = match db.prepare(sql) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let params_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p as &dyn rusqlite::types::ToSql).collect();
        let rows: Vec<(i64, String)> = stmt.query_map(params_refs.as_slice(), |row| {
            Ok((row.get(0)?, row.get(1)?))
        }).unwrap_or_else(|_| panic!("query failed"))
        .filter_map(|r| r.ok())
        .collect();

        let mut scored: Vec<(usize, i64)> = rows.iter().map(|(id, kw_str)| {
            let mem_kw: Vec<&str> = kw_str.split(',').filter(|s| !s.is_empty()).collect();
            let overlap = keywords.iter().filter(|k| mem_kw.contains(&k.as_str())).count();
            (overlap, *id)
        }).filter(|(o, _)| *o > 0).collect();

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.into_iter().take(max).map(|(_, id)| id).collect()
    }

    fn recent_memories_inner(
        &self,
        db: &Connection,
        api_key: &str,
        tenant_id: Option<&str>,
        max: usize,
    ) -> Vec<serde_json::Value> {
        let (sql, params): (&str, Vec<String>) = if let Some(tid) = tenant_id {
            (
                "SELECT id, name, content, memory_type, related_ids, created_at FROM memories WHERE api_key = ?1 AND tenant_id = ?2 ORDER BY created_at DESC LIMIT ?3",
                vec![api_key.to_string(), tid.to_string()],
            )
        } else {
            (
                "SELECT id, name, content, memory_type, related_ids, created_at FROM memories WHERE api_key = ?1 AND tenant_id IS NULL ORDER BY created_at DESC LIMIT ?2",
                vec![api_key.to_string()],
            )
        };

        let mut stmt = match db.prepare(sql) {
            Ok(s) => s,
            Err(_) => return vec![],
        };

        // Build params with max as i64
        let max_i64 = max as i64;
        let mut all_params: Vec<Box<dyn rusqlite::types::ToSql>> = params.iter()
            .map(|p| Box::new(p.clone()) as Box<dyn rusqlite::types::ToSql>)
            .collect();
        all_params.push(Box::new(max_i64));

        let params_refs: Vec<&dyn rusqlite::types::ToSql> = all_params.iter().map(|p| p.as_ref()).collect();

        stmt.query_map(params_refs.as_slice(), |row| {
            let related_str: String = row.get(4)?;
            Ok(serde_json::json!({
                "id": row.get::<_, i64>(0)?,
                "name": row.get::<_, String>(1)?,
                "content": row.get::<_, String>(2)?,
                "memory_type": row.get::<_, String>(3)?,
                "related_ids": serde_json::from_str::<Vec<i64>>(&related_str).unwrap_or_default(),
                "created_at": row.get::<_, u64>(5)?,
            }))
        }).unwrap_or_else(|_| panic!("query failed"))
        .filter_map(|r| r.ok())
        .collect()
    }

    // === PepAI Cognitive State ===

    pub fn get_pepai_state(&self, api_key: &str) -> crate::pepai::pipeline::PepaiState {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.query_row(
            "SELECT state_json FROM pepai_state WHERE api_key = ?1",
            rusqlite::params![api_key],
            |row| {
                let json: String = row.get(0)?;
                Ok(serde_json::from_str(&json).unwrap_or_else(|_| crate::pepai::pipeline::PepaiState::new()))
            },
        ).unwrap_or_else(|_| crate::pepai::pipeline::PepaiState::new())
    }

    pub fn save_pepai_state(&self, api_key: &str, state: &crate::pepai::pipeline::PepaiState) {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(json) = serde_json::to_string(state) {
            let _ = db.execute(
                "INSERT INTO pepai_state (api_key, state_json, updated_at) VALUES (?1, ?2, ?3) ON CONFLICT(api_key) DO UPDATE SET state_json = ?2, updated_at = ?3",
                rusqlite::params![api_key, json, now()],
            );
        }
    }
}

pub fn generate_cluster_secret() -> String {
    use rand::RngCore;
    let mut buf = [0u8; 32];
    rand::rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

fn generate_api_key() -> String {
    let timestamp = now();
    let random_bytes: [u8; 16] = rand_bytes();
    let hash = blake3::hash(&random_bytes);
    format!("hel-{}-{}", timestamp % 10000, &hash.to_hex()[..24])
}

fn rand_bytes() -> [u8; 16] {
    use rand::RngCore;
    let mut buf = [0u8; 16];
    rand::rng().fill_bytes(&mut buf);
    buf
}

pub struct TenantUpdate {
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

pub struct TenantPublic {
    pub id: String,
    pub api_key: String,
    pub name: String,
    pub faq: String,
    pub system_prompt: String,
    pub welcome_message: String,
    pub model: String,
    pub color_primary: String,
    pub color_bg: String,
    pub color_text: String,
    pub bot_type: String,
    pub tools: String,
    pub tool_config: String,
    pub max_messages_per_hour: i64,
    pub active: bool,
}

fn generate_tenant_id() -> String {
    let random_bytes: [u8; 8] = {
        use rand::RngCore;
        let mut buf = [0u8; 8];
        rand::rng().fill_bytes(&mut buf);
        buf
    };
    format!("tn-{}", &hex::encode(random_bytes)[..12])
}

/// Extract meaningful keywords from text (for memory indexing + retrieval)
fn extract_keywords(text: &str) -> Vec<String> {
    let stop_words = ["de", "het", "een", "van", "in", "is", "dat", "die", "voor", "met",
        "op", "aan", "als", "naar", "maar", "ook", "nog", "wel", "niet", "zijn",
        "was", "heeft", "kan", "zou", "moet", "wordt", "deze", "dit", "the", "and",
        "for", "with", "that", "this", "from", "are", "was", "have", "has", "been",
        "will", "would", "could", "should", "user", "assistant", "system"];
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 3 && !stop_words.contains(w))
        .map(|w| w.to_string())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect()
}

fn hash_password(password: &str, salt: &str) -> String {
    let input = format!("{}:{}", salt, password);
    blake3::hash(input.as_bytes()).to_hex().to_string()
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
