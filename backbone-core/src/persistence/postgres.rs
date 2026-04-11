//! PostgreSQL Repository Implementation
//!
//! Generic PostgreSQL-backed repository for production use.
//!
//! # Usage
//!
//! 1. Implement `PostgresEntity` for your entity (includes bind methods)
//! 2. Derive `FromRow` for automatic row mapping
//! 3. Create a repository instance with your entity type
//!
//! ```ignore
//! use backbone_core::persistence::{PostgresRepository, PostgresEntity, PersistentEntity};
//! use sqlx::FromRow;
//!
//! #[derive(Clone, Debug, Serialize, Deserialize, FromRow)]
//! struct User {
//!     id: String,
//!     name: String,
//!     email: String,
//!     created_at: Option<DateTime<Utc>>,
//!     updated_at: Option<DateTime<Utc>>,
//!     deleted_at: Option<DateTime<Utc>>,
//! }
//!
//! impl PersistentEntity for User { /* ... */ }
//!
//! impl PostgresEntity for User {
//!     fn table_name() -> &'static str { "users" }
//!     fn select_columns() -> &'static [&'static str] {
//!         &["id", "name", "email", "created_at", "updated_at", "deleted_at"]
//!     }
//!     fn insert_columns() -> &'static [&'static str] {
//!         &["id", "name", "email", "created_at", "updated_at"]
//!     }
//!     fn bind_for_insert<'q>(entity: &Self, query: Query<'q, ...>) -> Query<'q, ...> {
//!         query.bind(&entity.id).bind(&entity.name).bind(&entity.email)
//!             .bind(entity.created_at).bind(entity.updated_at)
//!     }
//! }
//!
//! let pool = PgPool::connect("...").await?;
//! let repo = PostgresRepository::<User>::new(pool);
//! ```

use async_trait::async_trait;
use sqlx::postgres::PgRow;
use sqlx::PgPool;
use std::collections::HashMap;
use std::marker::PhantomData;

use super::traits::{CrudRepository, PostgresEntity, RepositoryError, SearchableRepository};

/// Generic PostgreSQL repository
///
/// Provides PostgreSQL-backed persistence for any entity implementing
/// `PostgresEntity` (which includes `FromRow` + `PersistentEntity`).
pub struct PostgresRepository<E>
where
    E: PostgresEntity,
{
    pool: PgPool,
    _phantom: PhantomData<E>,
}

impl<E> PostgresRepository<E>
where
    E: PostgresEntity,
{
    /// Create a new repository with the given connection pool
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            _phantom: PhantomData,
        }
    }

    /// Get a reference to the connection pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Execute a raw query and return entities
    pub async fn query_as(&self, sql: &str) -> Result<Vec<E>, RepositoryError> {
        let rows = sqlx::query(sql).fetch_all(&self.pool).await?;
        rows.iter()
            .map(|row| E::from_row(row).map_err(|e| RepositoryError::DatabaseError(e.to_string())))
            .collect()
    }
}

impl<E> Clone for PostgresRepository<E>
where
    E: PostgresEntity,
{
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            _phantom: PhantomData,
        }
    }
}

#[async_trait]
impl<E> CrudRepository<E> for PostgresRepository<E>
where
    E: PostgresEntity,
{
    async fn create(&self, mut entity: E) -> Result<E, RepositoryError> {
        // Generate ID if empty
        if entity.entity_id().is_empty() {
            entity.set_entity_id(E::generate_id());
        }

        // Set timestamps
        let now = chrono::Utc::now();
        if entity.created_at().is_none() {
            entity.set_created_at(now);
        }
        entity.set_updated_at(now);

        // Build INSERT query
        let table = E::table_name();
        let columns = E::insert_columns();
        let select_cols = E::select_columns().join(", ");

        let placeholders: Vec<String> = (1..=columns.len()).map(|i| format!("${}", i)).collect();

        let query = format!(
            "INSERT INTO {} ({}) VALUES ({}) RETURNING {}",
            table,
            columns.join(", "),
            placeholders.join(", "),
            select_cols
        );

        let q = sqlx::query(&query);
        let q = E::bind_for_insert(&entity, q);
        let row: PgRow = q.fetch_one(&self.pool).await?;

        E::from_row(&row).map_err(|e| RepositoryError::DatabaseError(e.to_string()))
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<E>, RepositoryError> {
        let query = E::select_by_id_query();
        let row = sqlx::query(&query)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(row) => Ok(Some(
                E::from_row(&row).map_err(|e| RepositoryError::DatabaseError(e.to_string()))?,
            )),
            None => Ok(None),
        }
    }

    async fn find_by_id_including_deleted(&self, id: &str) -> Result<Option<E>, RepositoryError> {
        let query = E::select_by_id_including_deleted_query();
        let row = sqlx::query(&query)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(row) => Ok(Some(
                E::from_row(&row).map_err(|e| RepositoryError::DatabaseError(e.to_string()))?,
            )),
            None => Ok(None),
        }
    }

    async fn update(&self, mut entity: E) -> Result<E, RepositoryError> {
        entity.set_updated_at(chrono::Utc::now());

        // Build UPDATE query
        let table = E::table_name();
        let id_col = E::id_column();
        let columns = E::update_columns();
        let select_cols = E::select_columns().join(", ");

        // Build SET clause: col1 = $1, col2 = $2, ...
        let set_clause: Vec<String> = columns
            .iter()
            .enumerate()
            .map(|(i, col)| format!("{} = ${}", col, i + 1))
            .collect();

        let id_param = columns.len() + 1;
        let query = format!(
            "UPDATE {} SET {} WHERE {} = ${} RETURNING {}",
            table,
            set_clause.join(", "),
            id_col,
            id_param,
            select_cols
        );

        let q = sqlx::query(&query);
        let q = E::bind_for_update(&entity, q);
        let q = q.bind(entity.entity_id());
        let row: PgRow = q.fetch_one(&self.pool).await?;

        E::from_row(&row).map_err(|e| RepositoryError::DatabaseError(e.to_string()))
    }

    async fn soft_delete(&self, id: &str) -> Result<bool, RepositoryError> {
        let query = E::soft_delete_query();
        let result = sqlx::query(&query).bind(id).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    async fn restore(&self, id: &str) -> Result<Option<E>, RepositoryError> {
        let query = E::restore_query();
        let row = sqlx::query(&query)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(row) => Ok(Some(
                E::from_row(&row).map_err(|e| RepositoryError::DatabaseError(e.to_string()))?,
            )),
            None => Ok(None),
        }
    }

    async fn hard_delete(&self, id: &str) -> Result<bool, RepositoryError> {
        let query = E::hard_delete_query();
        let result = sqlx::query(&query).bind(id).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    async fn list(&self, page: u32, limit: u32) -> Result<(Vec<E>, u64), RepositoryError> {
        let offset = (page.saturating_sub(1)) * limit;

        // Get count
        let count_query = E::count_query();
        let count: i64 = sqlx::query_scalar(&count_query)
            .fetch_one(&self.pool)
            .await?;
        let total = count as u64;

        // Get entities
        let list_query = E::list_query();
        let rows = sqlx::query(&list_query)
            .bind(limit as i32)
            .bind(offset as i32)
            .fetch_all(&self.pool)
            .await?;

        let entities: Result<Vec<E>, _> = rows
            .iter()
            .map(|row| E::from_row(row))
            .collect();

        Ok((
            entities.map_err(|e| RepositoryError::DatabaseError(e.to_string()))?,
            total,
        ))
    }

    async fn list_deleted(&self, page: u32, limit: u32) -> Result<(Vec<E>, u64), RepositoryError> {
        let offset = (page.saturating_sub(1)) * limit;

        // Get count
        let count_query = E::count_deleted_query();
        let count: i64 = sqlx::query_scalar(&count_query)
            .fetch_one(&self.pool)
            .await?;
        let total = count as u64;

        // Get entities
        let list_query = E::list_deleted_query();
        let rows = sqlx::query(&list_query)
            .bind(limit as i32)
            .bind(offset as i32)
            .fetch_all(&self.pool)
            .await?;

        let entities: Result<Vec<E>, _> = rows
            .iter()
            .map(|row| E::from_row(row))
            .collect();

        Ok((
            entities.map_err(|e| RepositoryError::DatabaseError(e.to_string()))?,
            total,
        ))
    }

    async fn count(&self) -> Result<u64, RepositoryError> {
        let query = E::count_query();
        let count: i64 = sqlx::query_scalar(&query).fetch_one(&self.pool).await?;
        Ok(count as u64)
    }

    async fn count_deleted(&self) -> Result<u64, RepositoryError> {
        let query = E::count_deleted_query();
        let count: i64 = sqlx::query_scalar(&query).fetch_one(&self.pool).await?;
        Ok(count as u64)
    }

    async fn bulk_create(&self, entities: Vec<E>) -> Result<Vec<E>, RepositoryError> {
        // Use a transaction for bulk operations
        let mut tx = self.pool.begin().await?;
        let mut results = Vec::with_capacity(entities.len());

        for mut entity in entities {
            // Generate ID if empty
            if entity.entity_id().is_empty() {
                entity.set_entity_id(E::generate_id());
            }

            // Set timestamps
            let now = chrono::Utc::now();
            if entity.created_at().is_none() {
                entity.set_created_at(now);
            }
            entity.set_updated_at(now);

            // Build INSERT query
            let table = E::table_name();
            let columns = E::insert_columns();
            let select_cols = E::select_columns().join(", ");

            let placeholders: Vec<String> =
                (1..=columns.len()).map(|i| format!("${}", i)).collect();

            let query = format!(
                "INSERT INTO {} ({}) VALUES ({}) RETURNING {}",
                table,
                columns.join(", "),
                placeholders.join(", "),
                select_cols
            );

            let q = sqlx::query(&query);
            let q = E::bind_for_insert(&entity, q);
            let row: PgRow = q.fetch_one(&mut *tx).await?;
            let created =
                E::from_row(&row).map_err(|e| RepositoryError::DatabaseError(e.to_string()))?;
            results.push(created);
        }

        tx.commit().await?;
        Ok(results)
    }

    async fn empty_trash(&self) -> Result<u64, RepositoryError> {
        let query = E::empty_trash_query();
        let result = sqlx::query(&query).execute(&self.pool).await?;
        Ok(result.rows_affected())
    }
}

#[async_trait]
impl<E> SearchableRepository<E> for PostgresRepository<E>
where
    E: PostgresEntity,
{
    async fn search(
        &self,
        filters: HashMap<String, String>,
        page: u32,
        limit: u32,
    ) -> Result<(Vec<E>, u64), RepositoryError> {
        let offset = (page.saturating_sub(1)) * limit;
        let table = E::table_name();
        let columns = E::select_columns().join(", ");

        // Build WHERE clause from filters
        let mut conditions = vec!["deleted_at IS NULL".to_string()];
        let mut bind_values: Vec<String> = Vec::new();
        let mut param_idx = 1;

        for (key, value) in &filters {
            // Only allow filtering on known columns to prevent SQL injection
            if E::select_columns().contains(&key.as_str()) {
                conditions.push(format!("{}::text ILIKE ${}", key, param_idx));
                bind_values.push(format!("%{}%", value));
                param_idx += 1;
            }
        }

        let where_clause = conditions.join(" AND ");

        // Count query
        let count_query = format!("SELECT COUNT(*) FROM {} WHERE {}", table, where_clause);

        let mut count_q = sqlx::query_scalar::<_, i64>(&count_query);
        for val in &bind_values {
            count_q = count_q.bind(val.clone());
        }
        let total = count_q.fetch_one(&self.pool).await? as u64;

        // List query
        let list_query = format!(
            "SELECT {} FROM {} WHERE {} ORDER BY created_at DESC LIMIT ${} OFFSET ${}",
            columns,
            table,
            where_clause,
            param_idx,
            param_idx + 1
        );

        let mut list_q = sqlx::query(&list_query);
        for val in &bind_values {
            list_q = list_q.bind(val.clone());
        }
        list_q = list_q.bind(limit as i32).bind(offset as i32);

        let rows = list_q.fetch_all(&self.pool).await?;
        let entities: Result<Vec<E>, _> = rows.iter().map(|row| E::from_row(row)).collect();

        Ok((
            entities.map_err(|e| RepositoryError::DatabaseError(e.to_string()))?,
            total,
        ))
    }

    async fn find_by_field(&self, field: &str, value: &str) -> Result<Option<E>, RepositoryError> {
        // Validate field is a known column
        if !E::select_columns().contains(&field) {
            return Err(RepositoryError::ValidationError(format!(
                "Unknown field: {}",
                field
            )));
        }

        let query = format!(
            "SELECT {} FROM {} WHERE {} = $1 AND deleted_at IS NULL LIMIT 1",
            E::select_columns().join(", "),
            E::table_name(),
            field
        );

        let row = sqlx::query(&query)
            .bind(value)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(row) => Ok(Some(
                E::from_row(&row).map_err(|e| RepositoryError::DatabaseError(e.to_string()))?,
            )),
            None => Ok(None),
        }
    }

    async fn find_all_by_field(
        &self,
        field: &str,
        value: &str,
        page: u32,
        limit: u32,
    ) -> Result<(Vec<E>, u64), RepositoryError> {
        // Validate field is a known column
        if !E::select_columns().contains(&field) {
            return Err(RepositoryError::ValidationError(format!(
                "Unknown field: {}",
                field
            )));
        }

        let offset = (page.saturating_sub(1)) * limit;

        // Count
        let count_query = format!(
            "SELECT COUNT(*) FROM {} WHERE {} = $1 AND deleted_at IS NULL",
            E::table_name(),
            field
        );
        let total: i64 = sqlx::query_scalar(&count_query)
            .bind(value)
            .fetch_one(&self.pool)
            .await?;

        // List
        let list_query = format!(
            "SELECT {} FROM {} WHERE {} = $1 AND deleted_at IS NULL ORDER BY created_at DESC LIMIT $2 OFFSET $3",
            E::select_columns().join(", "),
            E::table_name(),
            field
        );

        let rows = sqlx::query(&list_query)
            .bind(value)
            .bind(limit as i32)
            .bind(offset as i32)
            .fetch_all(&self.pool)
            .await?;

        let entities: Result<Vec<E>, _> = rows.iter().map(|row| E::from_row(row)).collect();

        Ok((
            entities.map_err(|e| RepositoryError::DatabaseError(e.to_string()))?,
            total as u64,
        ))
    }
}

// ============================================================
// Helper: Repository Builder
// ============================================================

/// Builder for creating repositories with custom configuration
pub struct PostgresRepositoryBuilder<E>
where
    E: PostgresEntity,
{
    pool: Option<PgPool>,
    _phantom: PhantomData<E>,
}

impl<E> PostgresRepositoryBuilder<E>
where
    E: PostgresEntity,
{
    pub fn new() -> Self {
        Self {
            pool: None,
            _phantom: PhantomData,
        }
    }

    pub fn with_pool(mut self, pool: PgPool) -> Self {
        self.pool = Some(pool);
        self
    }

    pub fn build(self) -> Result<PostgresRepository<E>, RepositoryError> {
        let pool = self.pool.ok_or_else(|| {
            RepositoryError::InternalError("Database pool not configured".to_string())
        })?;

        Ok(PostgresRepository::new(pool))
    }
}

impl<E> Default for PostgresRepositoryBuilder<E>
where
    E: PostgresEntity,
{
    fn default() -> Self {
        Self::new()
    }
}
