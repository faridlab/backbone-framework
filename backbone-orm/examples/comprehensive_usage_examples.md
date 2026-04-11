# Backbone-ORM Examples

This guide demonstrates comprehensive usage of backbone-orm from basic CRUD operations to advanced real-world scenarios.

## Table of Contents

1. [Basic Setup](#basic-setup)
2. [Basic CRUD Operations](#basic-crud-operations)
3. [Query Builder Usage](#query-builder-usage)
4. [Advanced Querying](#advanced-querying)
5. [Raw SQL Queries](#raw-sql-queries)
6. [Database Migrations](#database-migrations)
7. [Database Seeding](#database-seeding)
8. [Real-World Scenarios](#real-world-scenarios)
   - E-commerce Application
   - User Analytics Dashboard
   - Inventory Management
   - Financial Reporting

## Basic Setup

First, set up your database connection and define your entity:

```rust
use backbone_orm::*;
use sqlx::FromRow;
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
    // Initialize database connection
    let database_url = "postgresql://root:password@localhost/backbone_examples";
    let pool = PgPool::connect(database_url).await?;

    // Create users table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id VARCHAR(255) PRIMARY KEY,
            name VARCHAR(255) NOT NULL,
            email VARCHAR(255) UNIQUE NOT NULL,
            age INTEGER NOT NULL,
            created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
        )
        "#
    )
    .execute(&pool)
    .await?;

    // Create repository
    let user_repo = PostgresRepository::<User>::new(pool, "users");

    // Use the repository...

    Ok(())
}
```

## Basic CRUD Operations

### Create User

```rust
async fn create_user_example(repo: &PostgresRepository<User>) -> anyhow::Result<()> {
    println!("=== Creating User Example ===");

    let new_user = User {
        id: uuid::Uuid::new_v4().to_string(),
        name: "John Doe".to_string(),
        email: "john.doe@example.com".to_string(),
        age: 30,
        created_at: chrono::Utc::now().naive_utc(),
    };

    let created_user = repo.create(&new_user).await?;
    println!("Created user: {:?}", created_user);

    Ok(())
}
```

### Find User by ID

```rust
async fn find_user_by_id_example(repo: &PostgresRepository<User>) -> anyhow::Result<()> {
    println!("\n=== Find User by ID Example ===");

    let user_id = "some-user-id";
    match repo.find_by_id(user_id).await? {
        Some(user) => println!("Found user: {:?}", user),
        None => println!("User with ID '{}' not found", user_id),
    }

    Ok(())
}
```

### Find All Users

```rust
async fn find_all_users_example(repo: &PostgresRepository<User>) -> anyhow::Result<()> {
    println!("\n=== Find All Users Example ===");

    let users = repo.find_all().await?;
    println!("Found {} users:", users.len());
    for user in users {
        println!("- {}: {} ({})", user.name, user.email, user.age);
    }

    Ok(())
}
```

### Update User

```rust
async fn update_user_example(repo: &PostgresRepository<User>) -> anyhow::Result<()> {
    println!("\n=== Update User Example ===");

    // First find the user
    let user_id = "some-user-id";
    if let Some(mut user) = repo.find_by_id(user_id).await? {
        user.age = 31;
        user.name = "John Updated".to_string();

        let updated_user = repo.update(&user).await?;
        println!("Updated user: {:?}", updated_user);
    } else {
        println!("User not found for update");
    }

    Ok(())
}
```

### Delete User

```rust
async fn delete_user_example(repo: &PostgresRepository<User>) -> anyhow::Result<()> {
    println!("\n=== Delete User Example ===");

    let user_id = "some-user-id";
    let deleted = repo.delete(user_id).await?;

    if deleted {
        println!("Successfully deleted user with ID: {}", user_id);
    } else {
        println!("Failed to delete user with ID: {}", user_id);
    }

    Ok(())
}
```

### Count Users

```rust
async fn count_users_example(repo: &PostgresRepository<User>) -> anyhow::Result<()> {
    println!("\n=== Count Users Example ===");

    let count = repo.count().await?;
    println!("Total users in database: {}", count);

    Ok(())
}
```

## Query Builder Usage

### Basic Filtering

```rust
async fn query_builder_filtering_example(repo: &PostgresRepository<User>) -> anyhow::Result<()> {
    println!("\n=== Query Builder Filtering Example ===");

    // Find users older than 25
    let users = QueryBuilder::new("users")
        .where_gt("age", QueryValue::integer(25))
        .order_by("name", "ASC")
        .limit(10)
        .build()
        .fetch_all::<User>(repo.pool())
        .await?;

    println!("Users older than 25:");
    for user in users {
        println!("- {} (age: {})", user.name, user.age);
    }

    Ok(())
}
```

### Multiple Conditions

```rust
async fn query_builder_multiple_conditions_example(repo: &PostgresRepository<User>) -> anyhow::Result<()> {
    println!("\n=== Multiple Conditions Example ===");

    // Find users aged 20-40 with specific email domain
    let users = QueryBuilder::new("users")
        .where_gte("age", QueryValue::integer(20))
        .where_lt("age", QueryValue::integer(40))
        .where_like("email", QueryValue::text("%@company.com"))
        .order_by("age", "ASC")
        .build()
        .fetch_all::<User>(repo.pool())
        .await?;

    println!("Company users aged 20-40:");
    for user in users {
        println!("- {} ({} years old)", user.name, user.age);
    }

    Ok(())
}
```

### Pagination

```rust
async fn query_builder_pagination_example(repo: &PostgresRepository<User>) -> anyhow::Result<()> {
    println!("\n=== Pagination Example ===");

    let page = 1;
    let per_page = 5;
    let offset = (page - 1) * per_page;

    let users = QueryBuilder::new("users")
        .order_by("created_at", "DESC")
        .limit(per_page as u32)
        .offset(offset as u32)
        .build()
        .fetch_all::<User>(repo.pool())
        .await?;

    println!("Page {} ({} per page):", page, per_page);
    for (i, user) in users.iter().enumerate() {
        println!("{}. {} - {}", i + 1, user.name, user.email);
    }

    Ok(())
}
```

### Complex Filtering with Dates

```rust
async fn query_builder_date_filtering_example(repo: &PostgresRepository<User>) -> anyhow::Result<()> {
    println!("\n=== Date Filtering Example ===");

    // Find users created in the last 7 days
    let seven_days_ago = chrono::Utc::now() - chrono::Duration::days(7);

    let users = QueryBuilder::new("users")
        .where_gte("created_at", QueryValue::Timestamp(seven_days_ago.naive_utc()))
        .order_by("created_at", "DESC")
        .build()
        .fetch_all::<User>(repo.pool())
        .await?;

    println!("Users created in the last 7 days:");
    for user in users {
        println!("- {} (created: {})", user.name, user.created_at);
    }

    Ok(())
}
```

## Advanced Querying

### JOIN Operations

```rust
use backbone_orm::*;
use sqlx::FromRow;

#[derive(Debug, FromRow)]
struct UserWithProfile {
    id: String,
    name: String,
    email: String,
    bio: Option<String>,
    avatar_url: Option<String>,
}

async fn join_operations_example(pool: &PgPool) -> anyhow::Result<()> {
    println!("\n=== JOIN Operations Example ===");

    // Complex query with LEFT JOIN
    let query = QueryBuilder::new("users")
        .select(&["users.id", "users.name", "users.email", "profiles.bio", "profiles.avatar_url"])
        .join(JoinType::Left, "profiles", "users.id = profiles.user_id")
        .where_gt("users.age", QueryValue::integer(18))
        .order_by("users.name", "ASC")
        .limit(20)
        .build();

    let results: Vec<UserWithProfile> = query.fetch_all(pool).await?;

    println!("Users with profiles:");
    for result in results {
        match result.bio {
            Some(bio) => println!("- {}: {}", result.name, bio),
            None => println!("- {} (no profile)", result.name),
        }
    }

    Ok(())
}
```

### Aggregation and GROUP BY

```rust
#[derive(Debug, FromRow)]
struct DepartmentStats {
    department: String,
    user_count: i64,
    avg_age: f64,
}

async fn aggregation_example(pool: &PgPool) -> anyhow::Result<()> {
    println!("\n=== Aggregation Example ===");

    // Query requiring raw SQL for aggregation
    let query = RawQueryBuilder::new(
        r#"
        SELECT
            department,
            COUNT(*) as user_count,
            AVG(age) as avg_age
        FROM users
        WHERE age >= $1
        GROUP BY department
        HAVING COUNT(*) > $2
        ORDER BY avg_age DESC
        "#
    )
    .bind(QueryValue::integer(18))  // $1
    .bind(QueryValue::integer(2))   // $2
    .build();

    let stats: Vec<DepartmentStats> = query.execute(pool).await?;

    println!("Department statistics:");
    for stat in stats {
        println!("- {}: {} users, avg age: {:.1} years",
                stat.department, stat.user_count, stat.avg_age);
    }

    Ok(())
}
```

### Window Functions

```rust
#[derive(Debug, FromRow)]
struct UserWithRank {
    name: String,
    age: i32,
    department: String,
    age_rank: i64,
    dept_avg_age: f64,
}

async fn window_functions_example(pool: &PgPool) -> anyhow::Result<()> {
    println!("\n=== Window Functions Example ===");

    let query = AdvancedQueryBuilder::new("users")
        .select(&[
            "name",
            "age",
            "department",
            "ROW_NUMBER() OVER (ORDER BY age DESC) as age_rank",
            "AVG(age) OVER (PARTITION BY department) as dept_avg_age"
        ])
        .where_gt("age", QueryValue::integer(20))
        .order_by("age_rank", "ASC")
        .limit(15)
        .build_sql();

    let results: Vec<UserWithRank> = RawQueryBuilder::new(&query.0)
        .bind_many(query.1)
        .execute(pool)
        .await?;

    println!("Users with age rankings and department averages:");
    for result in results {
        println!("{}. {} ({} years) - {} dept avg: {:.1} years",
                result.age_rank, result.name, result.age,
                result.department, result.dept_avg_age);
    }

    Ok(())
}
```

### Common Table Expressions (CTEs)

```rust
#[derive(Debug, FromRow)]
struct TopUserPerDepartment {
    department: String,
    top_user_name: String,
    top_user_age: i32,
    user_count: i64,
}

async fn cte_example(pool: &PgPool) -> anyhow::Result<()> {
    println!("\n=== Common Table Expression Example ===");

    let query = AdvancedQueryBuilder::new("users")
        .with_cte(
            "dept_stats",
            "SELECT
                department,
                COUNT(*) as user_count,
                MAX(age) as max_age
            FROM users
            GROUP BY department"
        )
        .with_cte(
            "ranked_users",
            "SELECT
                u.name,
                u.age,
                u.department,
                ROW_NUMBER() OVER (PARTITION BY u.department ORDER BY u.age DESC) as rank_in_dept
            FROM users u
            WHERE u.age >= 18"
        )
        .select(&[
            "ru.department",
            "ru.name as top_user_name",
            "ru.age as top_user_age",
            "ds.user_count"
        ])
        .join_alias(JoinType::Inner, "ranked_users", "ru", "true")
        .join_alias(JoinType::Inner, "dept_stats", "ds", "ru.department = ds.department")
        .where_raw("ru.rank_in_dept = $1", QueryValue::integer(1))
        .order_by("ds.user_count", "DESC")
        .build_sql();

    let results: Vec<TopUserPerDepartment> = RawQueryBuilder::new(&query.0)
        .bind_many(query.1)
        .execute(pool)
        .await?;

    println!("Top user per department:");
    for result in results {
        println!("{}: {} ({} years) - {} total users",
                result.department, result.top_user_name,
                result.top_user_age, result.user_count);
    }

    Ok(())
}
```

## Raw SQL Queries

### Parameterized Raw Query

```rust
async fn raw_query_example(pool: &PgPool) -> anyhow::Result<()> {
    println!("\n=== Raw SQL Query Example ===");

    // Safe parameterized query
    let users: Vec<User> = RawQueryBuilder::new(
        "SELECT * FROM users WHERE age > $1 AND email LIKE $2 ORDER BY created_at DESC LIMIT $3"
    )
    .bind(QueryValue::integer(25))
    .bind(QueryValue::text("%@gmail.com"))
    .bind(QueryValue::integer(10))
    .execute(pool)
    .await?;

    println!("Gmail users older than 25:");
    for user in users {
        println!("- {} ({})", user.name, user.email);
    }

    Ok(())
}
```

### Scalar Query (Single Value)

```rust
async fn scalar_query_example(pool: &PgPool) -> anyhow::Result<()> {
    println!("\n=== Scalar Query Example ===");

    // Get single value
    let average_age: f64 = RawQuery::scalar(
        pool,
        "SELECT AVG(age) FROM users WHERE created_at > $1",
        vec![QueryValue::Timestamp(
            (chrono::Utc::now() - chrono::Duration::days(30)).naive_utc()
        )]
    ).await?;

    println!("Average age of users created in last 30 days: {:.1} years", average_age);

    // Get multiple scalar values
    let departments: Vec<String> = RawQuery::many(
        pool,
        "SELECT DISTINCT department FROM users WHERE age >= $1 ORDER BY department",
        vec![QueryValue::integer(18)]
    ).await?;

    println!("Departments with adult users:");
    for dept in departments {
        println!("- {}", dept);
    }

    Ok(())
}
```

### UPDATE and DELETE Operations

```rust
async fn raw_update_delete_example(pool: &PgPool) -> anyhow::Result<()> {
    println!("\n=== Raw UPDATE/DELETE Example ===");

    // Update multiple records
    let updated_rows = RawQueryBuilder::new(
        "UPDATE users SET age = age + 1 WHERE department = $1 AND age < $2"
    )
    .bind(QueryValue::text("Engineering"))
    .bind(QueryValue::integer(65))
    .execute_raw(pool)
    .await?;

    println!("Updated {} Engineering employees (age increment)", updated_rows);

    // Delete records matching criteria
    let deleted_rows = RawQueryBuilder::new(
        "DELETE FROM users WHERE email LIKE $1 AND created_at < $2"
    )
    .bind(QueryValue::text("%@temp.com"))
    .bind(QueryValue::Timestamp(
        (chrono::Utc::now() - chrono::Duration::days(365)).naive_utc()
    ))
    .execute_raw(pool)
    .await?;

    println!("Deleted {} temporary email accounts older than 1 year", deleted_rows);

    Ok(())
}
```

## Database Migrations

### Creating Migrations

```rust
async fn migration_example() -> anyhow::Result<()> {
    println!("\n=== Migration Example ===");

    let database_url = "postgresql://root:password@localhost/backbone_examples";
    let pool = PgPool::connect(database_url).await?;

    // Create migration manager
    let migration_manager = MigrationManager::new(&pool);

    // Create a new migration
    let migration = Migration::new(
        "001_create_users_table".to_string(),
        r#"
        CREATE TABLE users (
            id VARCHAR(255) PRIMARY KEY,
            name VARCHAR(255) NOT NULL,
            email VARCHAR(255) UNIQUE NOT NULL,
            age INTEGER NOT NULL,
            department VARCHAR(100),
            created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
        );

        CREATE INDEX idx_users_email ON users(email);
        CREATE INDEX idx_users_department ON users(department);
        CREATE INDEX idx_users_created_at ON users(created_at);
        "#.to_string(),
        Some(r#"DROP TABLE IF EXISTS users;"#.to_string()),
    );

    // Add another migration
    let user_profiles_migration = Migration::new(
        "002_create_user_profiles_table".to_string(),
        r#"
        CREATE TABLE user_profiles (
            user_id VARCHAR(255) PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
            bio TEXT,
            avatar_url VARCHAR(500),
            phone VARCHAR(20),
            address TEXT,
            birth_date DATE,
            created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
        );

        CREATE INDEX idx_user_profiles_phone ON user_profiles(phone);
        "#.to_string(),
        Some("DROP TABLE IF EXISTS user_profiles;".to_string()),
    );

    // Add migrations to manager
    migration_manager.add_migration(migration);
    migration_manager.add_migration(user_profiles_migration);

    // Run pending migrations
    migration_manager.migrate().await?;

    // Check migration status
    let status = migration_manager.status().await?;
    println!("Migration status: {:?}", status);

    Ok(())
}
```

### Rolling Back Migrations

```rust
async fn rollback_example() -> anyhow::Result<()> {
    println!("\n=== Rollback Example ===");

    let database_url = "postgresql://root:password@localhost/backbone_examples";
    let pool = PgPool::connect(database_url).await?;
    let migration_manager = MigrationManager::new(&pool);

    // Rollback last migration
    migration_manager.rollback().await?;
    println!("Rolled back last migration");

    // Rollback multiple migrations
    migration_manager.rollback_n(2).await?;
    println!("Rolled back 2 migrations");

    // Check migration history
    let history = migration_manager.history().await?;
    println!("Migration history:");
    for record in history {
        println!("- {} ({})", record.name, record.status);
    }

    Ok(())
}
```

## Database Seeding

### Creating and Running Seeds

```rust
use backbone_orm::seeding::*;
use serde_json::json;

async fn seeding_example() -> anyhow::Result<()> {
    println!("\n=== Seeding Example ===");

    let database_url = "postgresql://root:password@localhost/backbone_examples";
    let pool = PgPool::connect(database_url).await?;

    // Create seed manager
    let seed_manager = SeedManager::new(&pool);

    // Initialize seed tracking table
    seed_manager.initialize().await?;

    // Create reference data seeds
    let departments_seed = Seed::new(
        "001_create_departments",
        SeedType::Reference,
        r#"
        INSERT INTO departments (id, name, description) VALUES
        ('eng', 'Engineering', 'Software development and IT'),
        ('sales', 'Sales', 'Business development and sales'),
        ('hr', 'Human Resources', 'Employee management and HR operations'),
        ('finance', 'Finance', 'Financial planning and accounting'),
        ('marketing', 'Marketing', 'Marketing and communications')
        ON CONFLICT (id) DO NOTHING;
        "#
    );

    // Create test data seeds
    let users_seed = Seed::new(
        "002_create_test_users",
        SeedType::Test,
        r#"
        INSERT INTO users (id, name, email, age, department) VALUES
        ('001', 'Alice Johnson', 'alice@company.com', 28, 'eng'),
        ('002', 'Bob Smith', 'bob@company.com', 35, 'sales'),
        ('003', 'Carol Davis', 'carol@company.com', 31, 'eng'),
        ('004', 'David Wilson', 'david@company.com', 42, 'finance'),
        ('005', 'Eva Brown', 'eva@company.com', 29, 'marketing')
        ON CONFLICT (id) DO NOTHING;
        "#
    );

    // Create JSON bulk data seed
    let products_json = json!([
        {
            "id": "prod-001",
            "name": "Laptop Pro",
            "category": "Electronics",
            "price": 1299.99,
            "stock": 50
        },
        {
            "id": "prod-002",
            "name": "Wireless Mouse",
            "category": "Electronics",
            "price": 29.99,
            "stock": 200
        },
        {
            "id": "prod-003",
            "name": "Office Chair",
            "category": "Furniture",
            "price": 249.99,
            "stock": 25
        }
    ]);

    let products_seed = Seed::new_json(
        "003_create_products",
        SeedType::Data,
        "products",
        products_json
    )?;

    // Run all seeds
    seed_manager.run_seed(&departments_seed).await?;
    seed_manager.run_seed(&users_seed).await?;
    seed_manager.run_seed(&products_seed).await?;

    // Check seed status
    let status = seed_manager.status().await?;
    println!("Seed status:");
    println!("Total seeds: {}", status.total_seeds);
    println!("Applied seeds: {}", status.applied_seeds.len());
    println!("Pending seeds: {}", status.pending_seeds.len());

    if let Some(last_seed) = status.last_seed {
        println!("Last applied seed: {}", last_seed);
    }

    // Rollback a seed (if needed)
    // seed_manager.rollback_seed(&users_seed).await?;
    // println!("Rolled back user seed");

    Ok(())
}
```

### YAML Seed Configuration

```yaml
# seeds.yaml
seeds:
  departments:
    type: Reference
    sql: |
      INSERT INTO departments (id, name, description) VALUES
      ('eng', 'Engineering', 'Software development and IT'),
      ('sales', 'Sales', 'Business development and sales'),
      ('hr', 'Human Resources', 'Employee management and HR operations')
      ON CONFLICT (id) DO NOTHING;
    rollback: DELETE FROM departments;

  admin_users:
    type: Data
    sql: |
      INSERT INTO users (id, name, email, age, department) VALUES
      ('admin001', 'System Admin', 'admin@company.com', 35, 'eng'),
      ('admin002', 'HR Manager', 'hr@company.com', 38, 'hr')
      ON CONFLICT (id) DO NOTHING;
    rollback: DELETE FROM users WHERE id LIKE 'admin%';

  sample_products:
    type: Test
    data_file: data/products.json
    table: products
    rollback: DELETE FROM products WHERE name LIKE 'Sample%';
```

## Real-World Scenarios

### E-commerce Application

```rust
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
struct Order {
    id: String,
    user_id: String,
    total_amount: f64,
    status: String,
    order_date: DateTime<Utc>,
    shipping_address: String,
}

#[derive(Debug, FromRow)]
struct OrderAnalytics {
    date: chrono::NaiveDate,
    total_orders: i64,
    total_revenue: f64,
    avg_order_value: f64,
    unique_customers: i64,
}

async fn ecommerce_analytics_example(pool: &PgPool) -> anyhow::Result<()> {
    println!("\n=== E-commerce Analytics Example ===");

    // Daily sales analytics for the last 30 days
    let thirty_days_ago = Utc::now() - chrono::Duration::days(30);

    let analytics_query = r#"
    WITH daily_sales AS (
        SELECT
            DATE(order_date) as order_date,
            COUNT(*) as total_orders,
            COALESCE(SUM(total_amount), 0) as total_revenue,
            COALESCE(SUM(total_amount), 0) / NULLIF(COUNT(*), 0) as avg_order_value,
            COUNT(DISTINCT user_id) as unique_customers
        FROM orders
        WHERE order_date >= $1
        AND status NOT IN ('cancelled', 'refunded')
        GROUP BY DATE(order_date)
    ),
    moving_averages AS (
        SELECT
            order_date,
            total_orders,
            total_revenue,
            avg_order_value,
            unique_customers,
            AVG(total_revenue) OVER (ORDER BY order_date ROWS BETWEEN 6 PRECEDING AND CURRENT ROW) as revenue_7day_ma
        FROM daily_sales
    )
    SELECT * FROM moving_averages
    ORDER BY order_date DESC
    LIMIT 30;
    "#;

    let analytics: Vec<OrderAnalytics> = RawQueryBuilder::new(analytics_query)
        .bind(QueryValue::Timestamp(thirty_days_ago.naive_utc()))
        .execute(pool)
        .await?;

    println!("E-commerce Analytics (Last 30 Days):");
    println!("Date\t\tOrders\tRevenue\tAvg Order\t7-Day MA\tCustomers");
    println!("{}", "-".repeat(70));

    for analytics in analytics.iter().take(10) {
        println!("{}\t{}\t${:.2}\t${:.2}\t${:.2}\t{}",
                analytics.date,
                analytics.total_orders,
                analytics.total_revenue,
                analytics.avg_order_value,
                analytics.total_revenue, // Simplified 7-day MA for display
                analytics.unique_customers);
    }

    // Top customers by lifetime value
    let top_customers_query = r#"
    SELECT
        u.id,
        u.name,
        u.email,
        COUNT(o.id) as total_orders,
        COALESCE(SUM(o.total_amount), 0) as lifetime_value,
        COALESCE(AVG(o.total_amount), 0) as avg_order_value,
        MIN(o.order_date) as first_order_date,
        MAX(o.order_date) as last_order_date
    FROM users u
    LEFT JOIN orders o ON u.id = o.user_id
    WHERE o.status NOT IN ('cancelled', 'refunded')
    GROUP BY u.id, u.name, u.email
    HAVING COUNT(o.id) >= 1
    ORDER BY lifetime_value DESC
    LIMIT 20;
    "#;

    #[derive(Debug, FromRow)]
    struct CustomerAnalytics {
        id: String,
        name: String,
        email: String,
        total_orders: i64,
        lifetime_value: f64,
        avg_order_value: f64,
        first_order_date: Option<DateTime<Utc>>,
        last_order_date: Option<DateTime<Utc>>,
    }

    let top_customers: Vec<CustomerAnalytics> = RawQueryBuilder::new(top_customers_query)
        .execute(pool)
        .await?;

    println!("\nTop 20 Customers by Lifetime Value:");
    println!("Rank\tName\t\tOrders\tLifetime\tAvg Order");
    println!("{}", "-".repeat(50));

    for (i, customer) in top_customers.iter().enumerate() {
        println!("{}\t{}\t{}\t${:.2}\t${:.2}",
                i + 1,
                truncate_string(&customer.name, 15),
                customer.total_orders,
                customer.lifetime_value,
                customer.avg_order_value);
    }

    Ok(())
}

fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}
```

### User Analytics Dashboard

```rust
#[derive(Debug, FromRow)]
struct UserEngagementMetrics {
    date: chrono::NaiveDate,
    active_users: i64,
    new_users: i64,
    returning_users: i64,
    sessions_count: i64,
    avg_session_duration: f64, // in minutes
    page_views: i64,
}

async fn user_analytics_dashboard(pool: &PgPool) -> anyhow::Result<()> {
    println!("\n=== User Analytics Dashboard ===");

    // User engagement metrics for last 14 days
    let fourteen_days_ago = Utc::now() - chrono::Duration::days(14);

    let engagement_query = r#"
    WITH daily_sessions AS (
        SELECT
            DATE(session_start) as session_date,
            COUNT(DISTINCT user_id) as active_users,
            COUNT(*) as sessions_count,
            SUM(EXTRACT(EPOCH FROM (session_end - session_start))/60) as total_session_minutes,
            SUM(page_views) as total_page_views
        FROM user_sessions
        WHERE session_start >= $1
        GROUP BY DATE(session_start)
    ),
    new_users_daily AS (
        SELECT
            DATE(created_at) as creation_date,
            COUNT(*) as new_users_count
        FROM users
        WHERE created_at >= $1
        GROUP BY DATE(created_at)
    ),
    user_activity AS (
        SELECT
            COALESCE(ds.session_date, nu.creation_date) as activity_date,
            COALESCE(ds.active_users, 0) as daily_active,
            COALESCE(nu.new_users_count, 0) as new_users,
            COALESCE(ds.sessions_count, 0) as sessions,
            CASE
                WHEN ds.total_session_minutes > 0 THEN ds.total_session_minutes / NULLIF(ds.sessions_count, 0)
                ELSE 0
            END as avg_session_duration,
            COALESCE(ds.total_page_views, 0) as page_views,
            COALESCE(ds.active_users, 0) - COALESCE(nu.new_users_count, 0) as returning_users
        FROM daily_sessions ds
        FULL OUTER JOIN new_users_daily nu ON ds.session_date = nu.creation_date
        WHERE COALESCE(ds.session_date, nu.creation_date) >= $1
    )
    SELECT
        activity_date as date,
        daily_active as active_users,
        new_users,
        returning_users,
        sessions as sessions_count,
        avg_session_duration,
        page_views
    FROM user_activity
    ORDER BY activity_date DESC;
    "#;

    let metrics: Vec<UserEngagementMetrics> = RawQueryBuilder::new(engagement_query)
        .bind(QueryValue::Timestamp(fourteen_days_ago.naive_utc()))
        .execute(pool)
        .await?;

    println!("User Engagement Metrics (Last 14 Days):");
    println!("Date\t\tActive\tNew\tReturning\tSessions\tAvg Min\tPages");
    println!("{}", "-".repeat(60));

    for metric in metrics.iter().take(7) {
        println!("{}\t{}\t{}\t{}\t\t{}\t{:.1}\t{}",
                metric.date,
                metric.active_users,
                metric.new_users,
                metric.returning_users,
                metric.sessions_count,
                metric.avg_session_duration,
                metric.page_views);
    }

    // User retention analysis (cohort-based)
    let retention_query = r#"
    WITH user_cohorts AS (
        SELECT
            user_id,
            DATE_TRUNC('month', created_at) as cohort_month,
            DATE(created_at) as first_activity_date
        FROM users
        WHERE created_at >= DATE_TRUNC('month', CURRENT_DATE) - INTERVAL '6 months'
    ),
    cohort_activity AS (
        SELECT
            c.cohort_month,
            c.user_id,
            DATE_TRUNC('month', s.session_start) as activity_month,
            DATEDIFF(DATE_TRUNC('month', s.session_start), c.cohort_month) as period_number
        FROM user_cohorts c
        LEFT JOIN user_sessions s ON c.user_id = s.user_id
            AND s.session_start >= c.first_activity_date
            AND s.session_start < DATE_TRUNC('month', c.cohort_month) + INTERVAL '6 months'
    ),
    retention_rates AS (
        SELECT
            cohort_month,
            period_number,
            COUNT(DISTINCT user_id) as active_users,
            FIRST_VALUE(COUNT(DISTINCT user_id)) OVER (PARTITION BY cohort_month ORDER BY period_number) as cohort_size
        FROM cohort_activity
        GROUP BY cohort_month, period_number
    )
    SELECT
        cohort_month,
        period_number,
        active_users,
        cohort_size,
        ROUND((active_users::float / cohort_size::float) * 100, 2) as retention_rate
    FROM retention_rates
    ORDER BY cohort_month, period_number;
    "#;

    #[derive(Debug, FromRow)]
    struct RetentionMetrics {
        cohort_month: chrono::NaiveDateTime,
        period_number: i32,
        active_users: i64,
        cohort_size: i64,
        retention_rate: f64,
    }

    let retention_data: Vec<RetentionMetrics> = RawQueryBuilder::new(retention_query)
        .execute(pool)
        .await?;

    println!("\nUser Retention by Cohort:");
    println!("Cohort\t\tPeriod\tUsers\tSize\tRetention%");
    println!("{}", "-".repeat(40));

    // Group by cohort month for display
    let mut cohorts: std::collections::HashMap<String, Vec<RetentionMetrics>> = std::collections::HashMap::new();
    for metric in retention_data {
        let cohort_key = metric.cohort_month.format("%Y-%m").to_string();
        cohorts.entry(cohort_key).or_insert_with(Vec::new).push(metric);
    }

    for (cohort, periods) in cohorts.iter().take(3) {
        println!("\n{} Cohort:", cohort);
        for period in periods.iter().take(6) {
            println!("  Month {}\t\t{}\t{}\t{}\t{:.1}%",
                    period.period_number,
                    period.active_users,
                    period.cohort_size,
                    period.retention_rate);
        }
    }

    Ok(())
}
```

### Inventory Management System

```rust
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
struct Product {
    id: String,
    name: String,
    category: String,
    sku: String,
    current_stock: i32,
    min_stock_level: i32,
    max_stock_level: i32,
    unit_price: f64,
    supplier_id: String,
    last_restocked: Option<chrono::NaiveDateTime>,
}

#[derive(Debug, FromRow)]
struct InventoryAlert {
    product_id: String,
    product_name: String,
    current_stock: i32,
    min_stock_level: i32,
    shortage_amount: i32,
    days_since_restock: i64,
    urgency_level: String,
}

async fn inventory_management_example(pool: &PgPool) -> anyhow::Result<()> {
    println!("\n=== Inventory Management Example ===");

    // Low stock alerts with urgency levels
    let low_stock_query = r#"
    WITH inventory_analysis AS (
        SELECT
            p.id,
            p.name,
            p.current_stock,
            p.min_stock_level,
            p.max_stock_level,
            p.sku,
            p.category,
            CASE
                WHEN p.current_stock = 0 THEN 'OUT_OF_STOCK'
                WHEN p.current_stock < p.min_stock_level * 0.5 THEN 'CRITICAL'
                WHEN p.current_stock < p.min_stock_level THEN 'LOW'
                ELSE 'OK'
            END as stock_status,
            p.last_restocked,
            DATEDIFF(NOW(), p.last_restocked) as days_since_restock,
            p.min_stock_level - p.current_stock as shortage_amount,
            s.name as supplier_name,
            s.lead_time_days
        FROM products p
        JOIN suppliers s ON p.supplier_id = s.id
        WHERE p.current_stock < p.min_stock_level OR p.current_stock = 0
    )
    SELECT
        id,
        name,
        current_stock,
        min_stock_level,
        shortage_amount,
        days_since_restock,
        CASE
            WHEN stock_status = 'OUT_OF_STOCK' AND days_since_restock > 7 THEN 'URGENT'
            WHEN stock_status = 'CRITICAL' THEN 'HIGH'
            WHEN stock_status = 'LOW' AND days_since_restock > lead_time_days THEN 'MEDIUM'
            ELSE 'LOW'
        END as urgency_level
    FROM inventory_analysis
    ORDER BY
        CASE
            WHEN urgency_level = 'URGENT' THEN 1
            WHEN urgency_level = 'HIGH' THEN 2
            WHEN urgency_level = 'MEDIUM' THEN 3
            ELSE 4
        END,
        shortage_amount DESC;
    "#;

    let alerts: Vec<InventoryAlert> = RawQueryBuilder::new(low_stock_query)
        .execute(pool)
        .await?;

    println!("Inventory Stock Alerts:");
    println!("Product\t\t\t\tStock\tMin\tShortage\tDays\tUrgency");
    println!("{}", "-".repeat(70));

    for alert in alerts.iter().take(15) {
        println!("{}\t{}\t{}\t{}\t{}\t{} days\t{}",
                truncate_string(&alert.product_name, 25),
                alert.current_stock,
                alert.min_stock_level,
                alert.shortage_amount,
                alert.days_since_restock,
                alert.urgency_level);
    }

    // Inventory turnover analysis
    let turnover_query = r#"
    WITH sales_data AS (
        SELECT
            product_id,
            SUM(quantity) as total_sold,
            SUM(quantity * unit_price) as total_revenue,
            COUNT(DISTINCT DATE(order_date)) as days_with_sales
        FROM order_items oi
        JOIN orders o ON oi.order_id = o.id
        WHERE o.order_date >= NOW() - INTERVAL '90 days'
        AND o.status NOT IN ('cancelled', 'refunded')
        GROUP BY product_id
    ),
    inventory_metrics AS (
        SELECT
            p.id,
            p.name,
            p.current_stock,
            p.unit_price,
            p.min_stock_level,
            COALESCE(s.total_sold, 0) as total_sold_90d,
            COALESCE(s.total_revenue, 0) as revenue_90d,
            p.current_stock * p.unit_price as inventory_value
        FROM products p
        LEFT JOIN sales_data s ON p.id = s.product_id
    )
    SELECT
        name,
        current_stock,
        unit_price,
        inventory_value,
        total_sold_90d,
        revenue_90d,
        CASE
            WHEN total_sold_90d > 0 THEN ROUND((inventory_value / NULLIF(revenue_90d, 0)) * 90, 2)
            ELSE 999
        END as days_of_supply,
        CASE
            WHEN total_sold_90d > 0 THEN ROUND((total_sold_90d * 4) / NULLIF(current_stock, 0), 2)
            ELSE 0
        END as turnover_ratio
    FROM inventory_metrics
    WHERE current_stock > 0
    ORDER BY
        CASE
            WHEN turnover_ratio = 0 THEN 0
            ELSE turnover_ratio
        END DESC,
        revenue_90d DESC
    LIMIT 20;
    "#;

    #[derive(Debug, FromRow)]
    struct TurnoverAnalysis {
        name: String,
        current_stock: i32,
        unit_price: f64,
        inventory_value: f64,
        total_sold_90d: i64,
        revenue_90d: f64,
        days_of_supply: f64,
        turnover_ratio: f64,
    }

    let turnover_data: Vec<TurnoverAnalysis> = RawQueryBuilder::new(turnover_query)
        .execute(pool)
        .await?;

    println!("\nInventory Turnover Analysis (Top 20):");
    println!("Product\t\t\t\tStock\tPrice\tValue\tSold\tRevenue\tDays Supply\tTurnover");
    println!("{}", "-".repeat(80));

    for item in turnover_data {
        println!("{}\t{}\t${:.2}\t${:.2}\t{}\t${:.2}\t{:.1} days\t{:.2}x",
                truncate_string(&item.name, 20),
                item.current_stock,
                item.unit_price,
                item.inventory_value,
                item.total_sold_90d,
                item.revenue_90d,
                item.days_of_supply,
                item.turnover_ratio);
    }

    // Recommended reorder quantities
    let reorder_query = r#"
    WITH demand_forecast AS (
        SELECT
            product_id,
            AVG(quantity) as avg_daily_demand,
            MAX(quantity) as peak_daily_demand,
            STDDEV(quantity) as demand_stddev
        FROM (
            SELECT
                product_id,
                DATE(order_date) as order_date,
                SUM(quantity) as quantity
            FROM order_items oi
            JOIN orders o ON oi.order_id = o.id
            WHERE o.order_date >= NOW() - INTERVAL '60 days'
            AND o.status NOT IN ('cancelled', 'refunded')
            GROUP BY product_id, DATE(order_date)
        ) daily_demand
        GROUP BY product_id
    ),
    supplier_lead_times AS (
        SELECT
            p.id as product_id,
            AVG(s.lead_time_days) as avg_lead_time,
            MAX(s.lead_time_days) as max_lead_time
        FROM products p
        JOIN suppliers s ON p.supplier_id = s.id
        GROUP BY p.id
    ),
    reorder_calculations AS (
        SELECT
            p.id,
            p.name,
            p.current_stock,
            p.min_stock_level,
            COALESCE(df.avg_daily_demand, 0) as avg_demand,
            COALESCE(df.peak_daily_demand, 0) as peak_demand,
            COALESCE(slt.avg_lead_time, 7) as avg_lead_time,
            COALESCE(slt.max_lead_time, 14) as max_lead_time
        FROM products p
        LEFT JOIN demand_forecast df ON p.id = df.product_id
        LEFT JOIN supplier_lead_times slt ON p.id = slt.product_id
        WHERE p.current_stock < p.min_stock_level
        OR p.current_stock < (COALESCE(df.avg_daily_demand, 1) * COALESCE(slt.max_lead_time, 14))
    )
    SELECT
        name,
        current_stock,
        min_stock_level,
        avg_demand,
        peak_demand,
        avg_lead_time,
        max_lead_time,
        CEIL(GREATEST(
            min_stock_level - current_stock + (avg_demand * max_lead_time * 1.5),
            peak_demand * max_lead_time * 0.3
        )) as recommended_order_qty
    FROM reorder_calculations
    ORDER BY recommended_order_qty DESC
    LIMIT 15;
    "#;

    #[derive(Debug, FromRow)]
    struct ReorderRecommendation {
        name: String,
        current_stock: i32,
        min_stock_level: i32,
        avg_demand: f64,
        peak_demand: f64,
        avg_lead_time: f64,
        max_lead_time: f64,
        recommended_order_qty: i64,
    }

    let reorder_data: Vec<ReorderRecommendation> = RawQueryBuilder::new(reorder_query)
        .execute(pool)
        .await?;

    println!("\nReorder Recommendations:");
    println!("Product\t\t\t\tStock\tMin\tAvg Demand\tLead Time\tRec Order");
    println!("{}", "-".repeat(60));

    for rec in reorder_data {
        println!("{}\t{}\t{}\t{:.1}/day\t{:.0} days\t{} units",
                truncate_string(&rec.name, 20),
                rec.current_stock,
                rec.min_stock_level,
                rec.avg_demand,
                rec.max_lead_time,
                rec.recommended_order_qty);
    }

    Ok(())
}
```

### Financial Reporting Dashboard

```rust
#[derive(Debug, FromRow)]
struct FinancialSummary {
    period: String,
    total_revenue: f64,
    total_expenses: f64,
    net_income: f64,
    operating_margin: f64,
    growth_rate: f64,
}

#[derive(Debug, FromRow)]
struct RevenueBreakdown {
    category: String,
    revenue: f64,
    percentage: f64,
    growth_vs_previous: f64,
}

async fn financial_reporting_example(pool: &PgPool) -> anyhow::Result<()> {
    println!("\n=== Financial Reporting Dashboard ===");

    // Monthly financial performance with trends
    let financial_query = r#"
    WITH monthly_data AS (
        SELECT
            DATE_TRUNC('month, order_date) as period,
            SUM(CASE WHEN revenue_type = 'sale' THEN amount ELSE 0 END) as revenue,
            SUM(CASE WHEN revenue_type = 'refund' THEN amount ELSE 0 END) as refunds,
            SUM(CASE WHEN expense_type IS NOT NULL THEN amount ELSE 0 END) as expenses
        FROM financial_transactions ft
        WHERE ft.transaction_date >= NOW() - INTERVAL '24 months'
        GROUP BY DATE_TRUNC('month, order_date)
    ),
    monthly_metrics AS (
        SELECT
            period,
            revenue,
            refunds,
            expenses,
            revenue - refunds - expenses as net_income,
            CASE
                WHEN revenue > 0 THEN ((revenue - refunds - expenses) / revenue) * 100
                ELSE 0
            END as operating_margin,
            LAG(revenue, 1) OVER (ORDER BY period) as prev_revenue
        FROM monthly_data
    )
    SELECT
        TO_CHAR(period, 'YYYY-MM') as period,
        revenue,
        expenses,
        net_income,
        operating_margin,
        CASE
            WHEN prev_revenue > 0 THEN ((revenue - prev_revenue) / prev_revenue) * 100
            ELSE 0
        END as growth_rate
    FROM monthly_metrics
    ORDER BY period DESC
    LIMIT 12;
    "#;

    let financial_data: Vec<FinancialSummary> = RawQueryBuilder::new(financial_query)
        .execute(pool)
        .await?;

    println!("Monthly Financial Performance (Last 12 Months):");
    println!("Period\tRevenue\tExpenses\tNet Income\tMargin\tGrowth");
    println!("{}", "-".repeat(60));

    for period in financial_data.iter().rev().take(6) {
        println!("{}\t${:.1}K\t${:.1}K\t${:.1}K\t{:.1}%\t{:.1}%",
                period.period,
                period.total_revenue / 1000.0,
                period.total_expenses / 1000.0,
                period.net_income / 1000.0,
                period.operating_margin,
                period.growth_rate);
    }

    // Revenue breakdown by category
    let revenue_breakdown_query = r#"
    WITH current_period_revenue AS (
        SELECT
            category,
            SUM(amount) as total_revenue
        FROM financial_transactions
        WHERE revenue_type = 'sale'
        AND transaction_date >= DATE_TRUNC('month', CURRENT_DATE) - INTERVAL '1 month'
        GROUP BY category
    ),
    previous_period_revenue AS (
        SELECT
            category,
            SUM(amount) as total_revenue
        FROM financial_transactions
        WHERE revenue_type = 'sale'
        AND transaction_date >= DATE_TRUNC('month', CURRENT_DATE) - INTERVAL '2 months'
        AND transaction_date < DATE_TRUNC('month', CURRENT_DATE) - INTERVAL '1 month'
        GROUP BY category
    ),
    total_revenue AS (
        SELECT SUM(total_revenue) as grand_total
        FROM current_period_revenue
    )
    SELECT
        c.category,
        c.total_revenue as revenue,
        ROUND((c.total_revenue / t.grand_total) * 100, 2) as percentage,
        CASE
            WHEN p.total_revenue > 0 THEN
                ROUND(((c.total_revenue - p.total_revenue) / p.total_revenue) * 100, 2)
            ELSE 0
        END as growth_vs_previous
    FROM current_period_revenue c
    JOIN total_revenue t ON 1=1
    LEFT JOIN previous_period_revenue p ON c.category = p.category
    ORDER BY revenue DESC;
    "#;

    let revenue_data: Vec<RevenueBreakdown> = RawQueryBuilder::new(revenue_breakdown_query)
        .execute(pool)
        .await?;

    println!("\nRevenue Breakdown (Current Month):");
    println!("Category\t\t\tRevenue\tShare\tGrowth");
    println!("{}", "-".repeat(50));

    for category in revenue_data {
        println!("{}\t\t${:.1}K\t{:.1}%\t{:.1}%",
                truncate_string(&category.category, 20),
                category.revenue / 1000.0,
                category.percentage,
                category.growth_vs_previous);
    }

    // Cash flow analysis
    let cashflow_query = r#"
    WITH cash_flow_data AS (
        SELECT
            DATE_TRUNC('week', transaction_date) as week,
            SUM(CASE WHEN cash_flow_direction = 'inflow' THEN amount ELSE 0 END) as cash_in,
            SUM(CASE WHEN cash_flow_direction = 'outflow' THEN amount ELSE 0 END) as cash_out,
            SUM(CASE WHEN cash_flow_direction = 'inflow' THEN amount ELSE 0 END) -
            SUM(CASE WHEN cash_flow_direction = 'outflow' THEN amount ELSE 0 END) as net_cash_flow
        FROM financial_transactions
        WHERE transaction_date >= NOW() - INTERVAL '12 weeks'
        GROUP BY DATE_TRUNC('week', transaction_date)
    ),
    cash_flow_metrics AS (
        SELECT
            week,
            cash_in,
            cash_out,
            net_cash_flow,
            SUM(net_cash_flow) OVER (ORDER BY week ROWS BETWEEN 11 PRECEDING AND CURRENT ROW) as rolling_12wk_cash,
            AVG(net_cash_flow) OVER (ORDER BY week ROWS BETWEEN 3 PRECEDING AND CURRENT ROW) as avg_4wk_cash,
            STDDEV(net_cash_flow) OVER (ORDER BY week ROWS BETWEEN 7 PRECEDING AND CURRENT ROW) as cash_flow_volatility
        FROM cash_flow_data
        ORDER BY week DESC
    )
    SELECT
        TO_CHAR(week, 'YYYY-MM-DD') as week,
        cash_in,
        cash_out,
        net_cash_flow,
        rolling_12wk_cash,
        avg_4wk_cash,
        cash_flow_volatility
    FROM cash_flow_metrics
    ORDER BY week DESC
    LIMIT 12;
    "#;

    #[derive(Debug, FromRow)]
    struct CashFlowMetrics {
        week: String,
        cash_in: f64,
        cash_out: f64,
        net_cash_flow: f64,
        rolling_12wk_cash: f64,
        avg_4wk_cash: f64,
        cash_flow_volatility: f64,
    }

    let cashflow_data: Vec<CashFlowMetrics> = RawQueryBuilder::new(cashflow_query)
        .execute(pool)
        .await?;

    println!("\nCash Flow Analysis (Last 12 Weeks):");
    println!("Week\t\tIn\t\tOut\t\tNet\t\t12Wk Total\t4Wk Avg\tVolatility");
    println!("{}", "-".repeat(70));

    for cf in cashflow_data.iter().rev() {
        println!("{}\t${:.1}K\t${:.1}K\t${:.1}K\t${:.1}K\t\t${:.1}K\t${:.1}K",
                cf.week,
                cf.cash_in / 1000.0,
                cf.cash_out / 1000.0,
                cf.net_cash_flow / 1000.0,
                cf.rolling_12wk_cash / 1000.0,
                cf.avg_4wk_cash / 1000.0,
                cf.cash_flow_volatility / 1000.0);
    }

    // Profitability trends by product line
    let profitability_query = r#"
    WITH product_line_metrics AS (
        SELECT
            product_line,
            DATE_TRUNC('month', order_date) as month,
            SUM(revenue) as revenue,
            SUM(cogs) as cost_of_goods_sold,
            SUM(shipping_cost + marketing_cost + other_expenses) as operating_expenses,
            SUM(revenue - cogs - shipping_cost - marketing_cost - other_expenses) as profit
        FROM profit_loss_by_order
        WHERE order_date >= NOW() - INTERVAL '12 months'
        GROUP BY product_line, DATE_TRUNC('month', order_date)
    ),
    profitability_trends AS (
        SELECT
            product_line,
            AVG(revenue) as avg_monthly_revenue,
            AVG(profit) as avg_monthly_profit,
            AVG(CASE WHEN revenue > 0 THEN (profit / revenue) * 100 ELSE 0 END) as avg_profit_margin,
            STDDEV(CASE WHEN revenue > 0 THEN (profit / revenue) * 100 ELSE 0 END) as margin_volatility,
            COUNT(*) as months_analyzed,
            SUM(CASE WHEN profit > 0 THEN 1 ELSE 0 END) as profitable_months
        FROM product_line_metrics
        GROUP BY product_line
        HAVING COUNT(*) >= 6  -- At least 6 months of data
    )
    SELECT
        product_line,
        avg_monthly_revenue,
        avg_monthly_profit,
        avg_profit_margin,
        margin_volatility,
        ROUND((profitable_months::float / months_analyzed) * 100, 2) as profitability_rate
    FROM profitability_trends
    ORDER BY avg_monthly_revenue DESC;
    "#;

    #[derive(Debug, FromRow)]
    struct ProfitabilityAnalysis {
        product_line: String,
        avg_monthly_revenue: f64,
        avg_monthly_profit: f64,
        avg_profit_margin: f64,
        margin_volatility: f64,
        profitability_rate: f64,
    }

    let profitability_data: Vec<ProfitabilityAnalysis> = RawQueryBuilder::new(profitability_query)
        .execute(pool)
        .await?;

    println!("\nProduct Line Profitability Analysis:");
    println!("Product Line\t\t\tMonthly Rev\tMonthly Profit\tMargin\tVolatility\tProfit Rate");
    println!("{}", "-".repeat(70));

    for prod in profitability_data {
        println!("{}\t${:.1}K\t\t${:.1}K\t\t{:.1}%\t{:.1}%\t\t{:.1}%",
                truncate_string(&prod.product_line, 20),
                prod.avg_monthly_revenue / 1000.0,
                prod.avg_monthly_profit / 1000.0,
                prod.avg_profit_margin,
                prod.margin_volatility,
                prod.profitability_rate);
    }

    Ok(())
}
```

## Complete Example Application

```rust
use backbone_orm::*;
use sqlx::{PgPool, FromRow};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
struct Product {
    id: String,
    name: String,
    category: String,
    price: f64,
    stock: i32,
}

async fn complete_example() -> anyhow::Result<()> {
    println!("🚀 Backbone-ORM Complete Example Application");
    println!("=" .repeat(50));

    // Initialize database
    let database_url = "postgresql://root:password@localhost/backbone_examples";
    let pool = PgPool::connect(database_url).await?;

    // Setup tables with migrations
    setup_database(&pool).await?;

    // Seed initial data
    seed_database(&pool).await?;

    // Run examples
    basic_crud_examples(&pool).await?;
    query_builder_examples(&pool).await?;
    advanced_querying_examples(&pool).await?;
    real_world_scenarios(&pool).await?;

    println!("\n✅ All examples completed successfully!");
    Ok(())
}

async fn setup_database(pool: &PgPool) -> anyhow::Result<()> {
    println!("\n📋 Setting up database...");

    let migration_manager = MigrationManager::new(pool);

    // Create products table migration
    let products_migration = Migration::new(
        "001_create_products_table",
        r#"
        CREATE TABLE products (
            id VARCHAR(255) PRIMARY KEY,
            name VARCHAR(255) NOT NULL,
            category VARCHAR(100) NOT NULL,
            price DECIMAL(10,2) NOT NULL,
            stock INTEGER NOT NULL DEFAULT 0,
            created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
        );

        CREATE INDEX idx_products_category ON products(category);
        CREATE INDEX idx_products_price ON products(price);
        CREATE INDEX idx_products_stock ON products(stock);
        "#,
        Some("DROP TABLE IF EXISTS products;")
    );

    // Create orders table migration
    let orders_migration = Migration::new(
        "002_create_orders_table",
        r#"
        CREATE TABLE orders (
            id VARCHAR(255) PRIMARY KEY,
            product_id VARCHAR(255) NOT NULL REFERENCES products(id),
            quantity INTEGER NOT NULL,
            total_price DECIMAL(10,2) NOT NULL,
            status VARCHAR(50) NOT NULL DEFAULT 'pending',
            order_date TIMESTAMP WITH TIME ZONE DEFAULT NOW()
        );

        CREATE INDEX idx_orders_product_id ON orders(product_id);
        CREATE INDEX idx_orders_status ON orders(status);
        CREATE INDEX idx_orders_order_date ON orders(order_date);
        "#,
        Some("DROP TABLE IF EXISTS orders;")
    );

    migration_manager.add_migration(products_migration);
    migration_manager.add_migration(orders_migration);
    migration_manager.migrate().await?;

    println!("✅ Database setup completed");
    Ok(())
}

async fn seed_database(pool: &PgPool) -> anyhow::Result<()> {
    println!("\n🌱 Seeding database...");

    let seed_manager = SeedManager::new(pool);
    seed_manager.initialize().await?;

    // Create products seed
    let products_seed = Seed::new(
        "001_create_products",
        SeedType::Data,
        r#"
        INSERT INTO products (id, name, category, price, stock) VALUES
        ('prod-001', 'Laptop Pro', 'Electronics', 1299.99, 50),
        ('prod-002', 'Wireless Mouse', 'Electronics', 29.99, 200),
        ('prod-003', 'Office Chair', 'Furniture', 249.99, 25),
        ('prod-004', 'Standing Desk', 'Furniture', 599.99, 15),
        ('prod-005', 'Monitor 4K', 'Electronics', 399.99, 30),
        ('prod-006', 'Keyboard Mechanical', 'Electronics', 89.99, 75),
        ('prod-007', 'Desk Lamp', 'Furniture', 45.99, 40),
        ('prod-008', 'USB-C Hub', 'Electronics', 49.99, 100)
        ON CONFLICT (id) DO NOTHING;
        "#
    );

    // Create orders seed
    let orders_seed = Seed::new(
        "002_create_orders",
        SeedType::Test,
        r#"
        INSERT INTO orders (id, product_id, quantity, total_price, status, order_date) VALUES
        ('order-001', 'prod-001', 1, 1299.99, 'completed', NOW() - INTERVAL '7 days'),
        ('order-002', 'prod-002', 2, 59.98, 'completed', NOW() - INTERVAL '5 days'),
        ('order-003', 'prod-003', 1, 249.99, 'pending', NOW() - INTERVAL '3 days'),
        ('order-004', 'prod-004', 2, 1199.98, 'shipped', NOW() - INTERVAL '2 days'),
        ('order-005', 'prod-005', 3, 1199.97, 'completed', NOW() - INTERVAL '1 day'),
        ('order-006', 'prod-001', 1, 1299.99, 'pending', NOW()),
        ('order-007', 'prod-006', 1, 89.99, 'completed', NOW() - INTERVAL '6 hours'),
        ('order-008', 'prod-007', 5, 229.95, 'shipped', NOW() - INTERVAL '3 hours')
        ON CONFLICT (id) DO NOTHING;
        "#
    );

    seed_manager.run_seed(&products_seed).await?;
    seed_manager.run_seed(&orders_seed).await?;

    println!("✅ Database seeding completed");
    Ok(())
}

// ... (Include all the example functions from above)

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    complete_example().await
}
```

## Performance Tips

### 1. Connection Pool Configuration

```rust
use sqlx::postgres::PgPoolOptions;

let pool = PgPoolOptions::new()
    .max_connections(20)
    .min_connections(5)
    .connect_timeout(Duration::from_secs(30))
    .idle_timeout(Duration::from_secs(600))
    .max_lifetime(Duration::from_secs(1800))
    .connect(&database_url)
    .await?;
```

### 2. Batch Operations

```rust
// Insert multiple records efficiently
let users = vec![
    User { /* ... */ },
    User { /* ... */ },
    // ... more users
];

let mut transaction = pool.begin().await?;
for user in users {
    sqlx::query("INSERT INTO users (id, name, email) VALUES ($1, $2, $3)")
        .bind(&user.id)
        .bind(&user.name)
        .bind(&user.email)
        .execute(&mut *transaction)
        .await?;
}
transaction.commit().await?;
```

### 3. Query Optimization

```rust
// Use specific fields instead of SELECT *
let users: Vec<(String, String)> = sqlx::query_as(
    "SELECT id, name FROM users WHERE active = true"
)
    .fetch_all(&pool)
    .await?;

// Use LIMIT for large result sets
let recent_users: Vec<User> = QueryBuilder::new("users")
    .order_by("created_at", "DESC")
    .limit(100)  // Always limit large queries
    .build()
    .fetch_all(&pool)
    .await?;
```

## Error Handling

```rust
use thiserror::Error;

#[derive(Error, Debug)]
enum RepositoryError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("User not found: {id}")]
    UserNotFound { id: String },

    #[error("Validation error: {field} - {message}")]
    ValidationError { field: String, message: String },
}

// Implement error handling in your repository
impl PostgresRepository<User> {
    pub async fn find_by_id_or_error(&self, id: &str) -> Result<User, RepositoryError> {
        match self.find_by_id(id).await? {
            Some(user) => Ok(user),
            None => Err(RepositoryError::UserNotFound {
                id: id.to_string()
            }),
        }
    }
}
```

This comprehensive guide covers all major features of backbone-orm from basic CRUD operations to advanced real-world scenarios. Each example includes error handling and follows best practices for production-ready applications.