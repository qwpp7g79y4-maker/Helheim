use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

/// Simple in-memory rate limiter using sliding window counters
pub struct RateLimiter {
    /// key -> (count, window_start)
    windows: Mutex<HashMap<String, (u32, Instant)>>,
    /// Max requests per window
    max_requests: u32,
    /// Window duration in seconds
    window_secs: u64,
}

impl RateLimiter {
    pub fn new(max_requests: u32, window_secs: u64) -> Self {
        Self {
            windows: Mutex::new(HashMap::new()),
            max_requests,
            window_secs,
        }
    }

    /// Check if a request is allowed. Returns (allowed, remaining, retry_after_secs)
    pub fn check(&self, key: &str) -> (bool, u32, u64) {
        let mut windows = self.windows.lock().unwrap();
        let now = Instant::now();
        let window_duration = std::time::Duration::from_secs(self.window_secs);

        let entry = windows.entry(key.to_string()).or_insert((0, now));

        // Reset window if expired
        if now.duration_since(entry.1) >= window_duration {
            entry.0 = 0;
            entry.1 = now;
        }

        if entry.0 >= self.max_requests {
            let elapsed = now.duration_since(entry.1).as_secs();
            let retry_after = self.window_secs.saturating_sub(elapsed);
            return (false, 0, retry_after);
        }

        entry.0 += 1;
        let remaining = self.max_requests.saturating_sub(entry.0);
        (true, remaining, 0)
    }

    /// Clean up expired entries (call periodically)
    pub fn cleanup(&self) {
        let mut windows = self.windows.lock().unwrap();
        let now = Instant::now();
        let window_duration = std::time::Duration::from_secs(self.window_secs);
        windows.retain(|_, (_, start)| now.duration_since(*start) < window_duration);
    }
}
