use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct InMemoryRateLimitStore {
    buckets: Mutex<HashMap<String, (u32, Instant)>>,
}

impl InMemoryRateLimitStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn check(&self, key: &str, limit: u32, window: Duration) -> bool {
        let mut buckets = self.buckets.lock().unwrap();
        let now = Instant::now();

        buckets.retain(|_, (_, started)| now.duration_since(*started) < window);

        let entry = buckets.entry(key.to_string()).or_insert((0, now));
        if now.duration_since(entry.1) >= window {
            *entry = (0, now);
        }
        entry.0 += 1;
        entry.0 <= limit
    }
}

impl Default for InMemoryRateLimitStore {
    fn default() -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
        }
    }
}
