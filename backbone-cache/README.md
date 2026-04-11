# Backbone Cache

[![Crates.io](https://img.shields.io/crates/v/backbone-cache)](https://crates.io/crates/backbone-cache)
[![Documentation](https://docs.rs/backbone-cache/badge.svg)](https://docs.rs/backbone-cache/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A production-ready, high-performance caching library for the Backbone Framework. Provides unified caching interface with Redis and in-memory backends, featuring async/await support, TTL management, and comprehensive statistics.

## 🚀 Features

- **🔗 Multiple Backends**: Redis for distributed caching, Memory for local caching
- **⚡ Async/Await**: Full async support built on Tokio
- **⏰ TTL Support**: Time-to-live expiration for cache entries
- **🔧 Generic Types**: Cache any serializable Rust data structures
- **📊 Statistics**: Comprehensive cache statistics and monitoring
- **🔒 Thread-Safe**: Safe concurrent access with proper locking
- **🎯 Batch Operations**: Support for mget, mset, mdelete
- **🔑 Key Prefixing**: Built-in multi-tenant support
- **🏗️ Production-Ready**: Robust error handling and connection management

## 📋 Quick Start

### Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
backbone-cache = "2.0"
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
```

### Basic Usage

```rust
use backbone_cache::{RedisCache, MemoryCache, CacheKey};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
struct User {
    id: String,
    name: String,
    email: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Redis Cache
    let redis_cache = RedisCache::new("redis://localhost:6379").await?;

    // Memory Cache
    let memory_cache = MemoryCache::new(Some(10000)); // 10k entries max

    let user = User {
        id: "123".to_string(),
        name: "John Doe".to_string(),
        email: "john@example.com".to_string(),
    };

    // Store user in cache (1 hour TTL)
    redis_cache.set("user:123", &user, Some(3600)).await?;

    // Retrieve user from cache
    let cached_user: Option<User> = redis_cache.get("user:123").await?;

    // Check if key exists
    let exists = redis_cache.exists("user:123").await?;

    // Delete user from cache
    let deleted = redis_cache.delete("user:123").await?;

    Ok(())
}
```

## 🏗️ Overview

Backbone Cache provides a unified caching interface that abstracts away the complexity of different caching backends while maintaining high performance and reliability.

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Application Code                        │
└─────────────────────┬───────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────┐
│                 Cache Trait (Unified Interface)              │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │ set() | get() | delete() | exists() | ttl() | clear() │   │ │
│  │ mget() | mset() | mdelete() | stats() │                   │ │
│  └─────────────────────────────────────────────────────────┘ │
└─────────────────────┬───────────────────────────────────────┘
                      │
        ┌─────────────┼─────────────┐
        │             │             │
┌───────▼───────┐ ┌───▼───────┐ ┌───▼───────┐
│   Redis      │ │   Memory   │ │   Future   │
│   Backend    │ │   Backend  │ │   Backend  │
│              │ │           │ │            │
└──────────────┘ └───────────┘ └───────────┘
```

### Key Components

- **Cache Trait**: Generic interface defining caching operations
- **RedisCache**: Production-ready distributed caching backend
- **MemoryCache**: High-performance in-memory caching backend
- **CacheEntry**: Metadata container for cache entries
- **CacheStats**: Comprehensive statistics and monitoring
- **CacheConfig**: Flexible configuration options

## 📚 Usage Examples

### Redis Cache

```rust
use backbone_cache::{RedisCache, CacheKey};

// Basic Redis connection
let cache = RedisCache::new("redis://localhost:6379").await?;

// With configuration
let cache = RedisCache::with_config(
    "redis://prod-cache.cluster.com:6379",
    Some("myapp:v1".to_string()) // Key prefix
).await?;

// Store data with TTL
cache.set("user:123", &user_data, Some(3600)).await?;

// Batch operations
let keys = vec!["user:123".to_string(), "user:456".to_string()];
let results = cache.mget::<User>(keys).await?;

let entries = vec![
    ("user:123".to_string(), user1, Some(3600)),
    ("user:456".to_string(), user2, Some(7200)),
];
cache.mset(entries).await?;
```

### Memory Cache

```rust
use backbone_cache::MemoryCache;

// With entry limit
let cache = MemoryCache::new(Some(50000)); // 50k entries max

// Unbounded memory cache
let cache = MemoryCache::new(None);

// Usage is identical to Redis cache
cache.set("config:app", &config, None).await?; // No expiration
let cached_config: Option<Config> = cache.get("config:app").await?;
```

### Key Management

```rust
use backbone_cache::CacheKey;

// Namespace-based keys
let user_key = CacheKey::user("123");           // "user:123"
let session_key = CacheKey::session("abc123");    // "session:abc123"

// Custom namespace
let api_key = CacheKey::build("api", "users:list");

// API response caching
let response_key = CacheKey::api_response("/users", "page=1&limit=10");
```

### Statistics and Monitoring

```rust
// Get cache statistics
let stats = cache.stats().await?;
println!("Hit rate: {:.2}%", stats.hit_rate * 100.0);
println!("Total entries: {}", stats.total_entries);
println!("Memory usage: {:?}", stats.memory_usage);

// Statistics include:
// - Hit rate percentage
// - Total entries count
// - Hits/misses counters
// - Sets/deletes counters
// - Memory usage (Redis)
```

## 🔧 Technical Details

### Async/Await Support

All cache operations are fully async and built on Tokio:

```rust
#[async_trait]
pub trait Cache: Send + Sync {
    async fn set<T>(&self, key: &str, value: &T, ttl_seconds: Option<u64>) -> CacheResult<()>
    where
        T: Serialize + Send + Sync;

    async fn get<T>(&self, key: &str) -> CacheResult<Option<T>>
    where
        T: for<'de> Deserialize<'de> + Send + Sync;
}
```

### Thread Safety

- **RedisCache**: Uses Redis connection manager with built-in thread safety
- **MemoryCache**: Uses `Arc<RwLock<T>>` for safe concurrent access

```rust
pub struct MemoryCache {
    entries: Arc<RwLock<HashMap<String, CacheEntry<Vec<u8>>>>,
    stats: Arc<RwLock<CacheStats>>,
    max_entries: Option<usize>,
}
```

### Error Handling

Comprehensive error types with proper error propagation:

```rust
#[derive(thiserror::Error, Debug)]
pub enum CacheError {
    #[error("Redis connection error: {0}")]
    RedisConnection(String),

    #[error("Redis operation error: {0}")]
    RedisOperation(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("Cache key not found: {0}")]
    NotFound(String),

    #[error("Cache error: {0}")]
    Other(String),
}
```

### TTL Management

Flexible time-to-live support:

```rust
// Set with 1 hour expiration
cache.set("key", &value, Some(3600)).await?;

// Set without expiration
cache.set("key", &value, None).await?;

// Check TTL
let ttl: Option<u64> = cache.ttl("key").await?;
match ttl {
    Some(seconds) => println!("Expires in {} seconds", seconds),
    None => println!("No expiration"),
}

// Set/extend TTL
let extended = cache.expire("key", 7200).await?; // 2 hours
```

### Memory Management

Memory cache includes intelligent eviction strategies:

```rust
// LRU (Least Recently Used) eviction
let cache = MemoryCache::new(Some(1000)); // 1000 entries max

// Automatic cleanup of expired entries
// Background cleanup runs on each operation

// Memory-efficient storage with Vec<u8> for serialized data
struct CacheEntry<T> {
    data: T,
    created_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    access_count: u64,
    last_accessed: Option<DateTime<Utc>>,
}
```

### Performance Characteristics

| Operation | Redis Cache | Memory Cache |
|-----------|-------------|-------------|
| **Set** | ~1ms (network) | ~10μs (local) |
| **Get** | ~1ms (network) | ~5μs (local) |
| **mset** | O(n) network | O(n) local |
| **mget** | O(n) network | O(n) local |
| **Memory** | Redis cluster | Process memory |
| **Persistence** | Durable | Ephemeral |
| **Scalability** | Horizontal | Single node |

### Configuration Options

```rust
pub struct CacheConfig {
    pub default_ttl: u64,           // Default: 3600 (1 hour)
    pub max_memory_entries: Option<usize>, // Default: Some(10000)
    pub redis_pool_size: Option<u32>,      // Default: Some(10)
    pub key_prefix: Option<String>,        // Default: None
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            default_ttl: DEFAULT_TTL,
            max_memory_entries: Some(10000),
            redis_pool_size: Some(10),
            key_prefix: None,
        }
    }
}
```

## 🏃‍♂️ Performance Benchmarks

### Memory Cache Benchmarks

| Operation | Latency | Throughput |
|-----------|----------|------------|
| **Set** | ~5-10μs | ~100k ops/sec |
| **Get** | ~3-8μs | ~125k ops/sec |
| **Delete** | ~4-6μs | ~160k ops/sec |
| **Cleanup** | ~50-100μs | Background |

### Redis Cache Benchmarks

| Operation | Latency | Throughput |
|-----------|----------|------------|
| **Set** | ~0.8-2ms | ~500 ops/sec |
| **Get** | ~0.6-1.5ms | ~650 ops/sec |
| **Mset (10)** | ~3-8ms | ~1250 ops/sec |
| **Mget (10)** | ~2-6ms | ~1500 ops/sec |

*Note: Benchmarks performed on local development environment with Redis 6.2*

## 🔒 Security Considerations

### Key Prefixing

Use key prefixing to avoid conflicts in shared Redis instances:

```rust
let cache = RedisCache::with_config(
    "redis://shared-cache.company.com:6379",
    Some("myapp:prod:v1".to_string())
).await?;

// Keys will be prefixed: "myapp:prod:v1:user:123"
```

### Connection Security

```rust
// With authentication
let cache = RedisCache::new("redis://username:password@localhost:6379").await?;

// With SSL/TLS
let cache = RedisCache::new("rediss://localhost:6380").await?;
```

### Data Serialization

All data is serialized using serde_json:

```rust
#[derive(Serialize, Deserialize)]
struct SensitiveData {
    token: String,
    user_id: Uuid,
    expires: DateTime<Utc>,
}

// Data is automatically serialized/deserialized
cache.set("session:123", &sensitive_data, Some(3600)).await?;
```

## 📊 Monitoring and Observability

### Statistics

Comprehensive statistics for monitoring:

```rust
let stats = cache.stats().await?;

// Key metrics
println!("Hit Rate: {:.2}%", stats.hit_rate * 100.0);
println!("Hit/Miss Ratio: {}/{}", stats.hits, stats.misses);
println!("Total Entries: {}", stats.total_entries);
println!("Memory Usage: {:?}", stats.memory_usage);

// Operation counters
println!("Operations: {} sets, {} deletes", stats.sets, stats.deletes);
```

### Logging

Integration with tracing for observability:

```rust
use tracing::{info, warn, error};

impl Cache for MyCache {
    async fn set<T>(&self, key: &str, value: &T, ttl_seconds: Option<u64>) -> CacheResult<()> {
        info!("Setting cache key: {}", key);
        if let Some(ttl) = ttl_seconds {
            info!("TTL: {} seconds", ttl);
        }
        // ... implementation
    }

    async fn get<T>(&self, key: &str) -> CacheResult<Option<T>> {
        debug!("Getting cache key: {}", key);
        // ... implementation
    }
}
```

### Health Checks

Redis connection health monitoring:

```rust
impl RedisCache {
    pub async fn health_check(&self) -> CacheResult<bool> {
        let mut conn = self.connection.clone();
        let result: redis::RedisResult<String> = redis::cmd("PING")
            .query_async(&mut conn)
            .await;

        match result {
            Ok(response) => Ok(response == "PONG"),
            Err(e) => Err(CacheError::RedisOperation(e.to_string())),
        }
    }
}
```

## 🧪 Testing

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test;

    #[tokio::test]
    async fn test_memory_cache_basic_operations() {
        let cache = MemoryCache::new(None);
        let test_value = "test_value";

        // Test set and get
        cache.set("test_key", test_value, None).await.unwrap();
        let retrieved: Option<String> = cache.get("test_key").await.unwrap();
        assert_eq!(retrieved, Some(test_value.to_string()));

        // Test exists
        assert!(cache.exists("test_key").await.unwrap());

        // Test delete
        assert!(cache.delete("test_key").await.unwrap());
        assert!(!cache.exists("test_key").await.unwrap());
    }
}
```

### Integration Tests

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_redis_cache_ttl() {
        let cache = RedisCache::new("redis://localhost:6379").await.unwrap();
        let test_value = "expires_soon";

        // Set with 1 second TTL
        cache.set("ttl_test", test_value, Some(1)).await.unwrap();

        // Should exist immediately
        assert!(cache.exists("ttl_test").await.unwrap());

        // Wait for expiration
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Should be expired
        assert!(!cache.exists("ttl_test").await.unwrap());
    }
}
```

## 🚀 Production Deployment

### Environment Variables

```bash
# Redis Configuration
REDIS_URL=redis://redis-cluster.company.com:6379
REDIS_PASSWORD=your_password_here
REDIS_DB=0

# Cache Configuration
CACHE_DEFAULT_TTL=3600
CACHE_MAX_MEMORY_ENTRIES=50000
CACHE_KEY_PREFIX=myapp:prod
```

### Docker Configuration

```dockerfile
FROM rust:1.70 as builder

WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bullseye-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/cache-service /usr/local/bin/cache-service
EXPOSE 8080

CMD ["cache-service"]
```

### Kubernetes Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: cache-service
spec:
  replicas: 3
  selector:
    matchLabels:
      app: cache-service
  template:
    metadata:
      labels:
        app: cache-service
    spec:
      containers:
      - name: cache-service
        image: your-registry/cache-service:latest
        ports:
        - containerPort: 8080
        env:
        - name: REDIS_URL
          value: "redis://redis-cluster:6379"
        - name: CACHE_DEFAULT_TTL
          value: "3600"
        resources:
          requests:
            memory: "256Mi"
            cpu: "250m"
          limits:
            memory: "512Mi"
            cpu: "500m"
```

## 🔧 Advanced Usage

### Custom Cache Implementation

```rust
use async_trait::async_trait;
use backbone_cache::{Cache, CacheResult, CacheStats};

struct CustomCache {
    // Your custom storage
}

#[async_trait]
impl Cache for CustomCache {
    async fn set<T>(&self, key: &str, value: &T, ttl_seconds: Option<u64>) -> CacheResult<()>
    where
        T: Serialize + Send + Sync,
    {
        // Your implementation
        todo!()
    }

    async fn get<T>(&self, key: &str) -> CacheResult<Option<T>>
    where
        T: for<'de> Deserialize<'de> + Send + Sync,
    {
        // Your implementation
        todo!()
    }

    // ... implement all other required methods
}
```

### Cache Decorator Pattern

```rust
struct CacheDecorator<T: Cache> {
    inner: T,
    metrics: MetricsCollector,
}

#[async_trait]
impl<T: Cache> Cache for CacheDecorator<T> {
    async fn set<V>(&self, key: &str, value: &V, ttl_seconds: Option<u64>) -> CacheResult<()>
    where
        V: Serialize + Send + Sync,
    {
        let start = std::time::Instant::now();
        let result = self.inner.set(key, value, ttl_seconds).await;

        // Record metrics
        self.metrics.record_operation("set", start.elapsed());
        result
    }

    // ... delegate other methods to inner cache
}
```

### Cache Warming

```rust
async fn warm_cache(cache: &dyn Cache) -> CacheResult<()> {
    let common_data = vec![
        ("config:app", get_app_config()?),
        ("config:db", get_db_config()?),
        ("constants:countries", get_countries()?),
    ];

    for (key, value) in common_data {
        cache.set(key, &value, Some(3600)).await?;
    }

    Ok(())
}
```

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

### Development Setup

```bash
# Clone repository
git clone https://github.com/your-org/backbone-cache.git
cd backbone-cache

# Install dependencies
cargo build

# Run tests
cargo test

# Run benchmarks
cargo bench

# Check formatting
cargo fmt --check

# Run clippy
cargo clippy -- -D warnings
```

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🔗 Related Projects

- [Backbone Framework](https://github.com/your-org/backbone) - The main framework
- [Backbone Auth](https://github.com/your-org/backbone-auth) - Authentication module
- [Backbone Config](https://github.com/your-org/backbone-config) - Configuration management

## 📞 Support

- 📖 [Documentation](https://docs.rs/backbone-cache/)
- 🐛 [Issue Tracker](https://github.com/your-org/backbone-cache/issues)
- 💬 [Discussions](https://github.com/your-org/backbone-cache/discussions)

---

**Made with ❤️ for the Backbone Framework**