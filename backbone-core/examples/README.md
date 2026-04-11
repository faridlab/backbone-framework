# Backbone Core Examples

This directory contains comprehensive examples demonstrating various features and use cases of Backbone Core.

## 📋 Available Examples

### 1. **Basic Usage** (`basic_usage.rs`)
**Perfect for beginners** - Shows the simplest way to implement Backbone CRUD operations.

**What you'll learn:**
- Creating entities with the `Entity` trait
- Implementing HTTP and gRPC handlers
- Basic CRUD operations
- Soft delete and restore functionality
- Bulk operations

**Run with:**
```bash
cargo run --example basic_usage
```

### 2. **Advanced Pagination** (`advanced_pagination.rs`)
**Complex querying example** - Demonstrates advanced filtering, sorting, and pagination.

**What you'll learn:**
- Multi-field filtering (price range, category, stock, rating)
- Flexible sorting options
- Pagination navigation
- Search functionality
- Complex filter combinations
- Performance optimization for large datasets

**Run with:**
```bash
cargo run --example advanced_pagination
```

### 3. **E-commerce Scenario** (`scenario_ecommerce.rs`)
**Real-world application** - Complete e-commerce system with multiple entities.

**What you'll learn:**
- Multiple related entities (Users, Products, Orders, Reviews)
- Business logic integration
- Data validation and relationships
- Transaction-like operations
- Bulk operations and upserts
- Complex workflows

**Features demonstrated:**
- User management with roles
- Product catalog with filtering
- Order processing with validation
- Review system with business rules
- Cross-entity data integrity

**Run with:**
```bash
cargo run --example scenario_ecommerce
```

### 4. **Error Handling** (`error_handling.rs`)
**Comprehensive error handling** - Best practices for robust applications.

**What you'll learn:**
- Custom error types with `thiserror`
- Error chaining with `anyhow::context`
- Input validation strategies
- Business rule enforcement
- Database error simulation
- Rate limiting and permission errors
- User-friendly error messages

**Error patterns covered:**
- Validation errors
- Business logic errors
- Database errors
- Permission errors
- Rate limiting errors

**Run with:**
```bash
cargo run --example error_handling
```

## 🚀 Quick Start

### Run All Examples
```bash
# Run each example individually
cargo run --example basic_usage
cargo run --example advanced_pagination
cargo run --example scenario_ecommerce
cargo run --example error_handling

# Or run in parallel (if supported)
cargo run --example basic_usage &
cargo run --example advanced_pagination &
cargo run --example scenario_ecommerce &
cargo run --example error_handling &
```

### Build Examples Only
```bash
# Build all examples without running
cargo build --examples

# Build specific example
cargo build --example basic_usage
```

## 📖 Learning Path

### For Beginners
1. Start with **`basic_usage.rs`** - Understand the fundamentals
2. Move to **`error_handling.rs`** - Learn robust error handling
3. Try **`advanced_pagination.rs`** - Master complex querying

### For Intermediate Developers
1. **`scenario_ecommerce.rs`** - Real-world application patterns
2. **`error_handling.rs`** - Production-ready error handling
3. **`advanced_pagination.rs`** - Performance optimization

### For Advanced Developers
1. **`scenario_ecommerce.rs`** - Complex business logic
2. Extend examples with your own requirements
3. Combine patterns from multiple examples

## 🛠️ Common Patterns

### Entity Pattern
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MyEntity {
    id: Uuid,
    // ... your fields
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Entity for MyEntity {
    fn id(&self) -> &Uuid { &self.id }
    fn created_at(&self) -> chrono::DateTime<chrono::Utc> { self.created_at }
    fn updated_at(&self) -> chrono::DateTime<chrono::Utc> { self.updated_at }
    fn deleted_at(&self) -> Option<chrono::DateTime<chrono::Utc>> { self.deleted_at }
}
```

### Service Pattern
```rust
struct MyService {
    // Database connections, other dependencies
}

impl BackboneHttpHandler<MyEntity> for MyService {
    fn list(&self, request: ListRequest) -> Result<ApiResponse<Vec<MyEntity>>> {
        // Your implementation
    }

    // ... implement all 11 methods
}
```

### Error Handling Pattern
```rust
use thiserror::Error;
use anyhow::{Result, Context};

#[derive(Debug, Error)]
enum MyError {
    #[error("Entity not found: {id}")]
    NotFound { id: Uuid },

    #[error("Validation failed: {field} - {message}")]
    Validation { field: String, message: String },
}

fn validate_entity(entity: &MyEntity) -> Result<()> {
    if entity.name.is_empty() {
        bail!(MyError::Validation {
            field: "name".to_string(),
            message: "Name cannot be empty".to_string(),
        });
    }
    Ok(())
}
```

## 🔧 Customization Examples

### Adding Custom Fields
```rust
// Extend ListRequest for custom filtering
#[derive(Debug, Serialize, Deserialize)]
struct CustomListRequest {
    #[serde(flatten)]
    base: ListRequest,

    custom_field: Option<String>,
    date_range: Option<DateRange>,
}

impl Default for CustomListRequest {
    fn default() -> Self {
        Self {
            base: ListRequest::default(),
            custom_field: None,
            date_range: None,
        }
    }
}
```

### Async Database Operations
```rust
use async_trait::async_trait;

#[async_trait]
trait AsyncRepository<T> {
    async fn find_by_id(&self, id: &Uuid) -> Result<Option<T>>;
    async fn save(&self, entity: &T) -> Result<T>;
    // ... other async methods
}
```

### Middleware Integration
```rust
use axum::middleware::Next;
use axum::response::Response;

async fn auth_middleware<T>(
    req: Request<T>,
    next: Next<T>,
) -> Result<Response, StatusCode> {
    // Your auth logic here
    next.run(req).await
}
```

## 📊 Performance Tips

### Pagination
- Use reasonable page sizes (10-50 items)
- Implement cursor-based pagination for large datasets
- Cache frequently accessed pages

### Filtering
- Apply filters before sorting when possible
- Use database indexes for filtered fields
- Combine multiple filters efficiently

### Bulk Operations
- Use transactions for consistency
- Batch database operations
- Handle partial failures gracefully

## 🔍 Testing Examples

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_entity() {
        let service = MyService::new();
        let entity = MyEntity::new(/* ... */);

        let result = service.create(entity);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validation_errors() {
        let service = MyService::new();
        let invalid_entity = MyEntity::new_with_empty_name();

        let result = service.create(invalid_entity);
        assert!(result.is_err());
    }
}
```

## 🚨 Production Considerations

### Database Transactions
```rust
async fn create_order_with_items(
    order: Order,
    items: Vec<OrderItem>,
) -> Result<Order> {
    // Begin transaction
    let mut tx = database.begin_transaction().await?;

    // Create order
    let order = create_order_in_tx(&mut tx, order).await?;

    // Create items
    for item in items {
        create_order_item_in_tx(&mut tx, item, order.id).await?;
    }

    // Commit transaction
    tx.commit().await?;

    Ok(order)
}
```

### Rate Limiting
```rust
use std::collections::HashMap;
use std::time::{Duration, Instant};

struct RateLimiter {
    requests: HashMap<Uuid, Vec<Instant>>,
    max_requests: usize,
    window: Duration,
}

impl RateLimiter {
    fn is_allowed(&mut self, user_id: &Uuid) -> bool {
        let now = Instant::now();
        let requests = self.requests.entry(*user_id).or_insert_with(Vec::new);

        // Remove old requests
        requests.retain(|&time| now.duration_since(time) < self.window);

        if requests.len() < self.max_requests {
            requests.push(now);
            true
        } else {
            false
        }
    }
}
```

## 📚 Additional Resources

- **[Backbone Core README](../README.md)** - Complete API documentation
- **[Rust Error Handling Guide](https://doc.rust-lang.org/rust-by-example/error.html)** - Rust error handling patterns
- **[Serde Documentation](https://serde.rs/)** - Serialization/deserialization
- **[Chrono Documentation](https://docs.rs/chrono/)** - Date/time handling

## 🤝 Contributing

### Adding New Examples

1. Create a new file: `examples/your_example.rs`
2. Follow the existing example structure
3. Include comprehensive comments
4. Add to this README with:
   - Brief description
   - Learning objectives
   - Usage instructions
5. Test with `cargo test --example your_example`

### Example Template

```rust
//! Your Example Title
//!
//! Brief description of what this example demonstrates.

use backbone_core::*;
use anyhow::Result;

fn main() -> Result<()> {
    println!("🦴 Backbone Core - Your Example");
    println!("=================================");

    // Your example code here

    println!("\n🎉 Your example completed!");
    Ok(())
}
```

---

**💡 Tip:** Start with `basic_usage.rs` and gradually explore more complex examples as you become comfortable with Backbone Core patterns!