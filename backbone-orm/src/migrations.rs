//! Database migration system

use anyhow::Result;
use sqlx::{PgPool, Row};
use std::fs;
use std::path::Path;
use colored::*;
use chrono::{DateTime, Utc};

/// Migration manager
pub struct MigrationManager {
    pool: PgPool,
}

impl MigrationManager {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Run all pending migrations
    pub async fn migrate(&self) -> Result<()> {
        println!("🔄 Running database migrations...");

        // Create migrations table if it doesn't exist
        self.create_migrations_table().await?;

        // Get all migration files
        let migrations = self.load_migration_files()?;

        // Get applied migrations from database
        let applied_migrations = self.get_applied_migrations().await?;

        // Run pending migrations
        for migration in migrations {
            if !applied_migrations.contains(&migration.name) {
                println!("  ↳ Applying migration: {}", migration.name.bright_green());
                self.apply_migration(&migration).await?;
            } else {
                println!("  ✓ Skipping already applied: {}", migration.name);
            }
        }

        println!("✅ Migrations completed successfully");
        Ok(())
    }

    /// Rollback the last migration
    pub async fn rollback(&self) -> Result<()> {
        println!("🔄 Rolling back last migration...");

        // Create migrations table if it doesn't exist
        self.create_migrations_table().await?;

        // Get the last applied migration
        let last_migration = self.get_last_applied_migration().await?;

        match last_migration {
            Some(migration_name) => {
                println!("  ↳ Rolling back migration: {}", migration_name.bright_yellow());

                // Find and execute the down migration
                let down_migration = self.find_down_migration(&migration_name)?;

                if let Some(down_content) = down_migration {
                    self.rollback_migration(&migration_name, &down_content).await?;
                    println!("  ✓ Successfully rolled back: {}", migration_name.bright_green());
                } else {
                    println!("  ⚠️  No down migration found for: {}", migration_name.yellow());
                }
            }
            None => {
                println!("  ℹ️  No migrations to rollback");
            }
        }

        println!("✅ Rollback completed");
        Ok(())
    }

    /// Rollback a specific number of migrations
    pub async fn rollback_n(&self, count: usize) -> Result<()> {
        println!("🔄 Rolling back {} migrations...", count);

        for i in 0..count {
            let last_migration = self.get_last_applied_migration().await?;

            match last_migration {
                Some(migration_name) => {
                    println!("  ↳ Rolling back {}/1: {}", i + 1, migration_name.bright_yellow());

                    let down_migration = self.find_down_migration(&migration_name)?;

                    if let Some(down_content) = down_migration {
                        self.rollback_migration(&migration_name, &down_content).await?;
                        println!("  ✓ Successfully rolled back: {}", migration_name.bright_green());
                    } else {
                        println!("  ⚠️  No down migration found for: {}", migration_name.yellow());
                        break;
                    }
                }
                None => {
                    println!("  ℹ️  No more migrations to rollback");
                    break;
                }
            }
        }

        println!("✅ Rollback of {} migrations completed", count);
        Ok(())
    }

    /// Get migration history
    pub async fn history(&self) -> Result<Vec<MigrationRecord>> {
        // Create migrations table if it doesn't exist
        self.create_migrations_table().await?;

        let rows = sqlx::query("SELECT id, name, applied_at FROM schema_migrations ORDER BY applied_at DESC")
            .fetch_all(&self.pool)
            .await?;

        let records: Result<Vec<MigrationRecord>, sqlx::Error> = rows.into_iter()
            .map(|row| {
                Ok(MigrationRecord {
                    id: row.get("id"),
                    name: row.get("name"),
                    applied_at: row.get("applied_at"),
                })
            })
            .collect();

        match records {
            Ok(history) => {
                if history.is_empty() {
                    println!("  ℹ️  No migrations have been applied yet");
                } else {
                    println!("📋 Migration History:");
                    for (i, record) in history.iter().enumerate() {
                        let status = if i == 0 {
                            "🔹".to_string() // Current (last) migration
                        } else {
                            "  ".to_string()
                        };
                        println!("  {} {} - Applied at {}", status, record.name.bright_cyan(), record.applied_at.format("%Y-%m-%d %H:%M:%S"));
                    }
                }
                Ok(history)
            }
            Err(e) => Err(anyhow::anyhow!("Failed to load migration history: {}", e))
        }
    }

    /// Validate migration status
    pub async fn status(&self) -> Result<MigrationStatus> {
        // Create migrations table if it doesn't exist
        self.create_migrations_table().await?;

        // Get migration files
        let migration_files = self.load_migration_files()?;

        // Get applied migrations
        let applied_migrations = self.get_applied_migrations().await?;

        // Determine pending and applied migrations
        let mut pending_migrations = Vec::new();
        let mut applied_migration_details = Vec::new();

        for migration_file in migration_files {
            if applied_migrations.contains(&migration_file.name) {
                applied_migration_details.push(migration_file.name);
            } else {
                pending_migrations.push(migration_file.name);
            }
        }

        let status = MigrationStatus {
            total_migrations: applied_migration_details.len() + pending_migrations.len(),
            applied_migrations: applied_migration_details,
            pending_migrations,
            last_migration: applied_migrations.last().cloned(),
        };

        // Print status
        println!("📊 Migration Status:");
        println!("  Total migrations: {}", status.total_migrations);
        println!("  Applied migrations: {}", status.applied_migrations.len());
        println!("  Pending migrations: {}", status.pending_migrations.len());

        if !status.pending_migrations.is_empty() {
            println!("  Pending:");
            for migration in &status.pending_migrations {
                println!("    - {}", migration.yellow());
            }
        }

        Ok(status)
    }

    /// Get the last applied migration
    async fn get_last_applied_migration(&self) -> Result<Option<String>> {
        let row = sqlx::query("SELECT name FROM schema_migrations ORDER BY applied_at DESC LIMIT 1")
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(|r| r.get("name")))
    }

    /// Find the corresponding down migration file.
    ///
    /// Migrations live as paired siblings: `<name>.up.sql` and
    /// `<name>.down.sql`. The migration "name" stored in the
    /// `schema_migrations` table is the part before `.up.sql`, so finding
    /// the rollback is a direct path lookup.
    fn find_down_migration(&self, migration_name: &str) -> Result<Option<String>> {
        let path = Path::new("migrations").join(format!("{}.down.sql", migration_name));
        if path.exists() {
            Ok(Some(fs::read_to_string(&path)?))
        } else {
            Ok(None)
        }
    }

    /// Rollback a specific migration in a single transaction.
    ///
    /// Down migrations are multi-statement (drop indexes, drop table,
    /// drop functions, drop types) so we use `sqlx::raw_sql` rather than
    /// `sqlx::query` — the latter goes through the extended protocol and
    /// rejects compound statements.
    async fn rollback_migration(&self, migration_name: &str, down_content: &str) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        sqlx::raw_sql(down_content)
            .execute(&mut *tx)
            .await?;

        sqlx::query("DELETE FROM schema_migrations WHERE name = $1")
            .bind(migration_name)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    /// Create a new paired up/down migration: `<ts>_<name>.up.sql` and
    /// `<ts>_<name>.down.sql` under `migrations/`. Timestamp is the
    /// `YYYYMMDDHHMMSS` form so it sorts cleanly next to generator-emitted
    /// migrations.
    pub async fn create_migration(&self, name: &str) -> Result<()> {
        println!("📝 Creating new migration: {}", name.bright_cyan());

        let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S");
        let stem = format!("{}_{}", timestamp, name);
        let migrations_dir = "migrations";
        fs::create_dir_all(migrations_dir)?;

        let up_content = format!(
            r#"-- Migration: {name}
-- Created: {timestamp}

-- Add your migration SQL here
-- Example:
-- CREATE TABLE example_table (
--     id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
--     created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
--     updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
-- );
"#,
            name = name,
            timestamp = timestamp
        );
        let up_path = Path::new(migrations_dir).join(format!("{}.up.sql", stem));
        fs::write(&up_path, up_content)?;

        let down_content = format!(
            r#"-- Down Migration: {name}
-- Created: {timestamp}

-- Add your rollback SQL here
-- Example:
-- DROP TABLE IF EXISTS example_table CASCADE;
"#,
            name = name,
            timestamp = timestamp
        );
        let down_path = Path::new(migrations_dir).join(format!("{}.down.sql", stem));
        fs::write(&down_path, down_content)?;

        println!("  ✓ Created: {}", up_path.display().to_string().bright_green());
        println!("  ✓ Created: {}", down_path.display().to_string().bright_green());
        Ok(())
    }

    /// Create migrations table to track applied migrations
    async fn create_migrations_table(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS schema_migrations (
                id SERIAL PRIMARY KEY,
                name VARCHAR(255) NOT NULL UNIQUE,
                applied_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
            )
            "#
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Load up-migration files from `migrations/`, sorted by name (which
    /// for our convention also sorts chronologically because filenames are
    /// timestamp-prefixed). The "name" returned strips the `.up.sql`
    /// suffix so it matches what gets recorded in `schema_migrations`.
    fn load_migration_files(&self) -> Result<Vec<MigrationFile>> {
        let migrations_dir = Path::new("migrations");
        let mut migrations = Vec::new();

        if migrations_dir.exists() {
            for entry in fs::read_dir(migrations_dir)? {
                let entry = entry?;
                let path = entry.path();
                let Some(file_name) = path.file_name().and_then(|f| f.to_str()) else {
                    continue;
                };
                let Some(name) = file_name.strip_suffix(".up.sql") else {
                    continue;
                };
                let content = fs::read_to_string(&path)?;
                migrations.push(MigrationFile {
                    name: name.to_string(),
                    content,
                });
            }
        }

        migrations.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(migrations)
    }

    /// Get list of applied migrations from database
    async fn get_applied_migrations(&self) -> Result<Vec<String>> {
        let rows = sqlx::query("SELECT name FROM schema_migrations ORDER BY name")
            .fetch_all(&self.pool)
            .await?;

        let migrations: Vec<String> = rows
            .into_iter()
            .map(|row| row.get::<String, _>("name"))
            .collect();

        Ok(migrations)
    }

    /// Apply a single migration in a single transaction.
    ///
    /// Generated `.up.sql` files contain multiple statements (CREATE
    /// TABLE + CREATE INDEX + CREATE FUNCTION + ALTER TABLE …). Use
    /// `sqlx::raw_sql` so the simple-query protocol handles the compound
    /// SQL — `sqlx::query` only accepts a single statement.
    async fn apply_migration(&self, migration: &MigrationFile) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        sqlx::raw_sql(&migration.content)
            .execute(&mut *tx)
            .await?;

        sqlx::query("INSERT INTO schema_migrations (name) VALUES ($1)")
            .bind(&migration.name)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }
}

/// Migration file representation
struct MigrationFile {
    name: String,
    content: String,
}

/// Migration trait
pub trait Migration {
    fn name(&self) -> &str;
    fn up(&self) -> &str;
    fn down(&self) -> &str;
}

/// Migration record from database
#[derive(Debug, Clone)]
pub struct MigrationRecord {
    pub id: i32,
    pub name: String,
    pub applied_at: DateTime<Utc>,
}

/// Migration status information
#[derive(Debug, Clone)]
pub struct MigrationStatus {
    pub total_migrations: usize,
    pub applied_migrations: Vec<String>,
    pub pending_migrations: Vec<String>,
    pub last_migration: Option<String>,
}