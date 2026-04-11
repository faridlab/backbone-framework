//! Database integration example for backbone-auth
//! Demonstrates PostgreSQL and MongoDB integration patterns

use backbone_auth::*;
use uuid::Uuid;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use async_trait::async_trait;

// PostgreSQL integration example
#[cfg(feature = "postgres")]
mod postgres_integration {
    use super::*;
    use sqlx::{PgPool, Row};
    use chrono::{DateTime, Utc};

    /// PostgreSQL user database implementation
    pub struct PostgresUserRepository {
        pool: PgPool,
    }

    impl PostgresUserRepository {
        pub async fn new(database_url: &str) -> Result<Self> {
            let pool = PgPool::connect(database_url).await
                .map_err(|e| anyhow::anyhow!("Failed to connect to PostgreSQL: {}", e))?;

            // Run migrations
            sqlx::migrate!("./migrations").run(&pool).await
                .map_err(|e| anyhow::anyhow!("Failed to run migrations: {}", e))?;

            Ok(Self { pool })
        }

        pub async fn create_sample_data(&self) -> Result<()> {
            let user_id = Uuid::new_v4();
            let password_hash = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$RdescudvJCsgt3ub+b+dWRWJTmaaJObG";

            sqlx::query!(
                r#"
                INSERT INTO users (id, email, password_hash, roles, is_active, is_locked,
                                  two_factor_enabled, two_factor_methods, created_at, updated_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                ON CONFLICT (email) DO NOTHING
                "#,
                user_id,
                "admin@startapp.id",
                password_hash,
                &["admin".to_string()],
                true,
                false,
                true,
                &["totp".to_string()],
                Utc::now(),
                Utc::now()
            )
            .execute(&self.pool)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create sample data: {}", e))?;

            println!("✅ Sample PostgreSQL user data created");
            Ok(())
        }
    }

    #[async_trait]
    impl UserRepository for PostgresUserRepository {
        async fn find_by_email(&self, email: &str) -> Result<Option<User>> {
            let row = sqlx::query!(
                r#"
                SELECT id, email, password_hash, roles, is_active, is_locked,
                       two_factor_enabled, two_factor_methods, account_expires_at,
                       requires_password_change, created_at, updated_at, deleted_at
                FROM users
                WHERE email = $1 AND deleted_at IS NULL
                "#,
                email
            )
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| anyhow::anyhow!("Database error: {}", e))?;

            if let Some(row) = row {
                Ok(Some(User {
                    id: row.id,
                    email: row.email,
                    password_hash: row.password_hash,
                    roles: row.roles.unwrap_or_default(),
                    is_active: row.is_active,
                    is_locked: row.is_locked,
                    two_factor_enabled: row.two_factor_enabled,
                    two_factor_methods: row.two_factor_methods.unwrap_or_default(),
                    account_expires_at: row.account_expires_at,
                    requires_password_change: row.requires_password_change,
                }))
            } else {
                Ok(None)
            }
        }

        async fn save(&self, user: &User) -> Result<()> {
            sqlx::query!(
                r#"
                INSERT INTO users (id, email, password_hash, roles, is_active, is_locked,
                                  two_factor_enabled, two_factor_methods, account_expires_at,
                                  requires_password_change, created_at, updated_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                "#,
                user.id,
                user.email,
                user.password_hash,
                &user.roles,
                user.is_active,
                user.is_locked,
                user.two_factor_enabled,
                &user.two_factor_methods,
                user.account_expires_at,
                user.requires_password_change,
                Utc::now(),
                Utc::now()
            )
            .execute(&self.pool)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to save user: {}", e))?;

            Ok(())
        }

        async fn update(&self, user: &User) -> Result<()> {
            sqlx::query!(
                r#"
                UPDATE users
                SET email = $2, password_hash = $3, roles = $4, is_active = $5,
                    is_locked = $6, two_factor_enabled = $7, two_factor_methods = $8,
                    account_expires_at = $9, requires_password_change = $10, updated_at = $11
                WHERE id = $1
                "#,
                user.id,
                user.email,
                user.password_hash,
                &user.roles,
                user.is_active,
                user.is_locked,
                user.two_factor_enabled,
                &user.two_factor_methods,
                user.account_expires_at,
                user.requires_password_change,
                Utc::now()
            )
            .execute(&self.pool)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to update user: {}", e))?;

            Ok(())
        }

        async fn delete(&self, user_id: &Uuid) -> Result<()> {
            sqlx::query!(
                "UPDATE users SET deleted_at = $1 WHERE id = $2",
                Utc::now(),
                user_id
            )
            .execute(&self.pool)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to delete user: {}", e))?;

            Ok(())
        }

        async fn find_by_id(&self, user_id: &Uuid) -> Result<Option<User>> {
            let row = sqlx::query!(
                r#"
                SELECT id, email, password_hash, roles, is_active, is_locked,
                       two_factor_enabled, two_factor_methods, account_expires_at,
                       requires_password_change, created_at, updated_at, deleted_at
                FROM users
                WHERE id = $1 AND deleted_at IS NULL
                "#,
                user_id
            )
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| anyhow::anyhow!("Database error: {}", e))?;

            if let Some(row) = row {
                Ok(Some(User {
                    id: row.id,
                    email: row.email,
                    password_hash: row.password_hash,
                    roles: row.roles.unwrap_or_default(),
                    is_active: row.is_active,
                    is_locked: row.is_locked,
                    two_factor_enabled: row.two_factor_enabled,
                    two_factor_methods: row.two_factor_methods.unwrap_or_default(),
                    account_expires_at: row.account_expires_at,
                    requires_password_change: row.requires_password_change,
                }))
            } else {
                Ok(None)
            }
        }
    }

    /// Session management with PostgreSQL
    pub struct PostgresSessionRepository {
        pool: PgPool,
    }

    impl PostgresSessionRepository {
        pub async fn new(pool: PgPool) -> Result<Self> {
            // Create sessions table
            sqlx::query!(
                r#"
                CREATE TABLE IF NOT EXISTS sessions (
                    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                    user_id UUID NOT NULL REFERENCES users(id),
                    token_hash VARCHAR(255) NOT NULL,
                    device_info JSONB,
                    ip_address INET,
                    user_agent TEXT,
                    expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
                    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                    last_accessed_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                    is_active BOOLEAN DEFAULT true
                )
                "#
            )
            .execute(&pool)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create sessions table: {}", e))?;

            Ok(Self { pool })
        }
    }
}

// MongoDB integration example
#[cfg(feature = "mongodb")]
mod mongodb_integration {
    use super::*;
    use mongodb::{Client, Database, Collection};
    use mongodb::bson::{doc, Document};
    use serde_json::Value;
    use chrono::{DateTime, Utc};

    /// MongoDB user database implementation
    pub struct MongoUserRepository {
        collection: Collection<Document>,
    }

    impl MongoUserRepository {
        pub async fn new(database_url: &str, database_name: &str) -> Result<Self> {
            let client = Client::with_uri_str(database_url).await
                .map_err(|e| anyhow::anyhow!("Failed to connect to MongoDB: {}", e))?;

            let database = client.database(database_name);
            let collection = database.collection("users");

            // Create indexes
            collection.create_index(
                doc! { "email": 1 },
                None
            ).await
            .map_err(|e| anyhow::anyhow!("Failed to create email index: {}", e))?;

            collection.create_index(
                doc! { "deleted_at": 1 },
                None
            ).await
            .map_err(|e| anyhow::anyhow!("Failed to create deleted_at index: {}", e))?;

            Ok(Self { collection })
        }

        pub async fn create_sample_data(&self) -> Result<()> {
            let user_id = Uuid::new_v4();
            let user_doc = doc! {
                "_id": user_id,
                "email": "admin@startapp.id",
                "password_hash": "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$RdescudvJCsgt3ub+b+dWRWJTmaaJObG",
                "roles": ["admin"],
                "is_active": true,
                "is_locked": false,
                "two_factor_enabled": true,
                "two_factor_methods": ["totp"],
                "created_at": Utc::now(),
                "updated_at": Utc::now()
            };

            self.collection.insert_one(user_doc, None).await
                .map_err(|e| anyhow::anyhow!("Failed to create sample data: {}", e))?;

            println!("✅ Sample MongoDB user data created");
            Ok(())
        }

        fn bson_to_user(doc: &Document) -> Result<User> {
            Ok(User {
                id: doc.get_uuid("_id")
                    .map_err(|e| anyhow::anyhow!("Failed to get user ID: {}", e))?,
                email: doc.get_str("email")
                    .map_err(|e| anyhow::anyhow!("Failed to get email: {}", e))?
                    .to_string(),
                password_hash: doc.get_str("password_hash")
                    .map_err(|e| anyhow::anyhow!("Failed to get password hash: {}", e))?
                    .to_string(),
                roles: doc.get_array("roles")
                    .unwrap_or(&mongodb::bson::Array::new())
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect(),
                is_active: doc.get_bool("is_active").unwrap_or(true),
                is_locked: doc.get_bool("is_locked").unwrap_or(false),
                two_factor_enabled: doc.get_bool("two_factor_enabled").unwrap_or(false),
                two_factor_methods: doc.get_array("two_factor_methods")
                    .unwrap_or(&mongodb::bson::Array::new())
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect(),
                account_expires_at: doc.get_datetime("account_expires_at").ok().map(|dt| dt.to_chrono()),
                requires_password_change: doc.get_bool("requires_password_change").unwrap_or(false),
            })
        }
    }

    #[async_trait]
    impl UserRepository for MongoUserRepository {
        async fn find_by_email(&self, email: &str) -> Result<Option<User>> {
            let filter = doc! {
                "email": email,
                "deleted_at": { "$exists": false }
            };

            if let Some(doc) = self.collection.find_one(filter, None).await
                .map_err(|e| anyhow::anyhow!("Database error: {}", e))? {
                Ok(Some(Self::bson_to_user(&doc)?))
            } else {
                Ok(None)
            }
        }

        async fn save(&self, user: &User) -> Result<()> {
            let doc = doc! {
                "_id": user.id,
                "email": &user.email,
                "password_hash": &user.password_hash,
                "roles": &user.roles,
                "is_active": user.is_active,
                "is_locked": user.is_locked,
                "two_factor_enabled": user.two_factor_enabled,
                "two_factor_methods": &user.two_factor_methods,
                "account_expires_at": user.account_expires_at,
                "requires_password_change": user.requires_password_change,
                "created_at": Utc::now(),
                "updated_at": Utc::now()
            };

            self.collection.insert_one(doc, None).await
                .map_err(|e| anyhow::anyhow!("Failed to save user: {}", e))?;

            Ok(())
        }

        async fn update(&self, user: &User) -> Result<()> {
            let filter = doc! { "_id": user.id };
            let update = doc! {
                "$set": {
                    "email": &user.email,
                    "password_hash": &user.password_hash,
                    "roles": &user.roles,
                    "is_active": user.is_active,
                    "is_locked": user.is_locked,
                    "two_factor_enabled": user.two_factor_enabled,
                    "two_factor_methods": &user.two_factor_methods,
                    "account_expires_at": user.account_expires_at,
                    "requires_password_change": user.requires_password_change,
                    "updated_at": Utc::now()
                }
            };

            self.collection.update_one(filter, update, None).await
                .map_err(|e| anyhow::anyhow!("Failed to update user: {}", e))?;

            Ok(())
        }

        async fn delete(&self, user_id: &Uuid) -> Result<()> {
            let filter = doc! { "_id": user_id };
            let update = doc! {
                "$set": {
                    "deleted_at": Utc::now(),
                    "updated_at": Utc::now()
                }
            };

            self.collection.update_one(filter, update, None).await
                .map_err(|e| anyhow::anyhow!("Failed to delete user: {}", e))?;

            Ok(())
        }

        async fn find_by_id(&self, user_id: &Uuid) -> Result<Option<User>> {
            let filter = doc! {
                "_id": user_id,
                "deleted_at": { "$exists": false }
            };

            if let Some(doc) = self.collection.find_one(filter, None).await
                .map_err(|e| anyhow::anyhow!("Database error: {}", e))? {
                Ok(Some(Self::bson_to_user(&doc)?))
            } else {
                Ok(None)
            }
        }
    }
}

// Mock database implementations for demonstration
struct MockPostgresUserRepository {
    users: HashMap<String, User>,
}

impl MockPostgresUserRepository {
    fn new() -> Self {
        let mut users = HashMap::new();
        let user_id = Uuid::new_v4();

        users.insert("admin@startapp.id".to_string(), User {
            id: user_id,
            email: "admin@startapp.id".to_string(),
            password_hash: "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$RdescudvJCsgt3ub+b+dWRWJTmaaJObG".to_string(),
            roles: vec!["admin".to_string()],
            is_active: true,
            is_locked: false,
            two_factor_enabled: true,
            two_factor_methods: vec!["totp".to_string()],
            account_expires_at: None,
            requires_password_change: false,
        });

        Self { users }
    }
}

#[async_trait]
impl UserRepository for MockPostgresUserRepository {
    async fn find_by_email(&self, email: &str) -> Result<Option<User>> {
        println!("🐘 PostgreSQL: Querying user by email: {}", email);
        Ok(self.users.get(email).cloned())
    }

    async fn save(&self, user: &User) -> Result<()> {
        println!("🐘 PostgreSQL: INSERT INTO users VALUES ({}, {}, ...)", user.id, user.email);
        Ok(())
    }

    async fn update(&self, user: &User) -> Result<()> {
        println!("🐘 PostgreSQL: UPDATE users SET ... WHERE id = {}", user.id);
        Ok(())
    }

    async fn delete(&self, user_id: &Uuid) -> Result<()> {
        println!("🐘 PostgreSQL: UPDATE users SET deleted_at = NOW() WHERE id = {}", user_id);
        Ok(())
    }

    async fn find_by_id(&self, user_id: &Uuid) -> Result<Option<User>> {
        println!("🐘 PostgreSQL: SELECT * FROM users WHERE id = {}", user_id);
        Ok(None)
    }
}

struct MockMongoUserRepository {
    users: HashMap<String, User>,
}

impl MockMongoUserRepository {
    fn new() -> Self {
        let mut users = HashMap::new();
        let user_id = Uuid::new_v4();

        users.insert("admin@startapp.id".to_string(), User {
            id: user_id,
            email: "admin@startapp.id".to_string(),
            password_hash: "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$RdescudvJCsgt3ub+b+dWRWJTmaaJObG".to_string(),
            roles: vec!["admin".to_string()],
            is_active: true,
            is_locked: false,
            two_factor_enabled: true,
            two_factor_methods: vec!["totp".to_string()],
            account_expires_at: None,
            requires_password_change: false,
        });

        Self { users }
    }
}

#[async_trait]
impl UserRepository for MockMongoUserRepository {
    async fn find_by_email(&self, email: &str) -> Result<Option<User>> {
        println!("🍃 MongoDB: db.users.findOne({{ email: '{}' }})", email);
        Ok(self.users.get(email).cloned())
    }

    async fn save(&self, user: &User) -> Result<()> {
        println!("🍃 MongoDB: db.users.insertOne({{ email: '{}', _id: {} }})", user.email, user.id);
        Ok(())
    }

    async fn update(&self, user: &User) -> Result<()> {
        println!("🍃 MongoDB: db.users.updateOne({{ _id: {} }}, {{ $set: {{ email: '{}' }} }})", user.id, user.email);
        Ok(())
    }

    async fn delete(&self, user_id: &Uuid) -> Result<()> {
        println!("🍃 MongoDB: db.users.updateOne({{ _id: {} }}, {{ $set: {{ deleted_at: new Date() }} }})", user_id);
        Ok(())
    }

    async fn find_by_id(&self, user_id: &Uuid) -> Result<Option<User>> {
        println!("🍃 MongoDB: db.users.findOne({{ _id: {} }})", user_id);
        Ok(None)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Backbone Auth Database Integration Examples ===\n");

    // Initialize auth service
    let auth_service = AuthService::new(AuthServiceConfig {
        jwt_secret: "database_integration_secret_change_in_production".to_string(),
        token_expiry_hours: 24,
        ..Default::default()
    })?;

    // Initialize mock security service
    struct MockSecurityService;
    #[async_trait::async_trait]
    impl SecurityService for MockSecurityService {
        async fn check_rate_limit(&self, _email: &str, _ip_address: Option<&str>) -> Result<()> { Ok(()) }
        async fn analyze_login_attempt(&self, _user_id: &Uuid, _device_info: &DeviceInfo, _ip_address: Option<&str>) -> Result<SecurityFlags> {
            Ok(SecurityFlags {
                new_device: false,
                suspicious_location: false,
                brute_force_detected: false,
                anomaly_detected: false,
            })
        }
        async fn log_failed_auth_attempt(&self, _user_id: &Uuid, _ip_address: Option<&str>) -> Result<()> { Ok(()) }
        async fn log_successful_auth(&self, _user_id: &Uuid, _ip_address: Option<&str>) -> Result<()> { Ok(()) }
    }

    let security_service = MockSecurityService;

    // 1. PostgreSQL Integration Example
    println!("🐘 PostgreSQL Integration Example");
    println!("================================");
    let postgres_repo = MockPostgresUserRepository::new();

    let auth_request = AuthRequest {
        email: "admin@startapp.id".to_string(),
        password: "SecureAdminPass123".to_string(),
        ip_address: Some("192.168.1.100".to_string()),
        device_info: DeviceInfo {
            device_id: "postgres_client_123".to_string(),
            user_agent: "PostgreSQL-Client/1.0".to_string(),
            ip_address: Some("192.168.1.100".to_string()),
            fingerprint: Some("fp_postgres_client".to_string()),
        },
        remember_me: Some(true),
    };

    match auth_service.authenticate_enhanced(
        auth_request,
        &postgres_repo,
        &security_service
    ).await {
        Ok(result) => {
            println!("✅ PostgreSQL authentication successful!");
            println!("   User ID: {}", result.user_id);
            println!("   Token generated: {}", result.token.is_some());
        }
        Err(e) => println!("❌ PostgreSQL authentication failed: {}", e),
    }
    println!();

    // 2. MongoDB Integration Example
    println!("🍃 MongoDB Integration Example");
    println!("==============================");
    let mongo_repo = MockMongoUserRepository::new();

    let auth_request = AuthRequest {
        email: "admin@startapp.id".to_string(),
        password: "SecureAdminPass123".to_string(),
        ip_address: Some("192.168.1.101".to_string()),
        device_info: DeviceInfo {
            device_id: "mongo_client_456".to_string(),
            user_agent: "MongoDB-Client/1.0".to_string(),
            ip_address: Some("192.168.1.101".to_string()),
            fingerprint: Some("fp_mongo_client".to_string()),
        },
        remember_me: Some(true),
    };

    match auth_service.authenticate_enhanced(
        auth_request,
        &mongo_repo,
        &security_service
    ).await {
        Ok(result) => {
            println!("✅ MongoDB authentication successful!");
            println!("   User ID: {}", result.user_id);
            println!("   Token generated: {}", result.token.is_some());
        }
        Err(e) => println!("❌ MongoDB authentication failed: {}", e),
    }
    println!();

    // 3. Database Configuration Examples
    println!("⚙️ Database Configuration Examples");
    println!("==================================");

    println!("📋 PostgreSQL Configuration:");
    println!("```toml");
    println!("# Cargo.toml");
    println!("sqlx = {{ version = \"0.8\", features = [\"runtime-tokio-rustls\", \"postgres\", \"uuid\", \"chrono\"] }}");
    println!("postgres-configuration = \"0.1\"");
    println!("```");
    println!();

    println!("📋 PostgreSQL Connection String:");
    println!("```env");
    println!("DATABASE_URL=postgresql://username:password@localhost:5432/auth_db");
    println!("DATABASE_MAX_CONNECTIONS=20");
    println!("DATABASE_MIN_CONNECTIONS=5");
    println!("```");
    println!();

    println!("📋 MongoDB Configuration:");
    println!("```toml");
    println!("# Cargo.toml");
    println!("mongodb = \"2.8\"");
    println!("bson = \"2.8\"");
    println!("```");
    println!();

    println!("📋 MongoDB Connection String:");
    println!("```env");
    println!("MONGODB_URL=mongodb://username:password@localhost:27017/auth_db");
    println!("MONGODB_DATABASE=auth_db");
    println!("```");
    println!();

    // 4. Migration Examples
    println!("🗄️ Database Migration Examples");
    println!("===============================");

    println!("🐘 PostgreSQL Migration (migrations/001_create_users.sql):");
    println!("```sql");
    println!("CREATE EXTENSION IF NOT EXISTS \"uuid-ossp\";");
    println!();
    println!("CREATE TABLE users (");
    println!("    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),");
    println!("    email VARCHAR(255) UNIQUE NOT NULL,");
    println!("    password_hash VARCHAR(255) NOT NULL,");
    println!("    roles TEXT[] DEFAULT '{}',");
    println!("    is_active BOOLEAN DEFAULT true,");
    println!("    is_locked BOOLEAN DEFAULT false,");
    println!("    two_factor_enabled BOOLEAN DEFAULT false,");
    println!("    two_factor_methods TEXT[] DEFAULT '{}',");
    println!("    account_expires_at TIMESTAMP WITH TIME ZONE,");
    println!("    requires_password_change BOOLEAN DEFAULT false,");
    println!("    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),");
    println!("    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),");
    println!("    deleted_at TIMESTAMP WITH TIME ZONE");
    println!(");");
    println!();
    println!("CREATE INDEX idx_users_email ON users(email);");
    println!("CREATE INDEX idx_users_deleted_at ON users(deleted_at);");
    println!("```");
    println!();

    println!("🍃 MongoDB Collection Setup:");
    println!("```javascript");
    println!("db.createCollection(\"users\");");
    println!();
    println!("// Create indexes");
    println!("db.users.createIndex({ \"email\": 1 }, { unique: true });");
    println!("db.users.createIndex({ \"deleted_at\": 1 });");
    println!("db.users.createIndex({ \"created_at\": -1 });");
    println!();
    println!("// Sample document");
    println!("db.users.insertOne({");
    println!("  email: \"admin@startapp.id\",");
    println!("  passwordHash: \"$argon2id$v=19$m=19456,t=2,p=1$...\",");
    println!("  roles: [\"admin\"],");
    println!("  isActive: true,");
    println!("  isLocked: false,");
    println!("  twoFactorEnabled: true,");
    println!("  twoFactorMethods: [\"totp\"],");
    println!("  createdAt: new Date()");
    println!("});");
    println!("```");
    println!();

    // 5. Performance Comparison
    println!("📊 Performance Comparison");
    println!("=========================");

    println!("🐘 PostgreSQL Strengths:");
    println!("✅ ACID compliance and data integrity");
    println!("✅ Complex queries with JOINs");
    println!("✅ Referential integrity with foreign keys");
    println!("✅ Advanced indexing strategies");
    println!("✅ Window functions and CTEs");
    println!("✅ Mature tooling and ecosystem");
    println!("✅ Better for transactional data");
    println!();

    println!("🍃 MongoDB Strengths:");
    println!("✅ Flexible schema design");
    println!("✅ Horizontal scaling with sharding");
    println!("✅ Natural document mapping to Rust structs");
    println!("✅ Built-in replication and high availability");
    println!("✅ Aggregation pipeline for complex queries");
    println!("✅ Better for rapidly evolving schemas");
    println!("✅ TTL indexes for automatic expiration");
    println!();

    println!("📋 Recommendation:");
    println!("🎯 Use PostgreSQL for:");
    println!("   • User authentication (primary choice)");
    println!("   • Financial transactions");
    println!("   • Data requiring strong consistency");
    println!("   • Complex relational queries");
    println!();
    println!("🎯 Use MongoDB for:");
    println!("   • Session storage");
    println!("   • Audit logs");
    println!("   • User preferences");
    println!("   • Rapidly evolving data structures");
    println!();

    // 6. Connection Pooling Best Practices
    println!("🔗 Connection Pooling Best Practices");
    println!("=====================================");

    println!("🐘 PostgreSQL Pool Configuration:");
    println!("```rust");
    println!("let pool = PgPoolOptions::new()");
    println!("    .max_connections(20)");
    println!("    .min_connections(5)");
    println!("    .acquire_timeout(Duration::from_secs(30))");
    println!("    .idle_timeout(Duration::from_secs(600))");
    println!("    .max_lifetime(Duration::from_secs(1800))");
    println!("    .connect(&database_url)");
    println!("    .await?;");
    println!("```");
    println!();

    println!("🍃 MongoDB Pool Configuration:");
    println!("```rust");
    println!("let client_options = ClientOptions::builder()");
    println!("    .max_pool_size(20)");
    println!("    .min_pool_size(5)");
    println!("    .max_idle_time(Duration::from_secs(600))");
    println!("    .server_selection_timeout(Duration::from_secs(30))");
    println!("    .build()?;");
    println!();
    println!("let client = Client::with_options(client_options)?;");
    println!("let database = client.database(\"auth_db\");");
    println!("```");
    println!();

    // 7. Database Security Best Practices
    println!("🔒 Database Security Best Practices");
    println!("===================================");

    println!("🛡️ PostgreSQL Security:");
    println!("• Use SSL/TLS connections");
    println!("• Implement row-level security (RLS)");
    println!("• Use database roles and permissions");
    println!("• Enable audit logging");
    println!("• Regular security updates");
    println!("• Connection encryption with certificate validation");
    println!();

    println!("🛡️ MongoDB Security:");
    println!("• Enable authentication and authorization");
    println!("• Use SSL/TLS connections");
    println!("• Implement role-based access control");
    println!("• Enable audit logging");
    println!("• Use field-level encryption for sensitive data");
    println!("• Network security with firewall rules");
    println!();

    // 8. Monitoring and Observability
    println!("📈 Monitoring and Observability");
    println!("===============================");

    println!("🐘 PostgreSQL Monitoring:");
    println!("• Connection pool metrics");
    println!("• Query performance with pg_stat_statements");
    println!("• Lock monitoring and deadlock detection");
    println!("• Replication lag monitoring");
    println!("• Database size and growth trends");
    println!();

    println!("🍃 MongoDB Monitoring:");
    println!("• Connection pool status");
    println!("• Query performance with profiler");
    println!("• Replication lag and oplog metrics");
    println!("• Index usage statistics");
    println!("• Disk space and memory usage");
    println!();

    println!("=== Database Integration Examples Complete ===");
    println!("🎉 Both PostgreSQL and MongoDB integration patterns demonstrated!");

    println!("\n📚 Next Steps:");
    println!("1. Choose database based on your requirements (PostgreSQL recommended)");
    println!("2. Set up proper connection pooling");
    println!("3. Implement database migrations");
    println!("4. Add comprehensive error handling");
    println!("5. Set up monitoring and alerting");
    println!("6. Plan for backup and disaster recovery");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_postgres_repository_operations() -> Result<()> {
        let repo = MockPostgresUserRepository::new();
        let user_id = Uuid::new_v4();

        let user = User {
            id: user_id,
            email: "test@example.com".to_string(),
            password_hash: "test_hash".to_string(),
            roles: vec!["user".to_string()],
            is_active: true,
            is_locked: false,
            two_factor_enabled: false,
            two_factor_methods: vec![],
            account_expires_at: None,
            requires_password_change: false,
        };

        // Test save
        repo.save(&user).await?;

        // Test find by email
        let found_user = repo.find_by_email("test@example.com").await?;
        assert!(found_user.is_some());

        // Test update
        repo.update(&user).await?;

        // Test delete
        repo.delete(&user_id).await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_mongo_repository_operations() -> Result<()> {
        let repo = MockMongoUserRepository::new();
        let user_id = Uuid::new_v4();

        let user = User {
            id: user_id,
            email: "test@example.com".to_string(),
            password_hash: "test_hash".to_string(),
            roles: vec!["user".to_string()],
            is_active: true,
            is_locked: false,
            two_factor_enabled: false,
            two_factor_methods: vec![],
            account_expires_at: None,
            requires_password_change: false,
        };

        // Test save
        repo.save(&user).await?;

        // Test find by email
        let found_user = repo.find_by_email("test@example.com").await?;
        assert!(found_user.is_some());

        // Test update
        repo.update(&user).await?;

        // Test delete
        repo.delete(&user_id).await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_database_authentication_flow() -> Result<()> {
        let auth_service = AuthService::with_secret("test_secret");
        let postgres_repo = MockPostgresUserRepository::new();
        let mongo_repo = MockMongoUserRepository::new();

        let security_service = MockSecurityService;
        let auth_request = AuthRequest {
            email: "admin@startapp.id".to_string(),
            password: "SecureAdminPass123".to_string(),
            ip_address: Some("127.0.0.1".to_string()),
            device_info: DeviceInfo {
                device_id: "test_device".to_string(),
                user_agent: "Test-Agent/1.0".to_string(),
                ip_address: Some("127.0.0.1".to_string()),
                fingerprint: Some("test_fingerprint".to_string()),
            },
            remember_me: Some(false),
        };

        // Test PostgreSQL authentication
        let pg_result = auth_service.authenticate_enhanced(
            auth_request.clone(),
            &postgres_repo,
            &security_service
        ).await?;
        assert!(!pg_result.user_id.to_string().is_empty());

        // Test MongoDB authentication
        let mongo_result = auth_service.authenticate_enhanced(
            auth_request,
            &mongo_repo,
            &security_service
        ).await?;
        assert!(!mongo_result.user_id.to_string().is_empty());

        Ok(())
    }
}