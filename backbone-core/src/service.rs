//! Generic CRUD service — one implementation used by all entities.
//!
//! Generated code emits only a type alias:
//!
//! ```rust,ignore
//! // Generated (was ~250 lines, now 1 line):
//! pub type StoredFileService = GenericCrudService<
//!     StoredFile,
//!     CreateStoredFileDto,
//!     UpdateStoredFileDto,
//!     PostgresStoredFileRepository,
//! >;
//! ```
//!
//! Custom services wrap the alias with a decorator:
//!
//! ```rust,ignore
//! pub struct StoredFileServiceCustom {
//!     inner: Arc<StoredFileService>,
//!     notifications: Arc<NotificationService>,
//! }
//!
//! impl StoredFileServiceCustom {
//!     pub async fn archive(&self, id: &str, reason: String) -> ServiceResult<StoredFile> {
//!         let entity = self.inner.get_by_id(id).await?.ok_or(ServiceError::NotFound)?;
//!         // custom business logic
//!     }
//! }
//! ```

use async_trait::async_trait;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;

use backbone_messaging::crud_event::{CrudEvent, CrudEventPublisher, EventMetadata, NoOpCrudEventPublisher};

use crate::persistence::traits::{CrudRepository, PersistentEntity, RepositoryError};

// ─── Service error ───────────────────────────────────────────────────────────

/// Standard service-layer error type.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("not found")]
    NotFound,

    #[error("already exists: {0}")]
    AlreadyExists(String),

    #[error("validation failed: {0}")]
    Validation(String),

    #[error("repository error: {0}")]
    Repository(#[from] RepositoryError),

    #[error("internal error: {0}")]
    Internal(String),
}

pub type ServiceResult<T> = Result<T, ServiceError>;

// ─── DTO conversion traits ────────────────────────────────────────────────────

/// Convert a Create DTO into a new entity instance.
///
/// Implemented once per entity by generated code, may be overridden in custom
/// decorators via `UseCaseHooks`.
pub trait FromCreateDto<DTO>: Sized {
    fn from_create_dto(dto: DTO) -> ServiceResult<Self>;
}

/// Apply an Update DTO's fields to an existing entity (full update).
pub trait ApplyUpdateDto<DTO> {
    fn apply_update(self, dto: DTO) -> ServiceResult<Self>
    where
        Self: Sized;
}

// ─── ServiceLifecycle hooks ──────────────────────────────────────────────────

/// Optional lifecycle callbacks injected into `GenericCrudService`.
///
/// Override individual methods — all default to no-op.
#[async_trait]
pub trait ServiceLifecycle<E: PersistentEntity>: Send + Sync {
    async fn before_create(&self, entity: &mut E) -> ServiceResult<()> {
        let _ = entity;
        Ok(())
    }

    async fn after_create(&self, entity: &E) -> ServiceResult<()> {
        let _ = entity;
        Ok(())
    }

    async fn before_update(&self, entity: &mut E) -> ServiceResult<()> {
        let _ = entity;
        Ok(())
    }

    async fn after_update(&self, entity: &E) -> ServiceResult<()> {
        let _ = entity;
        Ok(())
    }

    async fn before_delete(&self, entity: &E) -> ServiceResult<()> {
        let _ = entity;
        Ok(())
    }

    async fn after_delete(&self, id: &str) -> ServiceResult<()> {
        let _ = id;
        Ok(())
    }
}

/// No-op lifecycle — the default used by generated type aliases.
pub struct NoOpLifecycle<E> {
    _phantom: PhantomData<E>,
}

impl<E> NoOpLifecycle<E> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<E> Default for NoOpLifecycle<E> {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<E: PersistentEntity> ServiceLifecycle<E> for NoOpLifecycle<E> {}

// ─── GenericCrudService ──────────────────────────────────────────────────────

/// A fully generic CRUD service.
///
/// Type parameters:
/// - `E`  — entity type (must implement `PersistentEntity`)
/// - `C`  — create DTO (entity implements `FromCreateDto<C>`)
/// - `U`  — update DTO (entity implements `ApplyUpdateDto<U>`)
/// - `R`  — repository (implements `CrudRepository<E>`)
///
/// The service always holds an `Arc<dyn CrudEventPublisher<E>>`.  When no
/// publisher is needed, `NoOpCrudEventPublisher` is used (zero overhead).
/// Event-publish errors are fire-and-forget — they never fail the operation.
pub struct GenericCrudService<E, C, U, R>
where
    E: PersistentEntity + Clone,
    R: CrudRepository<E>,
{
    repository: Arc<R>,
    lifecycle: Arc<dyn ServiceLifecycle<E>>,
    event_publisher: Arc<dyn CrudEventPublisher<E>>,
    _phantom: PhantomData<(C, U)>,
}

impl<E, C, U, R> GenericCrudService<E, C, U, R>
where
    E: PersistentEntity + Clone + FromCreateDto<C> + ApplyUpdateDto<U>,
    C: Send + Sync + 'static,
    U: Send + Sync + 'static,
    R: CrudRepository<E>,
{
    /// Full constructor — supply all three dependencies.
    pub fn new(
        repository: Arc<R>,
        lifecycle: Arc<dyn ServiceLifecycle<E>>,
        event_publisher: Arc<dyn CrudEventPublisher<E>>,
    ) -> Self {
        Self {
            repository,
            lifecycle,
            event_publisher,
            _phantom: PhantomData,
        }
    }

    /// Create with a custom lifecycle and the no-op event publisher.
    pub fn with_lifecycle(repository: Arc<R>, lifecycle: Arc<dyn ServiceLifecycle<E>>) -> Self {
        Self::new(repository, lifecycle, NoOpCrudEventPublisher::arc())
    }

    /// Create with the no-op lifecycle and no-op event publisher (generated default).
    pub fn with_repository(repository: Arc<R>) -> Self {
        Self::new(
            repository,
            Arc::new(NoOpLifecycle::new()),
            NoOpCrudEventPublisher::arc(),
        )
    }

    /// Replace the event publisher on an existing service (builder pattern).
    pub fn with_event_publisher(mut self, publisher: Arc<dyn CrudEventPublisher<E>>) -> Self {
        self.event_publisher = publisher;
        self
    }

    /// Access the underlying repository for custom query extensions.
    pub fn repository(&self) -> &R {
        &self.repository
    }

    // ─── Individual operations ────────────────────────────────────────────

    pub async fn list(
        &self,
        page: u32,
        limit: u32,
        filters: HashMap<String, String>,
    ) -> ServiceResult<(Vec<E>, u64)> {
        self.repository
            .list_filtered(page, limit, filters)
            .await
            .map_err(ServiceError::Repository)
    }

    pub async fn create(&self, dto: C) -> ServiceResult<E> {
        let mut entity = E::from_create_dto(dto)?;
        self.lifecycle.before_create(&mut entity).await?;
        let saved = self
            .repository
            .create(entity)
            .await
            .map_err(ServiceError::Repository)?;
        self.lifecycle.after_create(&saved).await?;

        let meta = EventMetadata::new(saved.entity_id(), std::any::type_name::<E>());
        let _ = self
            .event_publisher
            .publish(CrudEvent::Created { entity: saved.clone(), metadata: meta })
            .await;

        Ok(saved)
    }

    pub async fn get_by_id(&self, id: &str) -> ServiceResult<Option<E>> {
        self.repository
            .find_by_id(id)
            .await
            .map_err(ServiceError::Repository)
    }

    pub async fn update(&self, id: &str, dto: U) -> ServiceResult<Option<E>> {
        let existing = self
            .repository
            .find_by_id(id)
            .await
            .map_err(ServiceError::Repository)?;
        let Some(before) = existing else {
            return Ok(None);
        };
        let mut updated = before.clone().apply_update(dto)?;
        self.lifecycle.before_update(&mut updated).await?;
        let saved = self
            .repository
            .update(updated)
            .await
            .map_err(ServiceError::Repository)?;
        self.lifecycle.after_update(&saved).await?;

        let meta = EventMetadata::new(saved.entity_id(), std::any::type_name::<E>());
        let _ = self
            .event_publisher
            .publish(CrudEvent::Updated { before, after: saved.clone(), metadata: meta })
            .await;

        Ok(Some(saved))
    }

    pub async fn soft_delete(&self, id: &str) -> ServiceResult<bool> {
        let existing = self
            .repository
            .find_by_id(id)
            .await
            .map_err(ServiceError::Repository)?;
        let Some(entity) = existing else {
            return Ok(false);
        };
        self.lifecycle.before_delete(&entity).await?;
        let deleted = self
            .repository
            .soft_delete(id)
            .await
            .map_err(ServiceError::Repository)?;
        self.lifecycle.after_delete(id).await?;

        if deleted {
            let meta = EventMetadata::new(id, std::any::type_name::<E>());
            let _ = self
                .event_publisher
                .publish(CrudEvent::SoftDeleted { entity, metadata: meta })
                .await;
        }

        Ok(deleted)
    }

    pub async fn restore(&self, id: &str) -> ServiceResult<Option<E>> {
        let restored = self
            .repository
            .restore(id)
            .await
            .map_err(ServiceError::Repository)?;

        if let Some(ref entity) = restored {
            let meta = EventMetadata::new(entity.entity_id(), std::any::type_name::<E>());
            let _ = self
                .event_publisher
                .publish(CrudEvent::Restored { entity: entity.clone(), metadata: meta })
                .await;
        }

        Ok(restored)
    }

    pub async fn hard_delete(&self, id: &str) -> ServiceResult<bool> {
        let deleted = self
            .repository
            .hard_delete(id)
            .await
            .map_err(ServiceError::Repository)?;

        if deleted {
            let meta = EventMetadata::new(id, std::any::type_name::<E>());
            let _ = self
                .event_publisher
                .publish(CrudEvent::HardDeleted {
                    entity_id: id.to_string(),
                    metadata: meta,
                })
                .await;
        }

        Ok(deleted)
    }

    pub async fn list_deleted(
        &self,
        page: u32,
        limit: u32,
    ) -> ServiceResult<(Vec<E>, u64)> {
        self.repository
            .list_deleted(page, limit)
            .await
            .map_err(ServiceError::Repository)
    }

    pub async fn empty_trash(&self) -> ServiceResult<u64> {
        self.repository
            .empty_trash()
            .await
            .map_err(ServiceError::Repository)
    }

    pub async fn count(&self) -> ServiceResult<u64> {
        self.repository
            .count()
            .await
            .map_err(ServiceError::Repository)
    }

    pub async fn count_active(&self) -> ServiceResult<u64> {
        self.repository
            .count()
            .await
            .map_err(ServiceError::Repository)
    }

    /// Alias for `get_by_id` — used by generated handlers.
    pub async fn find_by_id(&self, id: &str) -> ServiceResult<Option<E>> {
        self.get_by_id(id).await
    }

    /// Alias for `hard_delete` — used by generated handlers.
    pub async fn permanent_delete(&self, id: &str) -> ServiceResult<bool> {
        self.hard_delete(id).await
    }

    /// Count soft-deleted entities.
    pub async fn count_deleted(&self) -> ServiceResult<u64> {
        self.repository
            .count_deleted()
            .await
            .map_err(ServiceError::Repository)
    }

    /// Retrieve a soft-deleted entity by ID (includes deleted records).
    pub async fn get_deleted_by_id(&self, id: &str) -> ServiceResult<Option<E>> {
        self.repository
            .find_by_id_including_deleted(id)
            .await
            .map_err(ServiceError::Repository)
    }

    /// List deleted entities with filter support (filters currently ignored, delegates to list_deleted).
    pub async fn list_deleted_filtered(
        &self,
        page: u32,
        limit: u32,
        _filters: HashMap<String, String>,
    ) -> ServiceResult<(Vec<E>, u64)> {
        self.list_deleted(page, limit).await
    }

    /// Upsert: create the entity if it doesn't exist (no ID-based lookup — always creates).
    pub async fn upsert(&self, dto: C) -> ServiceResult<E> {
        self.create(dto).await
    }

    /// Apply a partial update from a JSON map of field values.
    ///
    /// Fields not present in the map are left unchanged.
    pub async fn partial_update(
        &self,
        id: &str,
        fields: HashMap<String, serde_json::Value>,
    ) -> ServiceResult<Option<E>>
    where
        E: serde::Serialize + serde::de::DeserializeOwned,
    {
        let existing = self
            .repository
            .find_by_id(id)
            .await
            .map_err(ServiceError::Repository)?;
        let Some(entity) = existing else {
            return Ok(None);
        };
        // Merge the patch fields into the entity via JSON round-trip
        let mut map = serde_json::to_value(&entity)
            .map_err(|e| ServiceError::Internal(e.to_string()))?;
        if let serde_json::Value::Object(ref mut obj) = map {
            for (key, value) in fields {
                obj.insert(key, value);
            }
        }
        let mut patched: E = serde_json::from_value(map)
            .map_err(|e| ServiceError::Internal(e.to_string()))?;
        self.lifecycle.before_update(&mut patched).await?;
        let saved = self
            .repository
            .update(patched)
            .await
            .map_err(ServiceError::Repository)?;
        self.lifecycle.after_update(&saved).await?;
        Ok(Some(saved))
    }

    pub async fn bulk_create(&self, dtos: Vec<C>) -> ServiceResult<Vec<E>>
    where
        E: FromCreateDto<C>,
    {
        let mut results = Vec::with_capacity(dtos.len());
        for dto in dtos {
            let entity = self.create(dto).await?;
            results.push(entity);
        }
        Ok(results)
    }

    // ── Atomic batch operations ───────────────────────────────────────────────
    //
    // All-or-nothing: ids are validated up-front (so a missing id fails the whole
    // request before any write), then the repository applies the change inside a
    // single transaction. Lifecycle hooks and CRUD events fire per affected
    // entity, mirroring the single-row methods. Events are best-effort.

    /// Soft-delete many entities by id (atomic).
    pub async fn bulk_soft_delete(&self, ids: Vec<String>) -> ServiceResult<u64> {
        check_batch_size(ids.len())?;
        let ids = dedup_ids(ids);
        // Load the entities up-front: they are the payloads for the lifecycle
        // `before_delete` hook and the `SoftDeleted` event below. The repository's
        // atomic affected-row check (not this loop) is what guarantees the write
        // is all-or-nothing.
        let mut entities = Vec::with_capacity(ids.len());
        for id in &ids {
            match self
                .repository
                .find_by_id(id)
                .await
                .map_err(ServiceError::Repository)?
            {
                Some(e) => entities.push(e),
                None => return Err(ServiceError::Validation(format!("id '{id}' not found"))),
            }
        }
        for entity in &entities {
            self.lifecycle.before_delete(entity).await?;
        }
        let affected = self
            .repository
            .bulk_soft_delete(&ids)
            .await
            .map_err(ServiceError::Repository)?;
        for (id, entity) in ids.iter().zip(entities.into_iter()) {
            self.lifecycle.after_delete(id).await?;
            let meta = EventMetadata::new(id, std::any::type_name::<E>());
            let _ = self
                .event_publisher
                .publish(CrudEvent::SoftDeleted { entity, metadata: meta })
                .await;
        }
        Ok(affected)
    }

    /// Restore many soft-deleted entities by id (atomic).
    pub async fn bulk_restore(&self, ids: Vec<String>) -> ServiceResult<Vec<E>> {
        check_batch_size(ids.len())?;
        let ids = dedup_ids(ids);
        let restored = self
            .repository
            .bulk_restore(&ids)
            .await
            .map_err(ServiceError::Repository)?;
        for entity in &restored {
            let meta = EventMetadata::new(entity.entity_id(), std::any::type_name::<E>());
            let _ = self
                .event_publisher
                .publish(CrudEvent::Restored { entity: entity.clone(), metadata: meta })
                .await;
        }
        Ok(restored)
    }

    /// Restore every soft-deleted entity (atomic). Returns the number restored.
    /// Emits a `Restored` event per entity, mirroring [`bulk_restore`](Self::bulk_restore).
    pub async fn restore_all(&self) -> ServiceResult<u64> {
        let restored = self
            .repository
            .restore_all()
            .await
            .map_err(ServiceError::Repository)?;
        for entity in &restored {
            let meta = EventMetadata::new(entity.entity_id(), std::any::type_name::<E>());
            let _ = self
                .event_publisher
                .publish(CrudEvent::Restored { entity: entity.clone(), metadata: meta })
                .await;
        }
        Ok(restored.len() as u64)
    }

    /// Permanently delete many soft-deleted entities by id (atomic).
    pub async fn bulk_permanent_delete(&self, ids: Vec<String>) -> ServiceResult<u64> {
        check_batch_size(ids.len())?;
        let ids = dedup_ids(ids);
        // No pre-validation loop: `bulk_hard_delete` deletes only rows that are
        // actually in trash and rolls the whole batch back unless every id
        // matched, so the repository's atomic check is the single source of truth
        // (and avoids a per-id round-trip plus a TOCTOU window).
        let affected = self
            .repository
            .bulk_hard_delete(&ids)
            .await
            .map_err(ServiceError::Repository)?;
        for id in &ids {
            let meta = EventMetadata::new(id, std::any::type_name::<E>());
            let _ = self
                .event_publisher
                .publish(CrudEvent::HardDeleted {
                    entity_id: id.to_string(),
                    metadata: meta,
                })
                .await;
        }
        Ok(affected)
    }

    /// Full-update many entities (atomic). Each item is `(id, UpdateDto)`.
    pub async fn bulk_update(&self, items: Vec<(String, U)>) -> ServiceResult<Vec<E>> {
        check_batch_size(items.len())?;
        if let Some(dup) = first_duplicate_id(items.iter().map(|(id, _)| id.clone())) {
            return Err(ServiceError::Validation(format!(
                "duplicate id '{dup}' in bulk update"
            )));
        }
        let mut befores = Vec::with_capacity(items.len());
        let mut prepared = Vec::with_capacity(items.len());
        for (id, dto) in items {
            let Some(before) = self
                .repository
                .find_by_id(&id)
                .await
                .map_err(ServiceError::Repository)?
            else {
                return Err(ServiceError::Validation(format!("id '{id}' not found")));
            };
            let mut updated = before.clone().apply_update(dto)?;
            self.lifecycle.before_update(&mut updated).await?;
            befores.push(before);
            prepared.push(updated);
        }
        let saved = self
            .repository
            .bulk_update(prepared)
            .await
            .map_err(ServiceError::Repository)?;
        self.publish_bulk_updates(befores, &saved).await?;
        Ok(saved)
    }

    /// Fire `after_update` and a `Updated` event for each `(before, after)` pair.
    /// Shared by [`bulk_update`](Self::bulk_update) and
    /// [`bulk_partial_update`](Self::bulk_partial_update); events are best-effort.
    async fn publish_bulk_updates(&self, befores: Vec<E>, saved: &[E]) -> ServiceResult<()> {
        for (before, after) in befores.into_iter().zip(saved.iter()) {
            self.lifecycle.after_update(after).await?;
            let meta = EventMetadata::new(after.entity_id(), std::any::type_name::<E>());
            let _ = self
                .event_publisher
                .publish(CrudEvent::Updated {
                    before,
                    after: after.clone(),
                    metadata: meta,
                })
                .await;
        }
        Ok(())
    }

    /// Partial-update many entities (atomic). Each item is `(id, field_map)`.
    pub async fn bulk_partial_update(
        &self,
        items: Vec<(String, HashMap<String, serde_json::Value>)>,
    ) -> ServiceResult<Vec<E>>
    where
        E: serde::Serialize + serde::de::DeserializeOwned,
    {
        check_batch_size(items.len())?;
        if let Some(dup) = first_duplicate_id(items.iter().map(|(id, _)| id.clone())) {
            return Err(ServiceError::Validation(format!(
                "duplicate id '{dup}' in bulk partial update"
            )));
        }
        let mut befores = Vec::with_capacity(items.len());
        let mut prepared = Vec::with_capacity(items.len());
        for (id, fields) in items {
            let Some(before) = self
                .repository
                .find_by_id(&id)
                .await
                .map_err(ServiceError::Repository)?
            else {
                return Err(ServiceError::Validation(format!("id '{id}' not found")));
            };
            let mut map = serde_json::to_value(&before)
                .map_err(|e| ServiceError::Internal(e.to_string()))?;
            if let serde_json::Value::Object(ref mut obj) = map {
                for (key, value) in fields {
                    obj.insert(key, value);
                }
            }
            let mut patched: E = serde_json::from_value(map)
                .map_err(|e| ServiceError::Internal(e.to_string()))?;
            self.lifecycle.before_update(&mut patched).await?;
            befores.push(before);
            prepared.push(patched);
        }
        let saved = self
            .repository
            .bulk_update(prepared)
            .await
            .map_err(ServiceError::Repository)?;
        self.publish_bulk_updates(befores, &saved).await?;
        Ok(saved)
    }
}

/// Maximum number of ids / items a single batch operation may contain. Enforced
/// here at the service layer (not only in the HTTP handlers) so every caller —
/// gRPC, background jobs, internal code — gets the same bound.
pub const MAX_BATCH_SIZE: usize = 1000;

/// Reject an over-sized batch before any work is done.
fn check_batch_size(count: usize) -> Result<(), ServiceError> {
    if count > MAX_BATCH_SIZE {
        return Err(ServiceError::Validation(format!(
            "Batch too large: {count} items exceeds the maximum of {MAX_BATCH_SIZE}."
        )));
    }
    Ok(())
}

/// Drop duplicate ids while preserving first-seen order. Id-list batch ops issue
/// a SQL `IN (...)` whose distinct-row count would never match a list that
/// repeats an id, so a stray duplicate would otherwise fail the whole batch.
fn dedup_ids(ids: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    ids.into_iter().filter(|id| seen.insert(id.clone())).collect()
}

/// Return the first id that appears more than once, if any. Used to reject
/// ambiguous bulk updates (the same id mapped to two different payloads).
fn first_duplicate_id(ids: impl IntoIterator<Item = String>) -> Option<String> {
    let mut seen = std::collections::HashSet::new();
    for id in ids {
        if !seen.insert(id.clone()) {
            return Some(id);
        }
    }
    None
}

// ─── Blanket CrudService impl ────────────────────────────────────────────────

/// Blanket impl so that any `GenericCrudService<E, C, U, R>` automatically
/// satisfies `CrudService<E, C, U>` without generated adapter structs.
///
/// Generated code emits only a type alias:
///   `pub type UserService = GenericCrudService<User, CreateUserDto, UpdateUserDto, UserRepository>;`
///
/// That alias now directly implements `CrudService` and can be passed to
/// `BackboneCrudHandler` without any additional boilerplate.
#[async_trait::async_trait]
impl<E, C, U, R> crate::http::CrudService<E, C, U> for GenericCrudService<E, C, U, R>
where
    E: PersistentEntity
        + Clone
        + serde::Serialize
        + serde::de::DeserializeOwned
        + FromCreateDto<C>
        + ApplyUpdateDto<U>
        + Send
        + Sync
        + 'static,
    C: Send + Sync + 'static,
    U: Send + Sync + 'static,
    R: CrudRepository<E> + Send + Sync + 'static,
{
    type Error = ServiceError;

    fn entity_name() -> &'static str {
        std::any::type_name::<E>()
    }

    async fn list(
        &self,
        page: u32,
        limit: u32,
        filters: HashMap<String, String>,
    ) -> Result<(Vec<E>, u64), ServiceError> {
        self.list(page, limit, filters).await
    }

    async fn create(&self, dto: C) -> Result<E, ServiceError> {
        self.create(dto).await
    }

    async fn get_by_id(&self, id: &str) -> Result<Option<E>, ServiceError> {
        self.get_by_id(id).await
    }

    async fn update(&self, id: &str, dto: U) -> Result<Option<E>, ServiceError> {
        self.update(id, dto).await
    }

    async fn partial_update(
        &self,
        id: &str,
        fields: HashMap<String, serde_json::Value>,
    ) -> Result<Option<E>, ServiceError> {
        self.partial_update(id, fields).await
    }

    async fn soft_delete(&self, id: &str) -> Result<bool, ServiceError> {
        self.soft_delete(id).await
    }

    async fn bulk_create(&self, items: Vec<C>) -> Result<Vec<E>, ServiceError> {
        self.bulk_create(items).await
    }

    async fn upsert(&self, dto: C) -> Result<E, ServiceError> {
        self.upsert(dto).await
    }

    async fn list_deleted(&self, page: u32, limit: u32) -> Result<(Vec<E>, u64), ServiceError> {
        self.list_deleted(page, limit).await
    }

    async fn restore(&self, id: &str) -> Result<Option<E>, ServiceError> {
        self.restore(id).await
    }

    async fn empty_trash(&self) -> Result<u64, ServiceError> {
        self.empty_trash().await
    }

    async fn get_deleted_by_id(&self, id: &str) -> Result<Option<E>, ServiceError> {
        self.get_deleted_by_id(id).await
    }

    async fn permanent_delete(&self, id: &str) -> Result<bool, ServiceError> {
        self.permanent_delete(id).await
    }

    async fn list_deleted_filtered(
        &self,
        page: u32,
        limit: u32,
        filters: HashMap<String, String>,
    ) -> Result<(Vec<E>, u64), ServiceError> {
        self.list_deleted_filtered(page, limit, filters).await
    }

    async fn count_active(&self) -> Result<u64, ServiceError> {
        self.count_active().await
    }

    async fn count_deleted(&self) -> Result<u64, ServiceError> {
        self.count_deleted().await
    }

    async fn bulk_soft_delete(&self, ids: Vec<String>) -> Result<u64, ServiceError> {
        self.bulk_soft_delete(ids).await
    }

    async fn bulk_restore(&self, ids: Vec<String>) -> Result<Vec<E>, ServiceError> {
        self.bulk_restore(ids).await
    }

    async fn bulk_permanent_delete(&self, ids: Vec<String>) -> Result<u64, ServiceError> {
        self.bulk_permanent_delete(ids).await
    }

    async fn restore_all(&self) -> Result<u64, ServiceError> {
        self.restore_all().await
    }

    async fn bulk_update(&self, items: Vec<(String, U)>) -> Result<Vec<E>, ServiceError> {
        self.bulk_update(items).await
    }

    async fn bulk_partial_update(
        &self,
        items: Vec<(String, HashMap<String, serde_json::Value>)>,
    ) -> Result<Vec<E>, ServiceError> {
        self.bulk_partial_update(items).await
    }
}

// Make the service cloneable (needed when Arc-ing it and sharing across handlers).
impl<E, C, U, R> Clone for GenericCrudService<E, C, U, R>
where
    E: PersistentEntity + Clone,
    R: CrudRepository<E>,
{
    fn clone(&self) -> Self {
        Self {
            repository: self.repository.clone(),
            lifecycle: self.lifecycle.clone(),
            event_publisher: self.event_publisher.clone(),
            _phantom: PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};

    // ── Minimal in-memory repository for testing ──────────────────────────

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct Widget {
        id: String,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        deleted_at: Option<DateTime<Utc>>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    }

    impl crate::persistence::traits::PersistentEntity for Widget {
        fn entity_id(&self) -> String {
            self.id.clone()
        }
        fn set_entity_id(&mut self, id: String) {
            self.id = id;
        }
        fn created_at(&self) -> Option<DateTime<Utc>> {
            Some(self.created_at)
        }
        fn set_created_at(&mut self, ts: DateTime<Utc>) {
            self.created_at = ts;
        }
        fn updated_at(&self) -> Option<DateTime<Utc>> {
            Some(self.updated_at)
        }
        fn set_updated_at(&mut self, ts: DateTime<Utc>) {
            self.updated_at = ts;
        }
        fn deleted_at(&self) -> Option<DateTime<Utc>> {
            self.deleted_at
        }
        fn set_deleted_at(&mut self, ts: Option<DateTime<Utc>>) {
            self.deleted_at = ts;
        }
    }

    struct CreateWidgetDto {
        name: String,
    }

    struct UpdateWidgetDto {
        name: String,
    }

    impl FromCreateDto<CreateWidgetDto> for Widget {
        fn from_create_dto(dto: CreateWidgetDto) -> ServiceResult<Self> {
            Ok(Widget {
                id: uuid::Uuid::new_v4().to_string(),
                name: dto.name,
                deleted_at: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
        }
    }

    impl ApplyUpdateDto<UpdateWidgetDto> for Widget {
        fn apply_update(mut self, dto: UpdateWidgetDto) -> ServiceResult<Self> {
            self.name = dto.name;
            self.updated_at = Utc::now();
            Ok(self)
        }
    }

    // Minimal in-memory CrudRepository
    struct InMemoryWidgetRepo {
        data: tokio::sync::Mutex<Vec<Widget>>,
    }

    impl InMemoryWidgetRepo {
        fn new() -> Self {
            Self {
                data: tokio::sync::Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl crate::persistence::traits::CrudRepository<Widget> for InMemoryWidgetRepo {
        async fn create(&self, entity: Widget) -> Result<Widget, RepositoryError> {
            self.data.lock().await.push(entity.clone());
            Ok(entity)
        }

        async fn find_by_id(&self, id: &str) -> Result<Option<Widget>, RepositoryError> {
            Ok(self
                .data
                .lock()
                .await
                .iter()
                .find(|w| w.id == id && w.deleted_at.is_none())
                .cloned())
        }

        async fn update(&self, entity: Widget) -> Result<Widget, RepositoryError> {
            let mut data = self.data.lock().await;
            data.retain(|w| w.id != entity.id);
            data.push(entity.clone());
            Ok(entity)
        }

        async fn soft_delete(&self, id: &str) -> Result<bool, RepositoryError> {
            let mut data = self.data.lock().await;
            if let Some(w) = data.iter_mut().find(|w| w.id == id) {
                w.deleted_at = Some(Utc::now());
                return Ok(true);
            }
            Ok(false)
        }

        async fn find_by_id_including_deleted(&self, id: &str) -> Result<Option<Widget>, RepositoryError> {
            Ok(self.data.lock().await.iter().find(|w| w.id == id).cloned())
        }

        async fn restore(&self, id: &str) -> Result<Option<Widget>, RepositoryError> {
            let mut data = self.data.lock().await;
            if let Some(w) = data.iter_mut().find(|w| w.id == id) {
                w.deleted_at = None;
                return Ok(Some(w.clone()));
            }
            Ok(None)
        }

        async fn hard_delete(&self, id: &str) -> Result<bool, RepositoryError> {
            let mut data = self.data.lock().await;
            let before = data.len();
            data.retain(|w| w.id != id);
            Ok(data.len() < before)
        }

        async fn list(
            &self,
            page: u32,
            limit: u32,
        ) -> Result<(Vec<Widget>, u64), RepositoryError> {
            let data = self.data.lock().await;
            let active: Vec<_> = data.iter().filter(|w| w.deleted_at.is_none()).cloned().collect();
            let total = active.len() as u64;
            let offset = ((page.saturating_sub(1)) as usize) * (limit as usize);
            let limit = limit as usize;
            let page = active.into_iter().skip(offset).take(limit).collect();
            Ok((page, total))
        }

        async fn list_deleted(
            &self,
            page: u32,
            limit: u32,
        ) -> Result<(Vec<Widget>, u64), RepositoryError> {
            let data = self.data.lock().await;
            let deleted: Vec<_> = data.iter().filter(|w| w.deleted_at.is_some()).cloned().collect();
            let total = deleted.len() as u64;
            let offset = ((page.saturating_sub(1)) as usize) * (limit as usize);
            let limit = limit as usize;
            let page = deleted.into_iter().skip(offset).take(limit).collect();
            Ok((page, total))
        }

        async fn count(&self) -> Result<u64, RepositoryError> {
            let data = self.data.lock().await;
            Ok(data.iter().filter(|w| w.deleted_at.is_none()).count() as u64)
        }

        async fn count_deleted(&self) -> Result<u64, RepositoryError> {
            let data = self.data.lock().await;
            Ok(data.iter().filter(|w| w.deleted_at.is_some()).count() as u64)
        }

        async fn bulk_create(&self, entities: Vec<Widget>) -> Result<Vec<Widget>, RepositoryError> {
            let mut data = self.data.lock().await;
            data.extend(entities.clone());
            Ok(entities)
        }

        async fn empty_trash(&self) -> Result<u64, RepositoryError> {
            let mut data = self.data.lock().await;
            let before = data.len();
            data.retain(|w| w.deleted_at.is_none());
            Ok((before - data.len()) as u64)
        }
    }

    #[tokio::test]
    async fn create_and_get_roundtrip() {
        let repo = Arc::new(InMemoryWidgetRepo::new());
        let service: GenericCrudService<Widget, CreateWidgetDto, UpdateWidgetDto, InMemoryWidgetRepo> =
            GenericCrudService::with_repository(repo);

        let widget = service
            .create(CreateWidgetDto {
                name: "sprocket".into(),
            })
            .await
            .unwrap();

        let found = service.get_by_id(&widget.id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "sprocket");
    }

    #[tokio::test]
    async fn soft_delete_hides_from_list() {
        let repo = Arc::new(InMemoryWidgetRepo::new());
        let service: GenericCrudService<Widget, CreateWidgetDto, UpdateWidgetDto, InMemoryWidgetRepo> =
            GenericCrudService::with_repository(repo);

        let w = service.create(CreateWidgetDto { name: "w".into() }).await.unwrap();
        service.soft_delete(&w.id).await.unwrap();

        let (items, _) = service.list(1, 20, Default::default()).await.unwrap();
        assert!(items.is_empty());

        let (deleted, _) = service.list_deleted(1, 20).await.unwrap();
        assert_eq!(deleted.len(), 1);
    }

    #[tokio::test]
    async fn update_publishes_event_and_returns_new_entity() {
        let repo = Arc::new(InMemoryWidgetRepo::new());
        let service: GenericCrudService<Widget, CreateWidgetDto, UpdateWidgetDto, InMemoryWidgetRepo> =
            GenericCrudService::with_repository(repo);

        let w = service.create(CreateWidgetDto { name: "old".into() }).await.unwrap();
        let updated = service
            .update(&w.id, UpdateWidgetDto { name: "new".into() })
            .await
            .unwrap();
        assert_eq!(updated.unwrap().name, "new");
    }

    // ── Batch operations ──────────────────────────────────────────────────────

    fn svc() -> GenericCrudService<Widget, CreateWidgetDto, UpdateWidgetDto, InMemoryWidgetRepo> {
        GenericCrudService::with_repository(Arc::new(InMemoryWidgetRepo::new()))
    }

    #[tokio::test]
    async fn bulk_soft_delete_success() {
        let service = svc();
        let mut ids = Vec::new();
        for n in 0..3 {
            ids.push(service.create(CreateWidgetDto { name: format!("w{n}") }).await.unwrap().id);
        }
        let n = service.bulk_soft_delete(ids).await.unwrap();
        assert_eq!(n, 3);
        let (active, _) = service.list(1, 20, Default::default()).await.unwrap();
        assert!(active.is_empty());
    }

    #[tokio::test]
    async fn bulk_soft_delete_is_all_or_nothing_on_missing_id() {
        let service = svc();
        let a = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap();
        let b = service.create(CreateWidgetDto { name: "b".into() }).await.unwrap();

        // One id is missing → whole batch must be rejected with nothing deleted.
        let res = service
            .bulk_soft_delete(vec![a.id.clone(), b.id.clone(), "does-not-exist".into()])
            .await;
        assert!(res.is_err());

        let (active, _) = service.list(1, 20, Default::default()).await.unwrap();
        assert_eq!(active.len(), 2, "no rows should have been deleted");
    }

    #[tokio::test]
    async fn bulk_restore_and_restore_all() {
        let service = svc();
        let mut ids = Vec::new();
        for n in 0..3 {
            ids.push(service.create(CreateWidgetDto { name: format!("w{n}") }).await.unwrap().id);
        }
        service.bulk_soft_delete(ids.clone()).await.unwrap();

        // Restore two explicitly.
        let restored = service.bulk_restore(vec![ids[0].clone(), ids[1].clone()]).await.unwrap();
        assert_eq!(restored.len(), 2);

        // Restore the remaining one via restore_all.
        let n = service.restore_all().await.unwrap();
        assert_eq!(n, 1);

        let (active, _) = service.list(1, 20, Default::default()).await.unwrap();
        assert_eq!(active.len(), 3);
    }

    #[tokio::test]
    async fn bulk_update_is_all_or_nothing_on_missing_id() {
        let service = svc();
        let a = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap();

        let res = service
            .bulk_update(vec![
                (a.id.clone(), UpdateWidgetDto { name: "A2".into() }),
                ("missing".into(), UpdateWidgetDto { name: "X".into() }),
            ])
            .await;
        assert!(res.is_err());

        // The valid row must be untouched because the batch aborted pre-write.
        let still = service.get_by_id(&a.id).await.unwrap().unwrap();
        assert_eq!(still.name, "a");
    }

    #[tokio::test]
    async fn bulk_partial_update_success() {
        let service = svc();
        let a = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap();
        let b = service.create(CreateWidgetDto { name: "b".into() }).await.unwrap();

        let mut patch_a = std::collections::HashMap::new();
        patch_a.insert("name".to_string(), serde_json::json!("a2"));
        let mut patch_b = std::collections::HashMap::new();
        patch_b.insert("name".to_string(), serde_json::json!("b2"));

        let saved = service
            .bulk_partial_update(vec![(a.id.clone(), patch_a), (b.id.clone(), patch_b)])
            .await
            .unwrap();
        assert_eq!(saved.len(), 2);
        assert_eq!(service.get_by_id(&a.id).await.unwrap().unwrap().name, "a2");
        assert_eq!(service.get_by_id(&b.id).await.unwrap().unwrap().name, "b2");
    }

    #[tokio::test]
    async fn bulk_soft_delete_tolerates_duplicate_ids() {
        let service = svc();
        let a = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap();
        let b = service.create(CreateWidgetDto { name: "b".into() }).await.unwrap();

        // A repeated id must not fail the batch: ids are de-duplicated first.
        let n = service
            .bulk_soft_delete(vec![a.id.clone(), a.id.clone(), b.id.clone()])
            .await
            .unwrap();
        assert_eq!(n, 2, "two distinct rows soft-deleted");

        let (active, _) = service.list(1, 20, Default::default()).await.unwrap();
        assert!(active.is_empty());
    }

    #[tokio::test]
    async fn bulk_update_rejects_duplicate_ids() {
        let service = svc();
        let a = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap();

        // The same id mapped to two payloads is ambiguous → rejected, nothing written.
        let res = service
            .bulk_update(vec![
                (a.id.clone(), UpdateWidgetDto { name: "first".into() }),
                (a.id.clone(), UpdateWidgetDto { name: "second".into() }),
            ])
            .await;
        assert!(res.is_err());
        assert_eq!(service.get_by_id(&a.id).await.unwrap().unwrap().name, "a");
    }

    #[tokio::test]
    async fn bulk_soft_delete_rejects_oversized_batch() {
        let service = svc();
        let ids: Vec<String> = (0..MAX_BATCH_SIZE + 1).map(|n| format!("id-{n}")).collect();
        // Enforced at the service layer, independent of any HTTP handler.
        assert!(service.bulk_soft_delete(ids).await.is_err());
    }
}
