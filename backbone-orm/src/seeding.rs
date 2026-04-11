//! Database seeding system for test data and initial data setup

use anyhow::Result;
use sqlx::{PgPool, Row};
use std::fs;
use std::path::Path;
use colored::*;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// Seed manager for handling database seeding operations
pub struct SeedManager {
    pool: PgPool,
}

impl SeedManager {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get access to the database pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Run all pending seeds
    pub async fn seed(&self) -> Result<()> {
        println!("🌱 Running database seeds...");

        // Create seeds table if it doesn't exist
        self.create_seeds_table().await?;

        // Get all seed files
        let seeds = self.load_seed_files()?;

        // Get applied seeds from database
        let applied_seeds = self.get_applied_seeds().await?;

        // Run pending seeds
        for seed in seeds {
            if !applied_seeds.contains(&seed.name) {
                println!("  ↳ Applying seed: {}", seed.name.bright_green());
                self.apply_seed(&seed).await?;
            } else {
                println!("  ✓ Skipping already applied: {}", seed.name);
            }
        }

        println!("✅ Seeding completed successfully");
        Ok(())
    }

    /// Run a specific seed by name
    pub async fn seed_by_name(&self, seed_name: &str) -> Result<()> {
        println!("🌱 Running specific seed: {}", seed_name.bright_cyan());

        // Create seeds table if it doesn't exist
        self.create_seeds_table().await?;

        // Find and run the specific seed
        let seeds = self.load_seed_files()?;
        let seed = seeds.iter().find(|s| s.name == seed_name);

        match seed {
            Some(seed) => {
                println!("  ↳ Applying seed: {}", seed.name.bright_green());
                self.apply_seed(seed).await?;
                println!("✅ Seed '{}' completed successfully", seed_name);
            }
            None => {
                return Err(anyhow::anyhow!("Seed '{}' not found", seed_name));
            }
        }

        Ok(())
    }

    /// Revert all applied seeds (in reverse order)
    pub async fn revert_seeds(&self) -> Result<()> {
        println!("🔄 Reverting all seeds...");

        // Create seeds table if it doesn't exist
        self.create_seeds_table().await?;

        // Get applied seeds in reverse order
        let applied_seeds = self.get_applied_seeds_reversed().await?;

        if applied_seeds.is_empty() {
            println!("  ℹ️  No seeds to revert");
            return Ok(());
        }

        for seed_name in applied_seeds {
            println!("  ↳ Reverting seed: {}", seed_name.bright_yellow());

            // Find and execute the revert seed
            let revert_seed = self.find_revert_seed(&seed_name)?;

            if let Some(revert_content) = revert_seed {
                self.revert_seed(&seed_name, &revert_content).await?;
                println!("  ✓ Successfully reverted: {}", seed_name.bright_green());
            } else {
                println!("  ⚠️  No revert seed found for: {}", seed_name.yellow());
            }
        }

        println!("✅ Seed reversion completed");
        Ok(())
    }

    /// Create a new seed file
    pub async fn create_seed(&self, name: &str, seed_type: SeedType) -> Result<()> {
        println!("📝 Creating new seed: {}", name.bright_cyan());

        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let seed_name = format!("{}_{}.sql", timestamp, name);
        let seeds_dir = "seeds";

        // Create seeds directory if it doesn't exist
        fs::create_dir_all(seeds_dir)?;

        let content = match seed_type {
            SeedType::Data => self.generate_data_seed_template(name),
            SeedType::Test => self.generate_test_seed_template(name),
            SeedType::Reference => self.generate_reference_seed_template(name),
        };

        let seed_path = Path::new(seeds_dir).join(&seed_name);
        fs::write(&seed_path, content)?;

        println!("  ✓ Created: {}", seed_path.display().to_string().bright_green());

        // Also create revert file
        let revert_name = format!("{}_{}_revert.sql", timestamp, name);
        let revert_content = format!(
            r#"-- Revert Seed: {name}
-- Created: {timestamp}
-- Type: {seed_type:?}

-- Add your revert SQL here
-- Example:
-- DELETE FROM users WHERE email LIKE '%@test.local';
"#,
            name = name,
            timestamp = timestamp,
            seed_type = seed_type
        );

        let revert_path = Path::new(seeds_dir).join(&revert_name);
        fs::write(&revert_path, revert_content)?;

        println!("  ✓ Created: {}", revert_path.display().to_string().bright_green());

        Ok(())
    }

    /// Get seed execution history
    pub async fn history(&self) -> Result<Vec<SeedRecord>> {
        // Create seeds table if it doesn't exist
        self.create_seeds_table().await?;

        let rows = sqlx::query("SELECT id, name, seed_type, applied_at FROM schema_seeds ORDER BY applied_at DESC")
            .fetch_all(&self.pool)
            .await?;

        let records: Result<Vec<SeedRecord>, sqlx::Error> = rows.into_iter()
            .map(|row| {
                Ok(SeedRecord {
                    id: row.get("id"),
                    name: row.get("name"),
                    seed_type: row.get("seed_type"),
                    applied_at: row.get("applied_at"),
                })
            })
            .collect();

        match records {
            Ok(history) => {
                if history.is_empty() {
                    println!("  ℹ️  No seeds have been applied yet");
                } else {
                    println!("📋 Seed History:");
                    for (i, record) in history.iter().enumerate() {
                        let status = if i == 0 {
                            "🔹".to_string() // Current (last) seed
                        } else {
                            "  ".to_string()
                        };
                        println!("  {} {} [{}] - Applied at {}",
                            status,
                            record.name.bright_cyan(),
                            record.seed_type.bright_yellow(),
                            record.applied_at.format("%Y-%m-%d %H:%M:%S")
                        );
                    }
                }
                Ok(history)
            }
            Err(e) => Err(anyhow::anyhow!("Failed to load seed history: {}", e))
        }
    }

    /// Check seed status
    pub async fn status(&self) -> Result<SeedStatus> {
        // Create seeds table if it doesn't exist
        self.create_seeds_table().await?;

        // Get seed files
        let seed_files = self.load_seed_files()?;

        // Get applied seeds
        let applied_seeds = self.get_applied_seeds().await?;

        // Determine pending and applied seeds
        let mut pending_seeds = Vec::new();
        let mut applied_seed_details = Vec::new();

        for seed_file in seed_files {
            if applied_seeds.contains(&seed_file.name) {
                applied_seed_details.push(seed_file.name);
            } else {
                pending_seeds.push(seed_file.name);
            }
        }

        let status = SeedStatus {
            total_seeds: applied_seed_details.len() + pending_seeds.len(),
            applied_seeds: applied_seed_details,
            pending_seeds,
            last_seed: applied_seeds.last().cloned(),
        };

        // Print status
        println!("📊 Seed Status:");
        println!("  Total seeds: {}", status.total_seeds);
        println!("  Applied seeds: {}", status.applied_seeds.len());
        println!("  Pending seeds: {}", status.pending_seeds.len());

        if !status.pending_seeds.is_empty() {
            println!("  Pending:");
            for seed in &status.pending_seeds {
                println!("    - {}", seed.yellow());
            }
        }

        Ok(status)
    }

    /// Load seed data from JSON file
    pub async fn load_seed_data<T>(&self, file_path: &str) -> Result<Vec<T>>
    where
        T: for<'de> Deserialize<'de> + Send,
    {
        let content = fs::read_to_string(file_path)?;
        let data: Vec<T> = serde_json::from_str(&content)?;
        Ok(data)
    }

    /// Execute bulk insert for seed data
    pub async fn bulk_insert(&self, table_name: &str, data: &[serde_json::Value]) -> Result<u64> {
        if data.is_empty() {
            return Ok(0);
        }

        let mut total_rows = 0;

        // Process in chunks to avoid memory issues
        let chunk_size = 1000;
        for chunk in data.chunks(chunk_size) {
            let sample_value = &chunk[0];
            let fields: Vec<String> = sample_value.as_object()
                .ok_or_else(|| anyhow::anyhow!("Data must be objects"))?
                .keys()
                .cloned()
                .collect();

            // Generate bulk insert SQL
            let placeholders: Vec<String> = chunk.iter()
                .enumerate()
                .map(|(i, _)| {
                    let value_placeholders: Vec<String> = (0..fields.len())
                        .map(|j| format!("${}", i * fields.len() + j + 1))
                        .collect();
                    format!("({})", value_placeholders.join(", "))
                })
                .collect();

            let query = format!(
                "INSERT INTO {} ({}) VALUES {}",
                table_name,
                fields.join(", "),
                placeholders.join(", ")
            );

            let mut query_builder = sqlx::query(&query);

            // Bind all parameters
            for item in chunk {
                for field in &fields {
                    if let Some(value) = item.get(field) {
                        if let Some(str_val) = value.as_str() {
                            query_builder = query_builder.bind(str_val);
                        } else if let Some(int_val) = value.as_i64() {
                            query_builder = query_builder.bind(int_val);
                        } else if let Some(float_val) = value.as_f64() {
                            query_builder = query_builder.bind(float_val);
                        } else if let Some(bool_val) = value.as_bool() {
                            query_builder = query_builder.bind(bool_val);
                        } else {
                            query_builder = query_builder.bind(serde_json::to_string(value)?);
                        }
                    } else {
                        query_builder = query_builder.bind::<Option<String>>(None);
                    }
                }
            }

            let result = query_builder.execute(&self.pool).await?;
            total_rows += result.rows_affected();
        }

        Ok(total_rows)
    }

    /// Create seeds table to track applied seeds
    async fn create_seeds_table(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS schema_seeds (
                id SERIAL PRIMARY KEY,
                name VARCHAR(255) NOT NULL UNIQUE,
                seed_type VARCHAR(50) NOT NULL,
                applied_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
            )
            "#
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Load seed files from filesystem
    fn load_seed_files(&self) -> Result<Vec<SeedFile>> {
        let seeds_dir = Path::new("seeds");
        let mut seeds = Vec::new();

        if seeds_dir.exists() {
            for entry in fs::read_dir(seeds_dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.extension().and_then(|s| s.to_str()) == Some("sql") &&
                   !path.file_name().unwrap().to_string_lossy().contains("_revert") {

                    let name = path.file_stem()
                        .unwrap()
                        .to_string_lossy()
                        .to_string();

                    let content = fs::read_to_string(&path)?;

                    // Determine seed type from file content or naming
                    let seed_type = self.determine_seed_type(&name, &content);

                    seeds.push(SeedFile {
                        name: name.clone(),
                        content,
                        seed_type,
                    });
                }
            }
        }

        seeds.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(seeds)
    }

    /// Determine seed type from name or content
    fn determine_seed_type(&self, name: &str, content: &str) -> String {
        if name.to_lowercase().contains("test") || content.to_lowercase().contains("test") {
            "test".to_string()
        } else if name.to_lowercase().contains("ref") || content.to_lowercase().contains("reference") {
            "reference".to_string()
        } else {
            "data".to_string()
        }
    }

    /// Get list of applied seeds from database
    async fn get_applied_seeds(&self) -> Result<Vec<String>> {
        let rows = sqlx::query("SELECT name FROM schema_seeds ORDER BY name")
            .fetch_all(&self.pool)
            .await?;

        let seeds: Vec<String> = rows
            .into_iter()
            .map(|row| row.get::<String, _>("name"))
            .collect();

        Ok(seeds)
    }

    /// Get applied seeds in reverse order for reversion
    async fn get_applied_seeds_reversed(&self) -> Result<Vec<String>> {
        let rows = sqlx::query("SELECT name FROM schema_seeds ORDER BY applied_at DESC")
            .fetch_all(&self.pool)
            .await?;

        let seeds: Vec<String> = rows
            .into_iter()
            .map(|row| row.get::<String, _>("name"))
            .collect();

        Ok(seeds)
    }

    /// Apply a single seed
    async fn apply_seed(&self, seed: &SeedFile) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        // Execute seed SQL
        sqlx::query(&seed.content)
            .execute(&mut *tx)
            .await?;

        // Record seed as applied
        sqlx::query("INSERT INTO schema_seeds (name, seed_type) VALUES ($1, $2)")
            .bind(&seed.name)
            .bind(&seed.seed_type)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(())
    }

    /// Find the corresponding revert seed file
    fn find_revert_seed(&self, seed_name: &str) -> Result<Option<String>> {
        let seeds_dir = Path::new("seeds");

        // Look for the revert seed file
        let revert_name = format!("{}_revert", seed_name);

        if seeds_dir.exists() {
            for entry in fs::read_dir(seeds_dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.extension().and_then(|s| s.to_str()) == Some("sql") {
                    let file_stem = path.file_stem()
                        .unwrap()
                        .to_string_lossy()
                        .to_string();

                    if file_stem == revert_name {
                        let content = fs::read_to_string(&path)?;
                        return Ok(Some(content));
                    }
                }
            }
        }

        Ok(None)
    }

    /// Revert a specific seed
    async fn revert_seed(&self, seed_name: &str, revert_content: &str) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        // Execute the revert seed SQL
        sqlx::query(revert_content)
            .execute(&mut *tx)
            .await?;

        // Remove seed record from database
        sqlx::query("DELETE FROM schema_seeds WHERE name = $1")
            .bind(seed_name)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(())
    }

    /// Generate data seed template
    fn generate_data_seed_template(&self, name: &str) -> String {
        format!(
            r#"-- Data Seed: {name}
-- Type: Data
-- Description: Initial data seed for production/staging

-- Insert initial data here
-- Example:
-- INSERT INTO users (id, name, email, created_at, updated_at) VALUES
-- ('admin-id', 'System Administrator', 'admin@company.com', NOW(), NOW()),
-- ('user-id', 'Test User', 'user@company.com', NOW(), NOW());

-- You can also use INSERT with multiple rows:
-- INSERT INTO categories (id, name, description) VALUES
-- ('cat1', 'Electronics', 'Electronic devices and accessories'),
-- ('cat2', 'Books', 'Books and educational materials'),
-- ('cat3', 'Clothing', 'Apparel and fashion items');

-- For complex data, consider using JSON:
-- INSERT INTO settings (key, value) VALUES
-- ('app_config', '{{"theme": "dark", "language": "en", "timezone": "UTC"}}'),
-- ('feature_flags', '{{"new_ui": true, "beta_features": false, "analytics": true}}');
"#,
            name = name
        )
    }

    /// Generate test seed template
    fn generate_test_seed_template(&self, name: &str) -> String {
        format!(
            r#"-- Test Seed: {name}
-- Type: Test
-- Description: Test data for development and testing

-- Insert test data here
-- Use identifiable test data with consistent patterns

-- Example test users
-- INSERT INTO users (id, name, email, created_at, updated_at) VALUES
-- ('test-user-1', 'Test User 1', 'user1@test.local', NOW(), NOW()),
-- ('test-user-2', 'Test User 2', 'user2@test.local', NOW(), NOW()),
-- ('test-admin', 'Test Admin', 'admin@test.local', NOW(), NOW());

-- Test categories
-- INSERT INTO categories (id, name, description) VALUES
-- ('test-cat-1', 'Test Category 1', 'Test category for testing'),
-- ('test-cat-2', 'Test Category 2', 'Another test category');

-- Test products
-- INSERT INTO products (id, name, price, category_id, active, created_at, updated_at) VALUES
-- ('test-prod-1', 'Test Product 1', 99.99, 'test-cat-1', true, NOW(), NOW()),
-- ('test-prod-2', 'Test Product 2', 149.99, 'test-cat-1', false, NOW(), NOW()),
-- ('test-prod-3', 'Test Product 3', 199.99, 'test-cat-2', true, NOW(), NOW());

-- Note: Test data should use identifiable patterns like:
-- - test-local domain (@test.local)
-- - test- prefixes (test-user-1, test-cat-1, etc.)
-- - Consistent test values for reproducibility
"#,
            name = name
        )
    }

    /// Generate reference seed template
    fn generate_reference_seed_template(&self, name: &str) -> String {
        format!(
            r#"-- Reference Seed: {name}
-- Type: Reference
-- Description: Reference data and lookup tables

-- Insert reference data here
-- This is typically static data that applications depend on

-- Example reference data
-- INSERT INTO countries (id, code, name, iso3, currency) VALUES
-- ('US', 'US', 'United States', 'USA', 'USD'),
-- ('CA', 'CA', 'Canada', 'CAN', 'CAD'),
-- ('GB', 'GB', 'United Kingdom', 'GBR', 'GBP');

-- INSERT INTO languages (id, code, name, is_active) VALUES
-- ('en', 'en', 'English', true),
-- ('es', 'es', 'Spanish', true),
-- ('fr', 'fr', 'French', true),
-- ('de', 'de', 'German', true);

-- INSERT INTO user_roles (id, name, description, permissions) VALUES
-- ('admin', 'Administrator', 'Full system access', '["create", "read", "update", "delete", "admin"]'),
-- ('moderator', 'Moderator', 'Content moderation access', '["read", "update", "moderate"]'),
-- ('user', 'User', 'Standard user access', '["read", "update_profile"]');

-- INSERT INTO system_settings (id, key, value, description, is_public) VALUES
-- ('1', 'app_name', 'My Application', 'Application name', true),
-- ('2', 'app_version', '1.0.0', 'Current application version', true),
-- ('3', 'max_file_size', '10485760', 'Maximum file size in bytes', false),
-- ('4', 'session_timeout', '3600', 'Session timeout in seconds', false);

-- Reference data characteristics:
-- - Static and rarely changes
-- - Required for application functionality
-- - Often has foreign key relationships
-- - Usually loaded early in the application lifecycle
"#,
            name = name
        )
    }
}

/// Seed file representation
struct SeedFile {
    name: String,
    content: String,
    seed_type: String,
}

/// Seed execution record
#[derive(Debug, Clone)]
pub struct SeedRecord {
    pub id: i32,
    pub name: String,
    pub seed_type: String,
    pub applied_at: DateTime<Utc>,
}

/// Seed status information
#[derive(Debug, Clone)]
pub struct SeedStatus {
    pub total_seeds: usize,
    pub applied_seeds: Vec<String>,
    pub pending_seeds: Vec<String>,
    pub last_seed: Option<String>,
}

/// Seed type enumeration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SeedType {
    Data,
    Test,
    Reference,
}

/// Seed trait for programmatic seed creation
pub trait Seed {
    fn name(&self) -> &str;
    fn seed_type(&self) -> SeedType;
    fn up(&self) -> &str;
    fn down(&self) -> &str;
}