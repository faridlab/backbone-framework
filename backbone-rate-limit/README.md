# backbone-rate-limit

Rate limiting middleware for Axum web framework with configurable backends.

## Features

- **Multiple Storage Backends**: In-memory for development/testing, Redis for production
- **Configuration**: YAML-based configuration with per-route overrides
- **Axum Middleware**: Easy integration with `State` extractor
- **Security-Focused**: IP-based and user-based rate limiting
- **Hard Lockout**: Optional `lockout_seconds` rejects all further requests
  for a fixed duration once the window limit is exceeded
- **Type-Safe**: Full Rust type safety with comprehensive error handling

## Installation

Add this to your `Cargo.toml` dependencies:

```toml
[dependencies]
backbone-rate-limit = { path = "../../crates/backbone-rate-limit" }
```

## Configuration

Configure rate limiting in your `application.yml`:

```yaml
rate_limiting:
  enabled: true
  key: "x-rate-limit"  # Header-based or IP-based
  config:
    max_requests: 100         # Max requests per window
    window_seconds: 60        # Time window in seconds
    lockout_seconds: 300      # Optional. When set, exceeding max_requests
                              # locks the key for this many seconds and
                              # rejects all further requests until expiry.
```

## Usage

### Basic Example

```rust
use axum::{
    extract::{Request, State, TypedHeader},
    http::{StatusCode, Response},
    response::{Json, IntoResponseParts},
};
use backbone_rate_limit::{
    RateLimitLayer, RateLimitConfig,
    types::{RateLimitConfig, RateLimitError, RateLimitResult},
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Build rate limiter layer with configuration
    let config = RateLimitConfig::new("default", 100, 60, true);

    // Optional: add a hard lockout. After 100 requests in 60s, the key
    // is locked for 5 minutes — every subsequent request is rejected
    // until the lockout expires, regardless of window position.
    // let config = RateLimitConfig::new("default", 100, 60, true)
    //     .with_lockout(300);

    let rate_limit_layer = RateLimitLayer::new(
        InMemoryStorage::new(),
        config
    );

    // Build application with rate limiting
    let app = Router::new()
        .route("/api/health", get_health.layer().layer(rate_limit_layer.clone()))
        .route("/api/v1/*", services().layer(rate_limit_layer.clone()))
        .layer(rate_limit_layer);

    // Start server
    let listener = TcpListener::bind("0.0.0.0:3000).await?;
    tracing::info!("Server running on port 3000");
    listener.run(app).await?;

    Ok(())
}
```

### Advanced Example: User-Based Rate Limiting

```rust
use axum::{
    extract::{Request, State, TypedHeader},
    http::{StatusCode, Response},
    response::{Json, IntoResponseParts},
};
use backbone_rate_limit::{
    RateLimitLayer, RateLimitConfig,
    types::{RateLimitResponse},
};
use std::sync::Arc;

async fn handle_request(
    State: &mut State,
    rate_limit: Arc<RateLimitLayer>,
) -> Result<Response, impl IntoResponseParts> {
    // Extract user ID from state (if authenticated)
    let user_id = State::get_user_id(&state);

    // Check rate limit for this user
    let rate_limit_result = rate_limit
        .check_rate_limit(user_id.as_deref(), "user_api", &rate_limit.config())
        .await;

    match rate_limit_result {
            Ok(allowed) => {
                // Rate limit check passed - allow request
                next.run(req).await
            }
            Ok(exceeded) => {
                // Rate limit exceeded - return error
                let response = Json(exceeded.into_response());
                return response.set_status(StatusCode::TOO_MANY_REQUESTS),
            }
            Err(e) => {
                // Internal error - log and return 500 error
                tracing::error!("Rate limit check error: {:?}", e);
                let response = Json(e.into_response());
                return response.set_status(StatusCode::INTERNAL_SERVER_ERROR),
            }
        }
    }
}
```

### Redis Backend Example

```yaml
# application.yml
rate_limiting:
  enabled: true
  key: "x-rate-limit"
  config:
    max_requests: 1000
    window_seconds: 60
    backend: "redis"  # Use Redis backend
    redis:
      url: "redis://127.0.0.1:6379"
      key_prefix: "rate_limit:"
```

## Hard Lockout

By default the limiter uses pure window semantics: once `window_seconds`
elapses, the counter resets and the caller may try again. For abuse
mitigation you can layer a **hard lockout** on top:

```rust
use backbone_rate_limit::RateLimitConfig;

let config = RateLimitConfig::new("login", 5, 60, true)
    .with_lockout(900); // 15-minute lockout after 5 failed attempts/min
```

Behavior:

- While inside the window, the limiter behaves as before.
- The first request that exceeds `max_requests` flips the key into
  lockout. The response carries `locked_until` (unix timestamp) and
  `reset_at` is set to that same value.
- Every subsequent request during the lockout is rejected with
  `allowed = false` and the same `locked_until`. The counter is **not**
  incremented while locked.
- When `locked_until` passes, the next call starts a fresh window.

Redis backend stores the lock as a sibling key
(`{prefix}:lock:{key}`) with a TTL equal to `lockout_seconds`, so
distributed nodes observe the same lockout window.

## License

MIT OR Apache-2.0
