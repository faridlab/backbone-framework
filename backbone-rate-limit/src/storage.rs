//! Storage backends for rate limiting

use crate::types::RateLimitConfig;
use crate::RateLimitResult;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Trait for storage backends
#[async_trait::async_trait]
pub trait StorageBackend: Send + Sync {
    async fn increment(&self, key: &str, config: &RateLimitConfig) -> RateLimitResult<u64>;
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

#[async_trait::async_trait]
impl StorageBackend for InMemoryStorage {
    async fn increment(&self, key: &str, config: &RateLimitConfig) -> RateLimitResult<u64> {
        let mut data = self.data.write().await;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let entry = data.entry(key.to_string());
        let state = entry.or_insert_with(|| CounterState {
            count: 0,
            window_start: now,
        });

        if now - state.window_start > config.window_seconds {
            state.count = 1;
            state.window_start = now;
        } else {
            state.count = state.count.saturating_add(1);
        }

        Ok(state.count)
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
