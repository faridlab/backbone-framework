//! `GenericCrudRepository<T, DeleteMode>` — one implementation for all entities.
//!
//! The generated repository file previously duplicated ~20 identical method bodies
//! for every entity.  This module provides those implementations once so that
//! the generator can emit a thin newtype wrapper instead of hundreds of lines of
//! copy-paste.
//!
//! # Delete mode markers
//!
//! | Marker | Behaviour |
//! |--------|-----------|
//! | [`SoftDelete`] | `deleted_at` stored in `metadata` JSONB; active rows filter `IS NULL` |
//! | [`HardDelete`] | Rows are permanently deleted; no trash concept |
//!
//! # Generated output (after this change)
//!
//! ```rust,ignore
//! // ~20 lines total instead of ~460-560
//! pub struct UserRepository(
//!     backbone_orm::GenericCrudRepository<User, backbone_orm::SoftDelete>
//! );
//! impl std::ops::Deref for UserRepository {
//!     type Target = backbone_orm::GenericCrudRepository<User, backbone_orm::SoftDelete>;
//!     fn deref(&self) -> &Self::Target { &self.0 }
//! }
//! impl UserRepository {
//!     pub fn new(pool: sqlx::PgPool) -> Self {
//!         Self(backbone_orm::GenericCrudRepository::new(pool, "users"))
//!     }
//!     // entity-specific methods only:
//!     pub async fn find_by_email(&self, email: &str) -> anyhow::Result<Option<User>> { … }
//!     pub async fn partial_update(…) { … }
//!     pub async fn list_paginated_filtered(…) { … }
//! }
//! backbone_core::impl_crud_repository!(UserRepository, User, soft_delete);
//! ```

use std::collections::HashMap;
use std::marker::PhantomData;

use anyhow::Result;
use serde::Serialize;
use sqlx::postgres::PgRow;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::repository::{
    DatabaseOperations, PaginatedResult, PaginationInfo, PaginationParams, PostgresRepository,
};

// ─── EntityRepoMeta trait ─────────────────────────────────────────────────────

/// Metadata trait that entities implement so `GenericCrudRepository` can provide
/// `list_paginated_filtered` and `list_deleted_filtered` without per-entity boilerplate.
///
/// Generated once per entity by the `rust` generator.
///
/// | Method | Purpose |
/// |--------|---------|
/// | `column_types` | PostgreSQL cast hints (e.g. `{"id": "uuid"}`) |
/// | `search_fields` | Text columns searched with `ILIKE` |
pub trait EntityRepoMeta {
    /// PostgreSQL type hints for filter/sort (e.g. `{"id": "uuid"}`).
    fn column_types() -> HashMap<String, String>;
    /// Text columns to include in full-text / ILIKE search.
    fn search_fields() -> &'static [&'static str];
}

// ─── Delete-mode markers ──────────────────────────────────────────────────────

/// Marker: entity uses JSONB-based soft delete (`metadata->>'deleted_at'`).
///
/// Active rows satisfy `metadata->>'deleted_at' IS NULL`.
pub struct SoftDelete;

/// Marker: entity uses hard deletes only — no soft delete / trash concept.
pub struct HardDelete;

// ─── GenericCrudRepository ────────────────────────────────────────────────────

/// Generic PostgreSQL CRUD repository parameterised by entity type `T` and
/// delete strategy `D` ([`SoftDelete`] or [`HardDelete`]).
///
/// All standard CRUD methods are implemented here once.  Generated repository
/// structs wrap this type via newtype + `Deref` and only add entity-specific
/// methods (`find_by_*`, `exists_by_*`, `partial_update`, filtered listings).
pub struct GenericCrudRepository<T, D = SoftDelete>
where
    T: for<'r> FromRow<'r, PgRow> + Send + Unpin,
{
    inner: PostgresRepository<T>,
    _mode: PhantomData<D>,
}

// ─── Common (both modes) ──────────────────────────────────────────────────────

impl<T, D> GenericCrudRepository<T, D>
where
    T: for<'r> FromRow<'r, PgRow> + Send + Unpin,
{
    pub fn new(pool: PgPool, table_name: &str) -> Self {
        Self {
            inner: PostgresRepository::new(pool, table_name),
            _mode: PhantomData,
        }
    }

    pub fn pool(&self) -> &PgPool {
        self.inner.pool()
    }

    pub fn table_name(&self) -> &str {
        self.inner.table_name()
    }

    /// Insert a new entity row and return the created row.
    pub async fn create(&self, entity: &T) -> Result<T>
    where
        T: Serialize + Send + Sync,
    {
        self.inner.create(entity).await
    }

    /// Insert multiple entity rows inside a single transaction.
    pub async fn bulk_create(&self, entities: &[T]) -> Result<Vec<T>>
    where
        T: Serialize + Send + Sync,
    {
        let tx = sqlx::pool::Pool::begin(self.pool()).await?;
        let mut results = Vec::with_capacity(entities.len());
        for entity in entities {
            results.push(self.create(entity).await?);
        }
        tx.commit().await?;
        Ok(results)
    }

    // ── Generic unique-field lookups ──────────────────────────────────────────
    //
    // These are shared across both delete modes. The SQL is the same for both;
    // the soft-delete guard is added separately by the per-mode wrappers below.

    /// Internal: query by a text field with a caller-supplied extra condition.
    async fn find_by_text_field_with_cond(
        &self,
        field: &str,
        value: &str,
        extra: &str,
    ) -> Result<Option<T>> {
        let query = format!(
            "SELECT * FROM {} WHERE {} = $1{}",
            self.table_name(), field, extra
        );
        let result = sqlx::query_as::<_, T>(&query)
            .bind(value)
            .fetch_optional(self.pool())
            .await?;
        Ok(result)
    }

    /// Internal: existence check by a text field with a caller-supplied extra condition.
    async fn exists_by_text_field_with_cond(
        &self,
        field: &str,
        value: &str,
        extra: &str,
    ) -> Result<bool> {
        let query = format!(
            "SELECT 1 FROM {} WHERE {} = $1{} LIMIT 1",
            self.table_name(), field, extra
        );
        let result = sqlx::query_scalar::<_, i32>(&query)
            .bind(value)
            .fetch_optional(self.pool())
            .await?;
        Ok(result.is_some())
    }

    /// Internal: query by a UUID field with a caller-supplied extra condition.
    async fn find_by_uuid_field_with_cond(
        &self,
        field: &str,
        value: Uuid,
        extra: &str,
    ) -> Result<Option<T>> {
        let query = format!(
            "SELECT * FROM {} WHERE {} = $1{}",
            self.table_name(), field, extra
        );
        let result = sqlx::query_as::<_, T>(&query)
            .bind(value)
            .fetch_optional(self.pool())
            .await?;
        Ok(result)
    }

    /// Internal: existence check by a UUID field with a caller-supplied extra condition.
    async fn exists_by_uuid_field_with_cond(
        &self,
        field: &str,
        value: Uuid,
        extra: &str,
    ) -> Result<bool> {
        let query = format!(
            "SELECT 1 FROM {} WHERE {} = $1{} LIMIT 1",
            self.table_name(), field, extra
        );
        let result = sqlx::query_scalar::<_, i32>(&query)
            .bind(value)
            .fetch_optional(self.pool())
            .await?;
        Ok(result.is_some())
    }

    /// Execute a filtered / paginated query against this entity's table.
    ///
    /// `base_condition` — when `Some`, it is inserted as the `__base_condition`
    /// filter key which the ORM injects verbatim into the WHERE clause.  Use
    /// this to add soft-delete guards without touching the caller-supplied
    /// filters.
    pub async fn run_filtered_query(
        &self,
        pagination: PaginationParams,
        base_condition: Option<&str>,
        filters: &HashMap<String, String>,
        column_types: &HashMap<String, String>,
        search_fields: &[&str],
    ) -> Result<PaginatedResult<T>>
    where
        T: Send + Sync,
    {
        let mut filters_map = filters.clone();
        if let Some(cond) = base_condition {
            filters_map.insert("__base_condition".to_string(), cond.to_string());
        }
        self.inner
            .list_with_filters(pagination, &filters_map, column_types, search_fields)
            .await
    }
}

// ─── SoftDelete mode ──────────────────────────────────────────────────────────

impl<T> GenericCrudRepository<T, SoftDelete>
where
    T: for<'r> FromRow<'r, PgRow> + Send + Unpin,
{
    // ── Unique-field lookups (active records only) ────────────────────────────

    /// Find an active entity by a unique text field.
    ///
    /// Filters `metadata->>'deleted_at' IS NULL` automatically.
    pub async fn find_by_text_field(&self, field: &str, value: &str) -> Result<Option<T>> {
        self.find_by_text_field_with_cond(field, value, " AND metadata->>'deleted_at' IS NULL").await
    }

    /// Check existence by a unique text field (active records only).
    pub async fn exists_by_text_field(&self, field: &str, value: &str) -> Result<bool> {
        self.exists_by_text_field_with_cond(field, value, " AND metadata->>'deleted_at' IS NULL").await
    }

    /// Find an active entity by a unique UUID field.
    pub async fn find_by_uuid_field(&self, field: &str, value: Uuid) -> Result<Option<T>> {
        self.find_by_uuid_field_with_cond(field, value, " AND metadata->>'deleted_at' IS NULL").await
    }

    /// Check existence by a unique UUID field (active records only).
    pub async fn exists_by_uuid_field(&self, field: &str, value: Uuid) -> Result<bool> {
        self.exists_by_uuid_field_with_cond(field, value, " AND metadata->>'deleted_at' IS NULL").await
    }

    // ── Filtered pagination (requires EntityRepoMeta on T) ────────────────────

    /// Paginate active entities with filter and search support.
    ///
    /// Requires `T: EntityRepoMeta` for column type hints and search fields.
    pub async fn list_paginated_filtered(
        &self,
        pagination: PaginationParams,
        filters: Option<&HashMap<String, String>>,
    ) -> Result<PaginatedResult<T>>
    where
        T: EntityRepoMeta + Send + Sync,
    {
        let filters_map = filters.cloned().unwrap_or_default();
        let column_types = T::column_types();
        let search_fields_owned: Vec<&'static str> = T::search_fields().iter().copied().collect();
        self.run_filtered_query(
            pagination,
            Some("metadata->>'deleted_at' IS NULL"),
            &filters_map,
            &column_types,
            &search_fields_owned,
        ).await
    }

    /// Paginate soft-deleted entities with filter support.
    pub async fn list_deleted_filtered(
        &self,
        pagination: PaginationParams,
        filters: Option<&HashMap<String, String>>,
    ) -> Result<PaginatedResult<T>>
    where
        T: EntityRepoMeta + Send + Sync,
    {
        let filters_map = filters.cloned().unwrap_or_default();
        let column_types = T::column_types();
        let empty: &[&str] = &[];
        self.run_filtered_query(
            pagination,
            Some("metadata->>'deleted_at' IS NOT NULL"),
            &filters_map,
            &column_types,
            empty,
        ).await
    }

    /// Find an active (non-deleted) entity by primary key.
    pub async fn find_by_id(&self, id: &str) -> Result<Option<T>> {
        let query = format!(
            "SELECT * FROM {} WHERE id = $1::uuid AND metadata->>'deleted_at' IS NULL",
            self.table_name()
        );
        let result = sqlx::query_as::<_, T>(&query)
            .bind(id)
            .fetch_optional(self.pool())
            .await?;
        Ok(result)
    }

    /// Return all active (non-deleted) entities.
    pub async fn find_all(&self) -> Result<Vec<T>> {
        let query = format!(
            "SELECT * FROM {} WHERE metadata->>'deleted_at' IS NULL",
            self.table_name()
        );
        let results = sqlx::query_as::<_, T>(&query)
            .fetch_all(self.pool())
            .await?;
        Ok(results)
    }

    /// Full update — skips silently if the record is already soft-deleted.
    pub async fn update(&self, id: &str, entity: &T) -> Result<Option<T>>
    where
        T: Serialize + Send + Sync,
    {
        if self.find_by_id(id).await?.is_none() {
            return Ok(None);
        }
        self.inner.update(id, entity).await
    }

    /// Soft-delete an entity (sets `metadata.deleted_at`).
    pub async fn delete(&self, id: &str) -> Result<bool> {
        self.soft_delete(id).await
    }

    /// Count active (non-deleted) entities.
    pub async fn count(&self) -> Result<u64> {
        self.count_active().await
    }

    /// Return `true` if an active entity with the given ID exists.
    pub async fn exists(&self, id: &str) -> Result<bool> {
        let query = format!(
            "SELECT 1 FROM {} WHERE id = $1::uuid AND metadata->>'deleted_at' IS NULL LIMIT 1",
            self.table_name()
        );
        let result = sqlx::query_scalar::<_, i32>(&query)
            .bind(id)
            .fetch_optional(self.pool())
            .await?;
        Ok(result.is_some())
    }

    /// Paginate active entities (most-recent-first by ID).
    pub async fn list_paginated(&self, pagination: PaginationParams) -> Result<PaginatedResult<T>> {
        let offset = pagination.offset();
        let limit = pagination.limit();
        let query = format!(
            "SELECT * FROM {} WHERE metadata->>'deleted_at' IS NULL \
             ORDER BY id DESC LIMIT $1 OFFSET $2",
            self.table_name()
        );
        let data = sqlx::query_as::<_, T>(&query)
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(self.pool())
            .await?;
        let total = self.count_active().await?;
        Ok(PaginatedResult {
            data,
            pagination: PaginationInfo::new(pagination.page, pagination.per_page, total),
        })
    }

    // ── Soft-delete helpers ───────────────────────────────────────────────────

    /// Set `metadata.deleted_at` to NOW() (soft delete).
    pub async fn soft_delete(&self, id: &str) -> Result<bool> {
        let query = format!(
            "UPDATE {} SET metadata = jsonb_set(\
               COALESCE(metadata, '{{}}'), \
               '{{deleted_at}}', \
               to_jsonb(NOW())\
             ) WHERE id = $1::uuid AND (metadata->>'deleted_at') IS NULL",
            self.table_name()
        );
        let result = sqlx::query(&query)
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Remove `deleted_at` from metadata, restoring the entity.
    pub async fn restore(&self, id: &str) -> Result<Option<T>> {
        let query = format!(
            "UPDATE {} SET metadata = metadata - 'deleted_at' \
             WHERE id = $1::uuid AND (metadata->>'deleted_at') IS NOT NULL \
             RETURNING *",
            self.table_name()
        );
        let result = sqlx::query_as::<_, T>(&query)
            .bind(id)
            .fetch_optional(self.pool())
            .await?;
        Ok(result)
    }

    /// Paginate soft-deleted entities (trash view).
    pub async fn list_deleted(&self, pagination: PaginationParams) -> Result<PaginatedResult<T>> {
        let offset = pagination.offset();
        let limit = pagination.limit();
        let query = format!(
            "SELECT * FROM {} WHERE (metadata->>'deleted_at') IS NOT NULL \
             ORDER BY (metadata->>'deleted_at') DESC LIMIT $1 OFFSET $2",
            self.table_name()
        );
        let data = sqlx::query_as::<_, T>(&query)
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(self.pool())
            .await?;
        let count_query = format!(
            "SELECT COUNT(*) FROM {} WHERE (metadata->>'deleted_at') IS NOT NULL",
            self.table_name()
        );
        let total = sqlx::query_scalar::<_, i64>(&count_query)
            .fetch_one(self.pool())
            .await? as u64;
        Ok(PaginatedResult {
            data,
            pagination: PaginationInfo::new(pagination.page, pagination.per_page, total),
        })
    }

    /// Permanently delete all soft-deleted rows (empty trash).
    pub async fn empty_trash(&self) -> Result<u64> {
        let query = format!(
            "DELETE FROM {} WHERE (metadata->>'deleted_at') IS NOT NULL",
            self.table_name()
        );
        let result = sqlx::query(&query).execute(self.pool()).await?;
        Ok(result.rows_affected())
    }

    /// Find a soft-deleted entity by primary key.
    pub async fn find_deleted_by_id(&self, id: &str) -> Result<Option<T>> {
        let query = format!(
            "SELECT * FROM {} WHERE id = $1::uuid AND (metadata->>'deleted_at') IS NOT NULL",
            self.table_name()
        );
        let result = sqlx::query_as::<_, T>(&query)
            .bind(id)
            .fetch_optional(self.pool())
            .await?;
        Ok(result)
    }

    /// Permanently delete a soft-deleted entity by primary key.
    pub async fn permanent_delete(&self, id: &str) -> Result<bool> {
        let query = format!(
            "DELETE FROM {} WHERE id = $1::uuid AND (metadata->>'deleted_at') IS NOT NULL",
            self.table_name()
        );
        let result = sqlx::query(&query).bind(id).execute(self.pool()).await?;
        Ok(result.rows_affected() > 0)
    }

    /// Count active (non-deleted) entities.
    pub async fn count_active(&self) -> Result<u64> {
        let query = format!(
            "SELECT COUNT(*) FROM {} WHERE (metadata->>'deleted_at') IS NULL",
            self.table_name()
        );
        let count = sqlx::query_scalar::<_, i64>(&query)
            .fetch_one(self.pool())
            .await? as u64;
        Ok(count)
    }

    /// Count soft-deleted entities.
    pub async fn count_deleted(&self) -> Result<u64> {
        let query = format!(
            "SELECT COUNT(*) FROM {} WHERE (metadata->>'deleted_at') IS NOT NULL",
            self.table_name()
        );
        let count = sqlx::query_scalar::<_, i64>(&query)
            .fetch_one(self.pool())
            .await? as u64;
        Ok(count)
    }
}

// ─── HardDelete mode ─────────────────────────────────────────────────────────

impl<T> GenericCrudRepository<T, HardDelete>
where
    T: for<'r> FromRow<'r, PgRow> + Send + Sync + Unpin + Serialize,
{
    // ── Unique-field lookups (no soft-delete guard) ───────────────────────────

    /// Find an entity by a unique text field (no soft-delete guard).
    pub async fn find_by_text_field(&self, field: &str, value: &str) -> Result<Option<T>> {
        self.find_by_text_field_with_cond(field, value, "").await
    }

    /// Check existence by a unique text field.
    pub async fn exists_by_text_field(&self, field: &str, value: &str) -> Result<bool> {
        self.exists_by_text_field_with_cond(field, value, "").await
    }

    /// Find an entity by a unique UUID field.
    pub async fn find_by_uuid_field(&self, field: &str, value: Uuid) -> Result<Option<T>> {
        self.find_by_uuid_field_with_cond(field, value, "").await
    }

    /// Check existence by a unique UUID field.
    pub async fn exists_by_uuid_field(&self, field: &str, value: Uuid) -> Result<bool> {
        self.exists_by_uuid_field_with_cond(field, value, "").await
    }

    // ── Filtered pagination ───────────────────────────────────────────────────

    /// Paginate entities with filter and search support.
    pub async fn list_paginated_filtered(
        &self,
        pagination: PaginationParams,
        filters: Option<&HashMap<String, String>>,
    ) -> Result<PaginatedResult<T>>
    where
        T: EntityRepoMeta + Send + Sync,
    {
        let filters_map = filters.cloned().unwrap_or_default();
        let column_types = T::column_types();
        let search_fields_owned: Vec<&'static str> = T::search_fields().iter().copied().collect();
        self.run_filtered_query(pagination, None, &filters_map, &column_types, &search_fields_owned).await
    }

    /// Find an entity by primary key.
    pub async fn find_by_id(&self, id: &str) -> Result<Option<T>> {
        self.inner.find_by_id(id).await
    }

    /// Return all entities.
    pub async fn find_all(&self) -> Result<Vec<T>> {
        let query = format!("SELECT * FROM {}", self.table_name());
        let results = sqlx::query_as::<_, T>(&query)
            .fetch_all(self.pool())
            .await?;
        Ok(results)
    }

    /// Full update.
    pub async fn update(&self, id: &str, entity: &T) -> Result<Option<T>> {
        self.inner.update(id, entity).await
    }

    /// Permanently delete an entity by primary key.
    pub async fn delete(&self, id: &str) -> Result<bool> {
        self.inner.delete(id).await
    }

    /// Count all entities.
    pub async fn count(&self) -> Result<u64> {
        let query = format!("SELECT COUNT(*) FROM {}", self.table_name());
        let count = sqlx::query_scalar::<_, i64>(&query)
            .fetch_one(self.pool())
            .await? as u64;
        Ok(count)
    }

    /// Return `true` if an entity with the given ID exists.
    pub async fn exists(&self, id: &str) -> Result<bool> {
        let query = format!(
            "SELECT 1 FROM {} WHERE id = $1::uuid LIMIT 1",
            self.table_name()
        );
        let result = sqlx::query_scalar::<_, i32>(&query)
            .bind(id)
            .fetch_optional(self.pool())
            .await?;
        Ok(result.is_some())
    }

    /// Paginate entities (most-recent-first by ID).
    pub async fn list_paginated(&self, pagination: PaginationParams) -> Result<PaginatedResult<T>> {
        let offset = pagination.offset();
        let limit = pagination.limit();
        let query = format!(
            "SELECT * FROM {} ORDER BY id DESC LIMIT $1 OFFSET $2",
            self.table_name()
        );
        let data = sqlx::query_as::<_, T>(&query)
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(self.pool())
            .await?;
        let total = self.count().await?;
        Ok(PaginatedResult {
            data,
            pagination: PaginationInfo::new(pagination.page, pagination.per_page, total),
        })
    }
}
