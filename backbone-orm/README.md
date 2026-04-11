# Backbone-ORM

**Status:** ✅ FULLY IMPLEMENTED
**Last Updated:** 2026-02-11

A powerful, type-safe PostgreSQL ORM for Rust applications, designed for Backbone Framework.

## 🚀 Features

- **Type-Safe Query Building** - Build queries with compile-time safety
- **SQL Injection Protection** - All queries use parameterized binding
- **Generic Repository Pattern** - Reusable CRUD operations
- **Advanced Querying** - JOINs, CTEs, window functions, aggregations
- **Database Migrations** - Version-controlled schema management
- **Database Seeding** - Structured data seeding for development and testing
- **Raw SQL Support** - When you need full power of SQL
- **Async/Await** - Built on Tokio for high performance
- **Comprehensive Testing** - Full test coverage with integration tests

## 📦 Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
backbone-orm = { version = "0.1", features = ["default"] }
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "postgres", "uuid", "chrono", "json", "migrate"] }
tokio = { version = "1.0", features = ["full"] }
anyhow = "1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
uuid = { version = "1.0", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
```

## 🎯 Quick Start

```rust
use backbone_orm::*;
use sqlx::{PgPool, FromRow};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
struct User {
    id: String,
    name: String,
    email: String,
    age: i32,
    created_at: chrono::NaiveDateTime,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Connect to database
    let database_url = "postgresql://root:password@localhost/myapp";
    let pool = PgPool::connect(database_url).await?;

    // Create repository
    let user_repo = PostgresRepository::<User>::new(pool, "users");

    // Create a user
    let new_user = User {
        id: uuid::Uuid::new_v4().to_string(),
        name: "John Doe".to_string(),
        email: "john@example.com".to_string(),
        age: 30,
        created_at: chrono::Utc::now().naive_utc(),
    };

    let created_user = user_repo.create(&new_user).await?;
    println!("Created user: {:?}", created_user);

    // Find user
    let found_user = user_repo.find_by_id(&created_user.id).await?;
    println!("Found user: {:?}", found_user);

    Ok(())
}
```

## 📖 Overview

Backbone-ORM provides a complete database solution for Rust applications with these key principles:

### Security First
- **Parameterized Queries**: All queries use parameter binding to prevent SQL injection
- **Type Safety**: Compile-time checks prevent runtime errors
- **Validation**: Built-in validation for all data operations

### Developer Experience
- **Intuitive API**: Clean, fluent interface for building queries
- **Comprehensive Documentation**: Examples and patterns for common use cases
- **Error Handling**: Rich error types with context for debugging

### Performance
- **Async/Await**: Non-blocking database operations
- **Connection Pooling**: Efficient connection management
- **Query Optimization**: Smart query building with minimal overhead

### Flexibility
- **Raw SQL Support**: Drop to raw SQL when needed
- **Generic Patterns**: Reusable repository implementations

## 🔧 Usage

### Basic CRUD Operations

```rust
use backbone_orm::*;

// Create
let user = User { /* fields */ };
let created = user_repo.create(&user).await?;

// Read by ID
let user = user_repo.find_by_id("user-id").await?;

// Read all
let users = user_repo.find_all().await?;

// Update
user.name = "Updated Name";
let updated = user_repo.update(&user).await?;

// Delete
let deleted = user_repo.delete("user-id").await?;

// Count
let count = user_repo.count().await?;
```

### Query Builder

```rust
use backbone_orm::*;

// Basic filtering
let users = QueryBuilder::new("users")
    .where_gt("age", QueryValue::integer(25))
    .where_like("email", QueryValue::text("%@company.com"))
    .order_by("name", "ASC")
    .limit(10)
    .build()
    .fetch_all::<User>(&pool)
    .await?;

// Complex conditions
let results = QueryBuilder::new("products")
    .where_gte("price", QueryValue::float(100.0))
    .where_lt("stock", QueryValue::integer(10))
    .order_by("price", "DESC")
    .limit(50)
    .build()
    .fetch_all::<Product>(&pool)
    .await?;
```

### Advanced Querying

```rust
use backbone_orm::*;

// JOIN operations
let users_with_profiles: Vec<UserWithProfile> = AdvancedQueryBuilder::new("users")
    .select(&["users.id", "users.name", "profiles.bio", "profiles.avatar_url"])
    .join(JoinType::Left, "profiles", "users.id = profiles.user_id")
    .where_gt("users.age", QueryValue::integer(18))
    .order_by("users.name", "ASC")
    .build()
    .execute(&pool)
    .await?;

// Common Table Expressions (CTEs)
let analytics: Vec<UserAnalytics> = AdvancedQueryBuilder::new("users")
    .with_cte(
        "user_activity",
        "SELECT user_id, COUNT(*) as login_count FROM user_sessions GROUP BY user_id"
    )
    .select(&["users.name", "user_activity.login_count"])
    .join(JoinType::Inner, "user_activity", "users.id = user_activity.user_id")
    .where_gt("user_activity.login_count", QueryValue::integer(10))
    .order_by("user_activity.login_count", "DESC")
    .build()
    .execute(&pool)
    .await?;

// Window Functions
let ranked_users: Vec<UserRank> = AdvancedQueryBuilder::new("users")
    .select(&[
        "name",
        "age",
        "department",
        "ROW_NUMBER() OVER (PARTITION BY department ORDER BY age DESC) as age_rank"
    ])
    .order_by("age_rank", "ASC")
    .build()
    .execute(&pool)
    .await?;
```

### Raw SQL Queries

```rust
use backbone_orm::*;

// Parameterized raw query
let users: Vec<User> = RawQueryBuilder::new(
    "SELECT * FROM users WHERE age > $1 AND email LIKE $2 ORDER BY created_at DESC"
)
.bind(QueryValue::integer(25))
.bind(QueryValue::text("%@gmail.com"))
.limit(10)
.execute(&pool)
.await?;

// Scalar queries
let average_age: f64 = RawQuery::scalar(
    &pool,
    "SELECT AVG(age) FROM users WHERE active = true",
    vec![]
).await?;

let departments: Vec<String> = RawQuery::many(
    &pool,
    "SELECT DISTINCT department FROM users ORDER BY department",
    vec![]
).await?;
```

## 🛠️ Security Features

1. **SQL Injection Prevention**
   - All queries use parameter binding
   - No string concatenation for values
   - Type-safe parameter handling

2. **Connection Security**
   - SSL/TLS support
   - Connection timeout configuration
   - Secure credential management

3. **Data Validation**
   - Type-safe field mapping
   - Runtime parameter validation
   - SQL error handling

## 📚 Testing

### Comprehensive testing approach:

1. **Unit Tests**: Test individual components in isolation
2. **Integration Tests**: Test database operations end-to-end
3. **Mock Tests**: Test business logic without database
4. **Performance Tests**: Validate query performance

### Running tests

```bash
# Run all tests
cargo test

# Run with database (for integration tests)
DATABASE_URL="postgresql://root:password@localhost/test" cargo test --test integration_tests
```

## 🔗 Dependencies

```toml
[dependencies]
# Core dependencies
tokio = { version = "1.0", features = ["full"] }
async-trait = "0.1"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
uuid = { version = "1.0", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1.0"
thiserror = "1.0"

# Database driver
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "postgres", "uuid", "chrono", "json", "migrate"] }
```

## 🤝 Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

**Built with ❤️ for Backbone Framework**
