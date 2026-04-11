//! Repository Traits - Contracts for Entity Persistence
//!
//! These traits define the contracts that entities and repositories must implement
//! to work with the generic repository implementations.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;
use std::fmt::Debug;

// ============================================================
// Error Types
// ============================================================

/// Repository error types
#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("Entity not found")]
    NotFound,

    #[error("Entity already exists: {0}")]
    AlreadyExists(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Internal error: {0}")]
    InternalError(String),
}

impl From<serde_json::Error> for RepositoryError {
    fn from(e: serde_json::Error) -> Self {
        RepositoryError::SerializationError(e.to_string())
    }
}

#[cfg(feature = "postgres")]
impl From<sqlx::Error> for RepositoryError {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::RowNotFound => RepositoryError::NotFound,
            sqlx::Error::Database(db_err) => {
                let msg = db_err.message().to_string();
                if msg.contains("duplicate key") || msg.contains("unique constraint") {
                    RepositoryError::AlreadyExists(msg)
                } else {
                    RepositoryError::DatabaseError(msg)
                }
            }
            _ => RepositoryError::DatabaseError(e.to_string()),
        }
    }
}

// ============================================================
// Entity Traits
// ============================================================

/// Trait for entities that can be persisted.
///
/// This trait defines the common fields and behavior required by all entities
/// that can be stored and retrieved from a repository.
pub trait PersistentEntity: Clone + Send + Sync + Debug + Serialize + DeserializeOwned + 'static {
    /// Get the entity's unique identifier
    fn entity_id(&self) -> String;

    /// Set the entity's unique identifier
    fn set_entity_id(&mut self, id: String);

    /// Get creation timestamp
    fn created_at(&self) -> Option<DateTime<Utc>>;

    /// Set creation timestamp
    fn set_created_at(&mut self, ts: DateTime<Utc>);

    /// Get last update timestamp
    fn updated_at(&self) -> Option<DateTime<Utc>>;

    /// Set last update timestamp
    fn set_updated_at(&mut self, ts: DateTime<Utc>);

    /// Get soft delete timestamp (None if not deleted)
    fn deleted_at(&self) -> Option<DateTime<Utc>>;

    /// Set soft delete timestamp
    fn set_deleted_at(&mut self, ts: Option<DateTime<Utc>>);

    /// Check if entity is soft-deleted
    fn is_deleted(&self) -> bool {
        self.deleted_at().is_some()
    }

    /// Mark entity as deleted (soft delete)
    fn mark_deleted(&mut self) {
        self.set_deleted_at(Some(Utc::now()));
        self.set_updated_at(Utc::now());
    }

    /// Restore a soft-deleted entity
    fn restore(&mut self) {
        self.set_deleted_at(None);
        self.set_updated_at(Utc::now());
    }

    /// Touch the entity (update timestamp)
    fn touch(&mut self) {
        self.set_updated_at(Utc::now());
    }

    /// Generate a new ID for this entity type
    fn generate_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }
}

/// Trait for entities that support partial updates via field map
pub trait PartialUpdatable: PersistentEntity {
    /// Apply partial updates from a field map
    fn apply_partial_update(&mut self, fields: &HashMap<String, serde_json::Value>) -> Result<(), RepositoryError>;
}

/// Trait for entities with version/optimistic locking
pub trait Versioned {
    fn version(&self) -> u64;
    fn set_version(&mut self, version: u64);
    fn increment_version(&mut self) {
        self.set_version(self.version() + 1);
    }
}

// ============================================================
// Repository Traits
// ============================================================

/// Core CRUD repository trait
///
/// This trait defines the basic CRUD operations that all repositories must implement.
/// It's designed to work with the `CrudService` trait from the HTTP layer.
#[async_trait]
pub trait CrudRepository<E>: Send + Sync
where
    E: PersistentEntity,
{
    /// Create a new entity
    async fn create(&self, entity: E) -> Result<E, RepositoryError>;

    /// Find entity by ID (excluding soft-deleted)
    async fn find_by_id(&self, id: &str) -> Result<Option<E>, RepositoryError>;

    /// Find entity by ID (including soft-deleted, for trash operations)
    async fn find_by_id_including_deleted(&self, id: &str) -> Result<Option<E>, RepositoryError>;

    /// Update an existing entity
    async fn update(&self, entity: E) -> Result<E, RepositoryError>;

    /// Soft delete an entity
    async fn soft_delete(&self, id: &str) -> Result<bool, RepositoryError>;

    /// Restore a soft-deleted entity
    async fn restore(&self, id: &str) -> Result<Option<E>, RepositoryError>;

    /// Permanently delete an entity
    async fn hard_delete(&self, id: &str) -> Result<bool, RepositoryError>;

    /// List entities with pagination (excluding soft-deleted)
    async fn list(&self, page: u32, limit: u32) -> Result<(Vec<E>, u64), RepositoryError>;

    /// List soft-deleted entities with pagination
    async fn list_deleted(&self, page: u32, limit: u32) -> Result<(Vec<E>, u64), RepositoryError>;

    /// Count all entities (excluding soft-deleted)
    async fn count(&self) -> Result<u64, RepositoryError>;

    /// Count soft-deleted entities
    async fn count_deleted(&self) -> Result<u64, RepositoryError>;

    /// Bulk create entities
    async fn bulk_create(&self, entities: Vec<E>) -> Result<Vec<E>, RepositoryError>;

    /// Permanently delete all soft-deleted entities
    async fn empty_trash(&self) -> Result<u64, RepositoryError>;

    /// List entities with pagination and filters (excluding soft-deleted)
    ///
    /// Default implementation ignores filters and delegates to `list()`.
    /// Override in repository implementations that support filter pushdown.
    async fn list_filtered(
        &self,
        page: u32,
        limit: u32,
        filters: HashMap<String, String>,
    ) -> Result<(Vec<E>, u64), RepositoryError> {
        let _ = filters; // ignored by default
        self.list(page, limit).await
    }

    /// Check if an entity exists by ID
    async fn exists(&self, id: &str) -> Result<bool, RepositoryError> {
        Ok(self.find_by_id(id).await?.is_some())
    }
}

/// Extended repository with search/filter capabilities
#[async_trait]
pub trait SearchableRepository<E>: CrudRepository<E>
where
    E: PersistentEntity,
{
    /// Search entities with filters
    async fn search(
        &self,
        filters: HashMap<String, String>,
        page: u32,
        limit: u32,
    ) -> Result<(Vec<E>, u64), RepositoryError>;

    /// Find by a specific field value
    async fn find_by_field(&self, field: &str, value: &str) -> Result<Option<E>, RepositoryError>;

    /// Find all by a specific field value
    async fn find_all_by_field(
        &self,
        field: &str,
        value: &str,
        page: u32,
        limit: u32,
    ) -> Result<(Vec<E>, u64), RepositoryError>;
}

// ============================================================
// PostgreSQL-Specific Traits
// ============================================================

#[cfg(feature = "postgres")]
pub use postgres_traits::*;

#[cfg(feature = "postgres")]
mod postgres_traits {
    use super::*;
    use sqlx::postgres::PgRow;
    use sqlx::FromRow;

    /// Trait for mapping entities to/from PostgreSQL rows
    ///
    /// Implement this trait to enable automatic PostgreSQL persistence for your entity.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use backbone_core::persistence::{PostgresEntity, PersistentEntity};
    /// use sqlx::FromRow;
    ///
    /// #[derive(Clone, Debug, Serialize, Deserialize, FromRow)]
    /// struct User {
    ///     id: String,
    ///     name: String,
    ///     email: String,
    ///     created_at: Option<DateTime<Utc>>,
    ///     updated_at: Option<DateTime<Utc>>,
    ///     deleted_at: Option<DateTime<Utc>>,
    /// }
    ///
    /// impl PostgresEntity for User {
    ///     fn table_name() -> &'static str { "users" }
    ///     fn select_columns() -> &'static [&'static str] {
    ///         &["id", "name", "email", "created_at", "updated_at", "deleted_at"]
    ///     }
    ///     fn insert_columns() -> &'static [&'static str] {
    ///         &["id", "name", "email", "created_at", "updated_at"]
    ///     }
    ///     fn update_columns() -> &'static [&'static str] {
    ///         &["name", "email", "updated_at"]
    ///     }
    ///     fn bind_for_insert(entity: &Self, query: Query<'_, ...>) -> Query<'_, ...> {
    ///         query.bind(&entity.id).bind(&entity.name).bind(&entity.email)
    ///             .bind(&entity.created_at).bind(&entity.updated_at)
    ///     }
    /// }
    /// ```
    pub trait PostgresEntity: PersistentEntity + for<'r> FromRow<'r, PgRow> + Unpin {
        /// Table name for this entity
        fn table_name() -> &'static str;

        /// Primary key column name (default: "id")
        fn id_column() -> &'static str {
            "id"
        }

        /// Column names for SELECT queries (excluding computed columns)
        fn select_columns() -> &'static [&'static str];

        /// Column names for INSERT (all columns that should be inserted)
        fn insert_columns() -> &'static [&'static str];

        /// Column names for UPDATE (columns that can be updated, excluding id)
        fn update_columns() -> &'static [&'static str] {
            Self::insert_columns()
        }

        /// Bind entity values to a query for INSERT
        ///
        /// The order must match insert_columns()
        fn bind_for_insert<'q>(
            entity: &'q Self,
            query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
        ) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>;

        /// Bind entity values to a query for UPDATE
        ///
        /// The order must match update_columns()
        fn bind_for_update<'q>(
            entity: &'q Self,
            query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
        ) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
            // Default: same as insert (override if different)
            Self::bind_for_insert(entity, query)
        }

        /// Build SELECT by ID query
        fn select_by_id_query() -> String {
            format!(
                "SELECT {} FROM {} WHERE {} = $1 AND deleted_at IS NULL",
                Self::select_columns().join(", "),
                Self::table_name(),
                Self::id_column()
            )
        }

        /// Build SELECT by ID including deleted
        fn select_by_id_including_deleted_query() -> String {
            format!(
                "SELECT {} FROM {} WHERE {} = $1",
                Self::select_columns().join(", "),
                Self::table_name(),
                Self::id_column()
            )
        }

        /// Build paginated list query
        fn list_query() -> String {
            format!(
                "SELECT {} FROM {} WHERE deleted_at IS NULL ORDER BY created_at DESC LIMIT $1 OFFSET $2",
                Self::select_columns().join(", "),
                Self::table_name()
            )
        }

        /// Build count query
        fn count_query() -> String {
            format!(
                "SELECT COUNT(*) FROM {} WHERE deleted_at IS NULL",
                Self::table_name()
            )
        }

        /// Build list deleted (trash) query
        fn list_deleted_query() -> String {
            format!(
                "SELECT {} FROM {} WHERE deleted_at IS NOT NULL ORDER BY deleted_at DESC LIMIT $1 OFFSET $2",
                Self::select_columns().join(", "),
                Self::table_name()
            )
        }

        /// Build count deleted query
        fn count_deleted_query() -> String {
            format!(
                "SELECT COUNT(*) FROM {} WHERE deleted_at IS NOT NULL",
                Self::table_name()
            )
        }

        /// Build soft delete query
        fn soft_delete_query() -> String {
            format!(
                "UPDATE {} SET deleted_at = NOW(), updated_at = NOW() WHERE {} = $1 AND deleted_at IS NULL",
                Self::table_name(),
                Self::id_column()
            )
        }

        /// Build restore query
        fn restore_query() -> String {
            format!(
                "UPDATE {} SET deleted_at = NULL, updated_at = NOW() WHERE {} = $1 AND deleted_at IS NOT NULL RETURNING {}",
                Self::table_name(),
                Self::id_column(),
                Self::select_columns().join(", ")
            )
        }

        /// Build hard delete query
        fn hard_delete_query() -> String {
            format!(
                "DELETE FROM {} WHERE {} = $1",
                Self::table_name(),
                Self::id_column()
            )
        }

        /// Build empty trash query
        fn empty_trash_query() -> String {
            format!(
                "DELETE FROM {} WHERE deleted_at IS NOT NULL",
                Self::table_name()
            )
        }
    }
}
