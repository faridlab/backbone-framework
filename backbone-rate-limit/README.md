# backbone-rate-limit

Rate limiting middleware for Axum web framework with configurable backends.

## Features

- **Multiple Storage Backends**: In-memory for development/testing, Redis for production
- **Configuration**: YAML-based configuration with per-route overrides
- **Axum Middleware**: Easy integration with `State` extractor
- **Security-Focused**: IP-based and user-based rate limiting
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
    max_requests: 100      # Max requests per window
    window_seconds: 60        # Time window in seconds
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
    let config = RateLimitConfig {
        key: "default".to_string(),
        max_requests: 100,
        window_seconds: 60,
        enabled: true,
    };

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

## License

MIT OR Apache-2.0
