use std::collections::HashMap;
use std::sync::Mutex;

/// Tracks active user sessions in-memory
pub struct SessionTracker {
    sessions: Mutex<HashMap<String, SessionInfo>>,
}

#[derive(Clone, serde::Serialize)]
pub struct SessionInfo {
    pub api_key: String,
    pub email: Option<String>,
    pub first_seen: u64,
    pub last_seen: u64,
    pub request_count: u64,
    pub tokens_used: u64,
    pub last_model: Option<String>,
    pub last_page: Option<String>,
    pub ip: Option<String>,
}

impl SessionTracker {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    /// Record activity for a user
    pub fn touch(&self, api_key: &str, email: Option<&str>, model: Option<&str>, page: Option<&str>, ip: Option<&str>, tokens: u64) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut sessions = self.sessions.lock().unwrap();
        let entry = sessions.entry(api_key.to_string()).or_insert_with(|| SessionInfo {
            api_key: api_key.to_string(),
            email: email.map(|s| s.to_string()),
            first_seen: now,
            last_seen: now,
            request_count: 0,
            tokens_used: 0,
            last_model: None,
            last_page: None,
            ip: None,
        });

        entry.last_seen = now;
        entry.request_count += 1;
        entry.tokens_used += tokens;
        if let Some(e) = email {
            entry.email = Some(e.to_string());
        }
        if let Some(m) = model {
            entry.last_model = Some(m.to_string());
        }
        if let Some(p) = page {
            entry.last_page = Some(p.to_string());
        }
        if let Some(i) = ip {
            entry.ip = Some(i.to_string());
        }
    }

    /// Get all sessions, optionally filtering to only "active" ones (seen in last N seconds)
    pub fn get_sessions(&self, active_within_secs: Option<u64>) -> Vec<SessionInfo> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let sessions = self.sessions.lock().unwrap();
        let mut result: Vec<SessionInfo> = sessions.values()
            .filter(|s| {
                if let Some(window) = active_within_secs {
                    now.saturating_sub(s.last_seen) <= window
                } else {
                    true
                }
            })
            .cloned()
            .collect();

        // Sort by last_seen descending
        result.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
        result
    }

    /// Count currently active users (seen in last N seconds)
    pub fn active_count(&self, window_secs: u64) -> usize {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let sessions = self.sessions.lock().unwrap();
        sessions.values()
            .filter(|s| now.saturating_sub(s.last_seen) <= window_secs)
            .count()
    }

    /// Cleanup old sessions (not seen in last hour)
    pub fn cleanup(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut sessions = self.sessions.lock().unwrap();
        sessions.retain(|_, s| now.saturating_sub(s.last_seen) < 3600);
    }
}
