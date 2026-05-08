//! Storage backends for rate limiting

use crate::types::{IncrementOutcome, RateLimitConfig};
use crate::RateLimitResult;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Trait for storage backends
#[async_trait::async_trait]
pub trait StorageBackend: Send + Sync {
    /// Increment the counter for `key` and return the post-increment
    /// outcome. When the config carries `lockout_seconds` and exceeding
    /// `max_requests` triggers a lockout, the returned `IncrementOutcome`
    /// carries `locked_until`. Subsequent calls during lockout are
    /// no-ops and return the same `locked_until` timestamp.
    async fn increment(
        &self,
        key: &str,
        config: &RateLimitConfig,
    ) -> RateLimitResult<IncrementOutcome>;

    async fn get_count(&self, key: &str, config: &RateLimitConfig) -> RateLimitResult<u64>;
    async fn reset(&self, key: &str, config: &RateLimitConfig) -> RateLimitResult<()>;
}

#[derive(Clone)]
pub struct InMemoryStorage {
    data: Arc<RwLock<HashMap<String, CounterState>>>,
}

#[derive(Clone)]
struct CounterState {
    count: u64,
    window_start: u64,
    locked_until: Option<u64>,
}

impl InMemoryStorage {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[async_trait::async_trait]
impl StorageBackend for InMemoryStorage {
    async fn increment(
        &self,
        key: &str,
        config: &RateLimitConfig,
    ) -> RateLimitResult<IncrementOutcome> {
        let mut data = self.data.write().await;
        let now = now_unix();

        let state = data.entry(key.to_string()).or_insert_with(|| CounterState {
            count: 0,
            window_start: now,
            locked_until: None,
        });

        // Honor any active lockout: don't increment, just report the lock.
        if let Some(until) = state.locked_until {
            if until > now {
                return Ok(IncrementOutcome {
                    count: state.count,
                    locked_until: Some(until),
                });
            }
            // Lockout expired — start a fresh window.
            state.locked_until = None;
            state.count = 0;
            state.window_start = now;
        }

        if now.saturating_sub(state.window_start) > config.window_seconds {
            state.count = 1;
            state.window_start = now;
        } else {
            state.count = state.count.saturating_add(1);
        }

        // Trigger lockout once the window count exceeds max_requests.
        if state.count > config.max_requests {
            if let Some(lockout) = config.lockout_seconds {
                state.locked_until = Some(now + lockout);
            }
        }

        Ok(IncrementOutcome {
            count: state.count,
            locked_until: state.locked_until,
        })
    }

    async fn get_count(&self, key: &str, _config: &RateLimitConfig) -> RateLimitResult<u64> {
        let data = self.data.read().await;
        Ok(data.get(key).map(|s| s.count).unwrap_or(0))
    }

    async fn reset(&self, key: &str, _config: &RateLimitConfig) -> RateLimitResult<()> {
        let mut data = self.data.write().await;
        data.remove(key);
        Ok(())
    }
}
