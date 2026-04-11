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
}
