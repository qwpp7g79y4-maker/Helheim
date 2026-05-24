use std::sync::Mutex;
use std::collections::VecDeque;
use serde::Serialize;

const MAX_EVENTS: usize = 500;

#[derive(Debug, Clone, Serialize)]
pub struct Event {
    pub ts: u64,
    pub kind: String,       // "inference", "tool_call", "tool_result", "error", "auth", "upload", "agent"
    pub source: String,     // "chat", "widget", "demo", "api", "rag"
    pub detail: String,     // human-readable summary
    pub meta: serde_json::Value, // structured data (model, tokens, latency, etc.)
    pub duration_ms: Option<u64>,
    pub success: bool,
}

pub struct EventLog {
    events: Mutex<VecDeque<Event>>,
    /// Usage counters: total inferences, total tool calls, total errors, total tokens (approx)
    counters: Mutex<Counters>,
}

#[derive(Default, Clone, Serialize)]
pub struct Counters {
    pub total_inferences: u64,
    pub total_tool_calls: u64,
    pub total_errors: u64,
    pub total_tokens_approx: u64,
    pub total_rag_queries: u64,
    pub total_agent_runs: u64,
}

impl EventLog {
    pub fn new() -> Self {
        Self {
            events: Mutex::new(VecDeque::with_capacity(MAX_EVENTS)),
            counters: Mutex::new(Counters::default()),
        }
    }

    pub fn log(&self, kind: &str, source: &str, detail: &str, meta: serde_json::Value, duration_ms: Option<u64>, success: bool) {
        let event = Event {
            ts: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
            kind: kind.to_string(),
            source: source.to_string(),
            detail: detail.to_string(),
            meta,
            duration_ms,
            success,
        };

        // Update counters
        {
            let mut c = self.counters.lock().unwrap();
            match kind {
                "inference" => {
                    c.total_inferences += 1;
                    // Rough token estimate from output length
                    if let Some(tokens) = meta_tokens(&event.meta) {
                        c.total_tokens_approx += tokens;
                    }
                }
                "tool_call" | "tool_result" => c.total_tool_calls += 1,
                "error" => c.total_errors += 1,
                "rag_query" => c.total_rag_queries += 1,
                "agent" => c.total_agent_runs += 1,
                _ => {}
            }
        }

        let mut events = self.events.lock().unwrap();
        if events.len() >= MAX_EVENTS {
            events.pop_front();
        }
        events.push_back(event);
    }

    /// Get recent events, optionally filtered by kind
    pub fn recent(&self, limit: usize, kind_filter: Option<&str>) -> Vec<Event> {
        let events = self.events.lock().unwrap();
        events.iter()
            .rev()
            .filter(|e| kind_filter.map_or(true, |k| e.kind == k))
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get all events since a given timestamp
    pub fn since(&self, ts: u64) -> Vec<Event> {
        let events = self.events.lock().unwrap();
        events.iter()
            .filter(|e| e.ts >= ts)
            .cloned()
            .collect()
    }

    pub fn counters(&self) -> Counters {
        self.counters.lock().unwrap().clone()
    }
}

fn meta_tokens(meta: &serde_json::Value) -> Option<u64> {
    meta.get("output_len")
        .and_then(|v| v.as_u64())
        .map(|len| len / 4) // rough: 1 token ≈ 4 chars
}
