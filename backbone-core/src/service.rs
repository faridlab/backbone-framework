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
use crate::write_hooks::{Change, NoWriteHooks, WriteCtx, WriteHooks};

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

    /// The write broke named rules: one violation per rule, each with the
    /// field it concerns and a stable code. Displays as the same
    /// `validation failed: …` sentence a plain validation error does.
    #[error("validation failed: {}", .0.iter().map(|v| v.message.as_str()).collect::<Vec<_>>().join("; "))]
    Violations(Vec<crate::violation::Violation>),

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

    /// Called before a soft-deleted entity is restored, singly or in a batch.
    async fn before_restore(&self, entity: &E) -> ServiceResult<()> {
        let _ = entity;
        Ok(())
    }

    /// Called before a soft-deleted entity is removed for good, singly or in
    /// a batch.
    async fn before_hard_delete(&self, entity: &E) -> ServiceResult<()> {
        let _ = entity;
        Ok(())
    }

    /// Called once before the whole trash is restored. The rows are not
    /// loaded for this; refuse here to keep the bulk path closed.
    async fn before_restore_all(&self) -> ServiceResult<()> {
        Ok(())
    }

    /// Called once before the whole trash is removed for good. The rows are
    /// not loaded for this; refuse here to keep the bulk path closed.
    async fn before_empty_trash(&self) -> ServiceResult<()> {
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
    guard: Arc<dyn crate::write_guard::WriteGuard<E>>,
    hooks: Arc<dyn WriteHooks<E>>,
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
            guard: Arc::new(crate::write_guard::AllowAll),
            hooks: Arc::new(NoWriteHooks),
            _phantom: PhantomData,
        }
    }

    /// Check every write against `guard`: its refusals refuse the write, its
    /// shadowed violations are logged and the write proceeds.
    pub fn with_guard(mut self, guard: Arc<dyn crate::write_guard::WriteGuard<E>>) -> Self {
        self.guard = guard;
        self
    }

    /// Run `hooks` inside every write's transaction, before and after the repository write (see
    /// [`crate::write_hooks`]). Declared events are staged by the hooks the schema generator
    /// emits for a model.
    pub fn with_write_hooks(mut self, hooks: Arc<dyn WriteHooks<E>>) -> Self {
        self.hooks = hooks;
        self
    }

    /// The context a write's hooks receive.
    fn write_ctx(&self) -> WriteCtx {
        WriteCtx::current(self.hooks.outbox_schema())
    }

    /// Run `work` inside one write transaction on the repository's pool: committed when it
    /// returns `Ok`, rolled back when it fails or is dropped. A repository with no pool (an
    /// in-memory one) runs `work` as it is.
    ///
    /// Joined to the transaction already open for the task when there is one, as a savepoint:
    /// a hand-written verb that opened its own transaction and writes through this service
    /// keeps one transaction.
    async fn atomically<T>(
        &self,
        work: impl std::future::Future<Output = ServiceResult<T>>,
    ) -> ServiceResult<T> {
        let Some(pool) = self.repository.write_pool() else {
            return work.await;
        };
        match backbone_orm::in_write_tx_result(pool, work).await {
            Ok(value) => Ok(value),
            Err(backbone_orm::WriteTxFailure::Inner(e)) => Err(e),
            Err(backbone_orm::WriteTxFailure::Db(e)) => {
                Err(ServiceError::Repository(RepositoryError::from_source(e)))
            }
        }
    }

    /// Publish the events of a committed write, best effort and in process.
    async fn publish_all(&self, events: Vec<CrudEvent<E>>) {
        for event in events {
            let _ = self.event_publisher.publish(event).await;
        }
    }

    /// Ask the guard about one write.
    async fn consult(
        &self,
        kind: crate::write_guard::WriteKind,
        before: Option<&E>,
        after: Option<&E>,
    ) -> ServiceResult<()> {
        self.consult_as(kind, before, after, true).await
    }

    /// [`consult`](Self::consult) for a dry run: the same refusals, without
    /// logging what a shadow rule would refuse — the real write logs that, and a
    /// preview before it would count every shadow violation twice.
    async fn consult_quietly(
        &self,
        kind: crate::write_guard::WriteKind,
        before: Option<&E>,
        after: Option<&E>,
    ) -> ServiceResult<()> {
        self.consult_as(kind, before, after, false).await
    }

    async fn consult_as(
        &self,
        kind: crate::write_guard::WriteKind,
        before: Option<&E>,
        after: Option<&E>,
        log_shadow: bool,
    ) -> ServiceResult<()> {
        let ctx = crate::write_guard::WriteCtx::new(kind, before, after);
        let outcome = self.guard.check(&ctx).await;
        for v in outcome.shadow.iter().filter(|_| log_shadow) {
            tracing::warn!(
                target: "backbone::write_guard",
                entity = std::any::type_name::<E>(),
                kind = ?kind,
                code = %v.code,
                path = %v.path,
                "a rule running in shadow would refuse this write: {}",
                v.message
            );
        }
        if outcome.refuse.is_empty() {
            Ok(())
        } else {
            Err(ServiceError::Violations(outcome.refuse))
        }
    }

    /// Refuse a write that points one of the entity's references
    /// ([`PersistentEntity::references`]) at a row the caller cannot see.
    ///
    /// Each reference the write sets — every one on a create, those whose value changes on
    /// an update — is looked up through the repository on the request's own scope
    /// ([`CrudRepository::reference_visible`]). A row that is missing or hidden by row-level
    /// security answers `reference_missing` on the field, worded as the database's own
    /// missing-reference answer is, so the refusal never tells that the row exists
    /// elsewhere. A null reference and a repository that cannot tell are let through to the
    /// database's key.
    async fn check_references(&self, before: Option<&E>, after: &E) -> ServiceResult<()> {
        let references = E::references();
        if references.is_empty() {
            return Ok(());
        }
        let after_obj = as_object(after)?;
        let before_obj = before.map(as_object).transpose()?;
        let mut refused = Vec::new();
        for reference in references {
            let Some(value) = after_obj.get(reference.field).filter(|v| !v.is_null()) else {
                continue;
            };
            if before_obj.as_ref().is_some_and(|b| b.get(reference.field) == Some(value)) {
                continue;
            }
            let visible = self
                .repository
                .reference_visible(reference.table, reference.column, value)
                .await
                .map_err(ServiceError::Repository)?;
            if visible == Some(false) {
                refused.push(crate::violation::Violation::new(
                    reference.field,
                    "reference_missing",
                    format!("{} refers to a record that does not exist", reference.field),
                ));
            }
        }
        if refused.is_empty() {
            Ok(())
        } else {
            Err(ServiceError::Violations(refused))
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

    /// `list`, carrying the pagination info (cursor positions included) so
    /// the HTTP layer can surface keyset paging on every generic route.
    pub async fn list_with_info(
        &self,
        page: u32,
        limit: u32,
        filters: HashMap<String, String>,
    ) -> ServiceResult<(Vec<E>, backbone_orm::repository::PaginationInfo)> {
        self.repository
            .list_filtered_with_info(page, limit, filters)
            .await
            .map_err(ServiceError::Repository)
    }

    /// Group and reduce the rows `list` would return, under the same filters.
    pub async fn aggregate(
        &self,
        spec: &backbone_orm::repository::AggregateSpec,
        filters: HashMap<String, String>,
    ) -> ServiceResult<backbone_orm::repository::AggregateResult> {
        self.repository
            .aggregate_filtered(spec, filters)
            .await
            .map_err(ServiceError::Repository)
    }

    pub async fn create(&self, dto: C) -> ServiceResult<E> {
        let (saved, events) = self
            .atomically(async {
                let ctx = self.write_ctx();
                let mut entity = E::from_create_dto(dto)?;
                self.consult(crate::write_guard::WriteKind::Create, None, Some(&entity)).await?;
                self.lifecycle.before_create(&mut entity).await?;
                self.hooks.before(&ctx, &mut Change::Created { after: &mut entity }).await?;
                self.check_references(None, &entity).await?;
                let mut saved = self
                    .repository
                    .create(entity)
                    .await
                    .map_err(ServiceError::Repository)?;
                self.lifecycle.after_create(&saved).await?;
                self.hooks.after(&ctx, &Change::Created { after: &mut saved }).await?;

                let meta = EventMetadata::new(saved.entity_id(), std::any::type_name::<E>());
                let event = CrudEvent::Created { entity: saved.clone(), metadata: meta };
                Ok((saved, vec![event]))
            })
            .await?;
        self.publish_all(events).await;
        Ok(saved)
    }

    pub async fn get_by_id(&self, id: &str) -> ServiceResult<Option<E>> {
        self.repository
            .find_by_id(id)
            .await
            .map_err(ServiceError::Repository)
    }

    pub async fn update(&self, id: &str, dto: U) -> ServiceResult<Option<E>> {
        let written = self
            .atomically(async {
                let ctx = self.write_ctx();
                let existing = self
                    .repository
                    .find_by_id(id)
                    .await
                    .map_err(ServiceError::Repository)?;
                let Some(before) = existing else {
                    return Ok(None);
                };
                let mut updated = before.clone().apply_update(dto)?;
                refuse_protected_changes::<E>(&as_object(&before)?, &as_object(&updated)?)?;
                self.consult(crate::write_guard::WriteKind::Update, Some(&before), Some(&updated)).await?;
                self.lifecycle.before_update(&mut updated).await?;
                self.hooks.before(&ctx, &mut Change::update(&before, &mut updated)).await?;
                self.check_references(Some(&before), &updated).await?;
                // The same changed-fields, live-row-only save as `partial_update`.
                let Some(mut saved) = self
                    .repository
                    .update_changed(&before, updated)
                    .await
                    .map_err(ServiceError::Repository)?
                else {
                    return Ok(None);
                };
                self.lifecycle.after_update(&saved).await?;
                self.hooks.after(&ctx, &Change::update(&before, &mut saved)).await?;

                let meta = EventMetadata::new(saved.entity_id(), std::any::type_name::<E>());
                let event = CrudEvent::Updated { before, after: saved.clone(), metadata: meta };
                Ok(Some((saved, vec![event])))
            })
            .await?;
        let Some((saved, events)) = written else {
            return Ok(None);
        };
        self.publish_all(events).await;
        Ok(Some(saved))
    }

    pub async fn soft_delete(&self, id: &str) -> ServiceResult<bool> {
        let (deleted, events) = self
            .atomically(async {
                let ctx = self.write_ctx();
                let existing = self
                    .repository
                    .find_by_id(id)
                    .await
                    .map_err(ServiceError::Repository)?;
                let Some(entity) = existing else {
                    return Ok((false, Vec::new()));
                };
                self.consult(crate::write_guard::WriteKind::Delete, Some(&entity), None).await?;
                self.lifecycle.before_delete(&entity).await?;
                self.hooks
                    .before(&ctx, &mut Change::Deleted { before: &entity, hard: false })
                    .await?;
                let deleted = self
                    .repository
                    .soft_delete(id)
                    .await
                    .map_err(ServiceError::Repository)?;
                self.lifecycle.after_delete(id).await?;
                if !deleted {
                    return Ok((false, Vec::new()));
                }
                self.hooks
                    .after(&ctx, &Change::Deleted { before: &entity, hard: false })
                    .await?;

                let meta = EventMetadata::new(id, std::any::type_name::<E>());
                Ok((true, vec![CrudEvent::SoftDeleted { entity, metadata: meta }]))
            })
            .await?;
        self.publish_all(events).await;
        Ok(deleted)
    }

    pub async fn restore(&self, id: &str) -> ServiceResult<Option<E>> {
        let (restored, events) = self
            .atomically(async {
                let ctx = self.write_ctx();
                let Some(trashed) = self.get_deleted_by_id(id).await? else {
                    return Ok((None, Vec::new()));
                };
                self.consult(crate::write_guard::WriteKind::Restore, Some(&trashed), None).await?;
                self.lifecycle.before_restore(&trashed).await?;
                self.hooks.before(&ctx, &mut Change::Restored { entity: &trashed }).await?;
                let restored = self
                    .repository
                    .restore(id)
                    .await
                    .map_err(ServiceError::Repository)?;

                let mut events = Vec::new();
                if let Some(ref entity) = restored {
                    self.hooks.after(&ctx, &Change::Restored { entity }).await?;
                    let meta = EventMetadata::new(entity.entity_id(), std::any::type_name::<E>());
                    events.push(CrudEvent::Restored { entity: entity.clone(), metadata: meta });
                }
                Ok((restored, events))
            })
            .await?;
        self.publish_all(events).await;
        Ok(restored)
    }

    pub async fn hard_delete(&self, id: &str) -> ServiceResult<bool> {
        let (deleted, events) = self
            .atomically(async {
                let ctx = self.write_ctx();
                let Some(entity) = self.get_deleted_by_id(id).await? else {
                    return Ok((false, Vec::new()));
                };
                self.consult(crate::write_guard::WriteKind::HardDelete, Some(&entity), None).await?;
                self.lifecycle.before_hard_delete(&entity).await?;
                self.hooks
                    .before(&ctx, &mut Change::Deleted { before: &entity, hard: true })
                    .await?;
                let deleted = self
                    .repository
                    .hard_delete(id)
                    .await
                    .map_err(ServiceError::Repository)?;
                if !deleted {
                    return Ok((false, Vec::new()));
                }
                self.hooks
                    .after(&ctx, &Change::Deleted { before: &entity, hard: true })
                    .await?;

                let meta = EventMetadata::new(id, std::any::type_name::<E>());
                let event = CrudEvent::HardDeleted { entity_id: id.to_string(), metadata: meta };
                Ok((true, vec![event]))
            })
            .await?;
        self.publish_all(events).await;
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

    /// Permanently delete everything in the trash (atomic).
    ///
    /// The trashed rows are loaded first, at most [`MAX_BATCH_SIZE`] of them (a larger trash is
    /// refused: empty it in batches by id), and the write guard and lifecycle are asked about
    /// each one as a single permanent delete would. The delete then names exactly those ids and
    /// rolls back unless every one is still in the trash, so a row trashed after the check is
    /// never deleted unchecked.
    pub async fn empty_trash(&self) -> ServiceResult<u64> {
        let (affected, events) = self
            .atomically(async {
                let ctx = self.write_ctx();
                self.lifecycle.before_empty_trash().await?;
                let trashed = self.whole_trash("emptied").await?;
                if trashed.is_empty() {
                    return Ok((0, Vec::new()));
                }
                for entity in &trashed {
                    self.consult(crate::write_guard::WriteKind::HardDelete, Some(entity), None).await?;
                    self.lifecycle.before_hard_delete(entity).await?;
                    self.hooks
                        .before(&ctx, &mut Change::Deleted { before: entity, hard: true })
                        .await?;
                }
                let ids: Vec<String> = trashed.iter().map(|e| e.entity_id()).collect();
                let affected = self
                    .repository
                    .bulk_hard_delete(&ids)
                    .await
                    .map_err(ServiceError::Repository)?;
                for entity in &trashed {
                    self.hooks
                        .after(&ctx, &Change::Deleted { before: entity, hard: true })
                        .await?;
                }
                Ok((affected, hard_deleted_events::<E>(&ids)))
            })
            .await?;
        self.publish_all(events).await;
        Ok(affected)
    }

    /// Every row in the trash, refused when there are more than one batch may hold.
    ///
    /// Read page by page: a repository caps the rows one page may hold
    /// ([`backbone_orm::repository::PaginationParams::MAX_PER_PAGE`]), so a single page
    /// of [`MAX_BATCH_SIZE`] would quietly hold only the first hundred.
    async fn whole_trash(&self, verb: &str) -> ServiceResult<Vec<E>> {
        let too_many = |total: u64| {
            ServiceError::Validation(format!(
                "The trash holds {total} items, more than the {MAX_BATCH_SIZE} that can be {verb} \
                 at once. Select the items to act on instead."
            ))
        };
        let per_page = backbone_orm::repository::PaginationParams::MAX_PER_PAGE;
        let mut rows: Vec<E> = Vec::new();
        let mut page = 1u32;
        loop {
            let (batch, total) = self
                .repository
                .list_deleted(page, per_page)
                .await
                .map_err(ServiceError::Repository)?;
            if total > MAX_BATCH_SIZE as u64 {
                return Err(too_many(total));
            }
            let fetched = batch.len();
            rows.extend(batch);
            if rows.len() > MAX_BATCH_SIZE {
                return Err(too_many(rows.len() as u64));
            }
            if fetched == 0 || rows.len() as u64 >= total || fetched < per_page as usize {
                break;
            }
            page += 1;
        }
        Ok(rows)
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

    /// List deleted entities with the same filters and search the active list takes.
    pub async fn list_deleted_filtered(
        &self,
        page: u32,
        limit: u32,
        filters: HashMap<String, String>,
    ) -> ServiceResult<(Vec<E>, u64)> {
        self.repository
            .list_deleted_filtered(page, limit, filters)
            .await
            .map_err(ServiceError::Repository)
    }

    /// Upsert is not available on the generic service, and refuses rather than guessing.
    ///
    /// An upsert must match the record it would update, by id or by a natural key. A create
    /// DTO carries no id and the framework knows no natural key for an entity, so the only thing
    /// this could do is create — which is what it used to do, silently duplicating the record on
    /// every retry. The generic HTTP route answers 501 for the same reason. An entity with a real
    /// upsert implements it in its own service.
    pub async fn upsert(&self, dto: C) -> ServiceResult<E> {
        let _ = dto;
        Err(ServiceError::Validation(UPSERT_UNSUPPORTED.to_string()))
    }

    /// Apply a partial update from a JSON map of field values.
    ///
    /// Fields not present in the map are left unchanged. A key the entity does
    /// not have, or a new value for a protected field, refuses the whole write.
    pub async fn partial_update(
        &self,
        id: &str,
        fields: HashMap<String, serde_json::Value>,
    ) -> ServiceResult<Option<E>>
    where
        E: serde::Serialize + serde::de::DeserializeOwned,
    {
        let written = self
            .atomically(async {
                let ctx = self.write_ctx();
                let existing = self
                    .repository
                    .find_by_id(id)
                    .await
                    .map_err(ServiceError::Repository)?;
                let Some(before) = existing else {
                    return Ok(None);
                };
                let mut patched = merge_patch(&before, fields)?;
                self.consult(crate::write_guard::WriteKind::Update, Some(&before), Some(&patched)).await?;
                self.lifecycle.before_update(&mut patched).await?;
                self.hooks.before(&ctx, &mut Change::update(&before, &mut patched)).await?;
                self.check_references(Some(&before), &patched).await?;
                // Only the fields this write changed are saved, and only while the row is still
                // live: see `CrudRepository::update_changed`. A row deleted since the read
                // answers not found.
                let Some(mut saved) = self
                    .repository
                    .update_changed(&before, patched)
                    .await
                    .map_err(ServiceError::Repository)?
                else {
                    return Ok(None);
                };
                self.lifecycle.after_update(&saved).await?;
                self.hooks.after(&ctx, &Change::update(&before, &mut saved)).await?;

                let meta = EventMetadata::new(saved.entity_id(), std::any::type_name::<E>());
                let event = CrudEvent::Updated { before, after: saved.clone(), metadata: meta };
                Ok(Some((saved, vec![event])))
            })
            .await?;
        let Some((saved, events)) = written else {
            return Ok(None);
        };
        self.publish_all(events).await;
        Ok(Some(saved))
    }

    /// Create many entities, all or nothing.
    ///
    /// Each item goes through what a single create does — the DTO conversion, the write guard,
    /// the `before_create` hook — before anything is written; then the repository inserts them
    /// in one transaction. A failure anywhere fails the whole batch and names the item: a
    /// violation's path starts with the item's index (`3/name`).
    pub async fn bulk_create(&self, dtos: Vec<C>) -> ServiceResult<Vec<E>>
    where
        E: FromCreateDto<C>,
    {
        check_batch_size(dtos.len())?;
        let (saved, events) = self
            .atomically(async {
                let ctx = self.write_ctx();
                let mut prepared = Vec::with_capacity(dtos.len());
                for (index, dto) in dtos.into_iter().enumerate() {
                    let at = |e: ServiceError| at_batch_index(e, index);
                    let mut entity = E::from_create_dto(dto).map_err(at)?;
                    self.consult(crate::write_guard::WriteKind::Create, None, Some(&entity))
                        .await
                        .map_err(at)?;
                    self.lifecycle.before_create(&mut entity).await.map_err(at)?;
                    self.hooks
                        .before(&ctx, &mut Change::Created { after: &mut entity })
                        .await
                        .map_err(at)?;
                    self.check_references(None, &entity).await.map_err(at)?;
                    prepared.push(entity);
                }
                let mut saved = self
                    .repository
                    .bulk_create(prepared)
                    .await
                    .map_err(ServiceError::Repository)?;
                let mut events = Vec::with_capacity(saved.len());
                for entity in saved.iter_mut() {
                    self.lifecycle.after_create(entity).await?;
                    self.hooks.after(&ctx, &Change::Created { after: entity }).await?;
                    let meta = EventMetadata::new(entity.entity_id(), std::any::type_name::<E>());
                    events.push(CrudEvent::Created { entity: entity.clone(), metadata: meta });
                }
                Ok((saved, events))
            })
            .await?;
        self.publish_all(events).await;
        Ok(saved)
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
        let (affected, events) = self
            .atomically(async {
                let ctx = self.write_ctx();
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
                    self.consult(crate::write_guard::WriteKind::Delete, Some(entity), None).await?;
                    self.lifecycle.before_delete(entity).await?;
                    self.hooks
                        .before(&ctx, &mut Change::Deleted { before: entity, hard: false })
                        .await?;
                }
                let affected = self
                    .repository
                    .bulk_soft_delete(&ids)
                    .await
                    .map_err(ServiceError::Repository)?;
                let mut events = Vec::with_capacity(entities.len());
                for (id, entity) in ids.iter().zip(entities.into_iter()) {
                    self.lifecycle.after_delete(id).await?;
                    self.hooks
                        .after(&ctx, &Change::Deleted { before: &entity, hard: false })
                        .await?;
                    let meta = EventMetadata::new(id, std::any::type_name::<E>());
                    events.push(CrudEvent::SoftDeleted { entity, metadata: meta });
                }
                Ok((affected, events))
            })
            .await?;
        self.publish_all(events).await;
        Ok(affected)
    }

    /// Restore many soft-deleted entities by id (atomic).
    pub async fn bulk_restore(&self, ids: Vec<String>) -> ServiceResult<Vec<E>> {
        check_batch_size(ids.len())?;
        let ids = dedup_ids(ids);
        let (restored, events) = self
            .atomically(async {
                let ctx = self.write_ctx();
                // Ask the lifecycle about every row it can see; an id that is not in
                // the trash is left to the repository's atomic check below.
                for id in &ids {
                    if let Some(trashed) = self.get_deleted_by_id(id).await? {
                        self.consult(crate::write_guard::WriteKind::Restore, Some(&trashed), None).await?;
                        self.lifecycle.before_restore(&trashed).await?;
                        self.hooks.before(&ctx, &mut Change::Restored { entity: &trashed }).await?;
                    }
                }
                let restored = self
                    .repository
                    .bulk_restore(&ids)
                    .await
                    .map_err(ServiceError::Repository)?;
                let events = self.restored_events(&ctx, &restored).await?;
                Ok((restored, events))
            })
            .await?;
        self.publish_all(events).await;
        Ok(restored)
    }

    /// Run the after-hooks of each restored row and build its `Restored` event.
    async fn restored_events(&self, ctx: &WriteCtx, restored: &[E]) -> ServiceResult<Vec<CrudEvent<E>>> {
        let mut events = Vec::with_capacity(restored.len());
        for entity in restored {
            self.hooks.after(ctx, &Change::Restored { entity }).await?;
            let meta = EventMetadata::new(entity.entity_id(), std::any::type_name::<E>());
            events.push(CrudEvent::Restored { entity: entity.clone(), metadata: meta });
        }
        Ok(events)
    }

    /// Restore every soft-deleted entity (atomic). Returns the number restored.
    /// Emits a `Restored` event per entity, mirroring [`bulk_restore`](Self::bulk_restore).
    ///
    /// Like [`empty_trash`](Self::empty_trash), the trashed rows are loaded first (at most
    /// [`MAX_BATCH_SIZE`]), each is put to the write guard and lifecycle as a single restore
    /// would be, and the restore names exactly those ids, all or nothing.
    pub async fn restore_all(&self) -> ServiceResult<u64> {
        let (count, events) = self
            .atomically(async {
                let ctx = self.write_ctx();
                self.lifecycle.before_restore_all().await?;
                let trashed = self.whole_trash("restored").await?;
                if trashed.is_empty() {
                    return Ok((0, Vec::new()));
                }
                for entity in &trashed {
                    self.consult(crate::write_guard::WriteKind::Restore, Some(entity), None).await?;
                    self.lifecycle.before_restore(entity).await?;
                    self.hooks.before(&ctx, &mut Change::Restored { entity }).await?;
                }
                let ids: Vec<String> = trashed.iter().map(|e| e.entity_id()).collect();
                let restored = self
                    .repository
                    .bulk_restore(&ids)
                    .await
                    .map_err(ServiceError::Repository)?;
                let events = self.restored_events(&ctx, &restored).await?;
                Ok((restored.len() as u64, events))
            })
            .await?;
        self.publish_all(events).await;
        Ok(count)
    }

    /// Permanently delete many soft-deleted entities by id (atomic).
    pub async fn bulk_permanent_delete(&self, ids: Vec<String>) -> ServiceResult<u64> {
        check_batch_size(ids.len())?;
        let ids = dedup_ids(ids);
        let (affected, events) = self
            .atomically(async {
                let ctx = self.write_ctx();
                // The lifecycle is asked about every row it can see. Whether every id
                // is in the trash stays with `bulk_hard_delete`, which deletes only
                // trashed rows and rolls the whole batch back unless every id matched,
                // so a row that moves between this loop and the write cannot slip
                // through.
                let mut seen = Vec::new();
                for id in &ids {
                    if let Some(trashed) = self.get_deleted_by_id(id).await? {
                        self.consult(crate::write_guard::WriteKind::HardDelete, Some(&trashed), None).await?;
                        self.lifecycle.before_hard_delete(&trashed).await?;
                        self.hooks
                            .before(&ctx, &mut Change::Deleted { before: &trashed, hard: true })
                            .await?;
                        seen.push(trashed);
                    }
                }
                let affected = self
                    .repository
                    .bulk_hard_delete(&ids)
                    .await
                    .map_err(ServiceError::Repository)?;
                for trashed in &seen {
                    self.hooks
                        .after(&ctx, &Change::Deleted { before: trashed, hard: true })
                        .await?;
                }
                Ok((affected, hard_deleted_events::<E>(&ids)))
            })
            .await?;
        self.publish_all(events).await;
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
        let (saved, events) = self
            .atomically(async {
                let ctx = self.write_ctx();
                let mut befores = Vec::with_capacity(items.len());
                let mut prepared = Vec::with_capacity(items.len());
                for (index, (id, dto)) in items.into_iter().enumerate() {
                    let Some(before) = self
                        .repository
                        .find_by_id(&id)
                        .await
                        .map_err(ServiceError::Repository)?
                    else {
                        return Err(ServiceError::Validation(format!("id '{id}' not found")));
                    };
                    let mut updated = before.clone().apply_update(dto)?;
                    refuse_protected_changes::<E>(&as_object(&before)?, &as_object(&updated)?)?;
                    self.consult(crate::write_guard::WriteKind::Update, Some(&before), Some(&updated)).await?;
                    self.lifecycle.before_update(&mut updated).await?;
                    self.hooks.before(&ctx, &mut Change::update(&before, &mut updated)).await?;
                    self.check_references(Some(&before), &updated)
                        .await
                        .map_err(|e| at_batch_index(e, index))?;
                    prepared.push((before.clone(), updated));
                    befores.push(before);
                }
                // Only the fields each item changed are written, as a single PUT does, so a
                // column a concurrent verb moved is not written back with the value read before it.
                let mut saved = self
                    .repository
                    .bulk_update_changed(prepared)
                    .await
                    .map_err(ServiceError::Repository)?;
                let events = self.after_bulk_updates(&ctx, befores, &mut saved).await?;
                Ok((saved, events))
            })
            .await?;
        self.publish_all(events).await;
        Ok(saved)
    }

    /// Run `after_update` and the after-hooks for each `(before, after)` pair, and build its
    /// `Updated` event. Shared by [`bulk_update`](Self::bulk_update) and
    /// [`bulk_partial_update`](Self::bulk_partial_update).
    async fn after_bulk_updates(
        &self,
        ctx: &WriteCtx,
        befores: Vec<E>,
        saved: &mut [E],
    ) -> ServiceResult<Vec<CrudEvent<E>>> {
        let mut events = Vec::with_capacity(saved.len());
        for (before, after) in befores.into_iter().zip(saved.iter_mut()) {
            self.lifecycle.after_update(after).await?;
            self.hooks.after(ctx, &Change::update(&before, after)).await?;
            let meta = EventMetadata::new(after.entity_id(), std::any::type_name::<E>());
            events.push(CrudEvent::Updated { before, after: after.clone(), metadata: meta });
        }
        Ok(events)
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
        let (saved, events) = self
            .atomically(async {
                let ctx = self.write_ctx();
                let mut befores = Vec::with_capacity(items.len());
                let mut prepared = Vec::with_capacity(items.len());
                for (index, (id, fields)) in items.into_iter().enumerate() {
                    let Some(before) = self
                        .repository
                        .find_by_id(&id)
                        .await
                        .map_err(ServiceError::Repository)?
                    else {
                        return Err(ServiceError::Validation(format!("id '{id}' not found")));
                    };
                    let mut patched = merge_patch(&before, fields)?;
                    self.consult(crate::write_guard::WriteKind::Update, Some(&before), Some(&patched)).await?;
                    self.lifecycle.before_update(&mut patched).await?;
                    self.hooks.before(&ctx, &mut Change::update(&before, &mut patched)).await?;
                    self.check_references(Some(&before), &patched)
                        .await
                        .map_err(|e| at_batch_index(e, index))?;
                    prepared.push((before.clone(), patched));
                    befores.push(before);
                }
                let mut saved = self
                    .repository
                    .bulk_update_changed(prepared)
                    .await
                    .map_err(ServiceError::Repository)?;
                let events = self.after_bulk_updates(&ctx, befores, &mut saved).await?;
                Ok((saved, events))
            })
            .await?;
        self.publish_all(events).await;
        Ok(saved)
    }

    /// What [`bulk_partial_update`](Self::bulk_partial_update) would do, with
    /// nothing written. Each row goes through the same steps the write takes —
    /// it is found, the patch is merged and checked against the protected fields,
    /// the write rules are consulted (quietly: a shadow rule logs on the real
    /// write, not here) and the update hook runs — and its outcome is
    /// kept instead of failing the batch, so one dialog can say which rows would
    /// change and which would be refused. Only a batch that is too large or names
    /// an id twice fails as a whole, as the write does.
    pub async fn preview_bulk_partial_update(
        &self,
        items: Vec<(String, HashMap<String, serde_json::Value>)>,
    ) -> ServiceResult<Vec<crate::http::BulkPreviewRow<E, ServiceError>>>
    where
        E: serde::Serialize + serde::de::DeserializeOwned,
    {
        check_batch_size(items.len())?;
        if let Some(dup) = first_duplicate_id(items.iter().map(|(id, _)| id.clone())) {
            return Err(ServiceError::Validation(format!(
                "duplicate id '{dup}' in bulk partial update"
            )));
        }
        let mut rows = Vec::with_capacity(items.len());
        for (id, fields) in items {
            let outcome = self.preview_one(&id, fields).await;
            rows.push(crate::http::BulkPreviewRow { id, outcome });
        }
        Ok(rows)
    }

    async fn preview_one(
        &self,
        id: &str,
        fields: HashMap<String, serde_json::Value>,
    ) -> ServiceResult<(E, E)>
    where
        E: serde::Serialize + serde::de::DeserializeOwned,
    {
        let Some(before) = self
            .repository
            .find_by_id(id)
            .await
            .map_err(ServiceError::Repository)?
        else {
            return Err(ServiceError::Validation(format!("id '{id}' not found")));
        };
        let mut patched = merge_patch(&before, fields)?;
        self.consult_quietly(crate::write_guard::WriteKind::Update, Some(&before), Some(&patched))
            .await?;
        self.lifecycle.before_update(&mut patched).await?;
        self.check_references(Some(&before), &patched).await?;
        Ok((before, patched))
    }
}

// ─── Generic write protection ────────────────────────────────────────────────

/// Serialized fields no generic write may change on any entity: the row's
/// identity, and the audit block that records who created, changed and
/// deleted it. Entities add their own through
/// [`PersistentEntity::write_protected_fields`].
pub const ALWAYS_WRITE_PROTECTED: &[&str] = &["id", "metadata"];

fn as_object(entity: &impl serde::Serialize) -> ServiceResult<serde_json::Map<String, serde_json::Value>> {
    match serde_json::to_value(entity).map_err(|e| ServiceError::Internal(e.to_string()))? {
        serde_json::Value::Object(map) => Ok(map),
        _ => Err(ServiceError::Internal("an entity must serialize to a JSON object".into())),
    }
}

/// Refuse a generic write that gives a protected field a new value. Carrying
/// the stored value unchanged is allowed, so a form that sends the whole
/// record is not refused for fields it did not touch.
///
/// A state machine's verb writes through the same path, inside
/// [`as_transition`](crate::write_guard::as_transition), and its whole point is
/// to move the state field the entity protects from generic writes. Inside a
/// transition the entity's [`PersistentEntity::state_fields`] are therefore let
/// through — those and nothing else; the write guard still decides whether the
/// move itself is allowed.
fn refuse_protected_changes<E: PersistentEntity>(
    before: &serde_json::Map<String, serde_json::Value>,
    after: &serde_json::Map<String, serde_json::Value>,
) -> ServiceResult<()> {
    let in_transition = crate::write_guard::current_transition().is_some();
    let refused: Vec<crate::violation::Violation> = ALWAYS_WRITE_PROTECTED
        .iter()
        .chain(E::write_protected_fields())
        .filter(|key| !(in_transition && E::state_fields().contains(*key)))
        .filter(|key| before.get(**key) != after.get(**key))
        .map(|key| {
            crate::violation::Violation::new(
                *key,
                "field_not_writable",
                format!(
                    "field_not_writable: `{key}` cannot be changed by a generic write; \
                     it changes only through the operation that owns it"
                ),
            )
        })
        .collect();
    if refused.is_empty() {
        Ok(())
    } else {
        Err(ServiceError::Violations(refused))
    }
}

/// Merge a partial update onto an entity.
///
/// The merge goes through the entity's JSON form, so a key the entity has no
/// field for would otherwise be dropped without a word and the edit lost. It
/// is refused instead: a key given a value that does not survive the round
/// trip is not a field of this record. A key set to `null` is exempt, since
/// an empty optional field may be left out of the JSON form entirely.
fn merge_patch<E: PersistentEntity>(
    before: &E,
    fields: HashMap<String, serde_json::Value>,
) -> ServiceResult<E> {
    let before_obj = as_object(before)?;
    let mut merged = before_obj.clone();
    let mut given = Vec::new();
    for (key, value) in fields {
        if !value.is_null() {
            given.push(key.clone());
        }
        merged.insert(key, value);
    }
    let patched: E = serde_path_to_error::deserialize(serde_json::Value::Object(merged))
        .map_err(|e| patch_type_refusal(&e))?;
    let after_obj = as_object(&patched)?;

    let mut unknown: Vec<String> = given.into_iter().filter(|k| !after_obj.contains_key(k)).collect();
    if !unknown.is_empty() {
        unknown.sort();
        return Err(ServiceError::Violations(
            unknown
                .into_iter()
                .map(|k| {
                    let message = format!("unknown_field: `{k}` is not a field of this record");
                    crate::violation::Violation::new(k, "unknown_field", message)
                })
                .collect(),
        ));
    }
    refuse_protected_changes::<E>(&before_obj, &after_obj)?;
    Ok(patched)
}

/// The refusal for a patch value the entity cannot hold: `required` on a field set to null
/// that may not be empty, `invalid_value` on a field given a value of the wrong kind — the
/// codes a create body's decoding answers with for the same mistakes — on the field's path.
fn patch_type_refusal(err: &serde_path_to_error::Error<serde_json::Error>) -> ServiceError {
    let path = err
        .path()
        .iter()
        .filter_map(|segment| match segment {
            serde_path_to_error::Segment::Seq { index } => Some(index.to_string()),
            serde_path_to_error::Segment::Map { key } => Some(key.clone()),
            serde_path_to_error::Segment::Enum { variant } => Some(variant.clone()),
            serde_path_to_error::Segment::Unknown => None,
        })
        .collect::<Vec<_>>()
        .join("/");
    let message = err.inner().to_string();
    let message = match message.rfind(" at line ") {
        Some(at) => message[..at].to_string(),
        None => message,
    };
    let violation = if let Some(field) = message
        .strip_prefix("missing field `")
        .and_then(|rest| rest.split('`').next())
    {
        let field_path = if path.is_empty() { field.to_string() } else { format!("{path}/{field}") };
        crate::violation::Violation::new(field_path.clone(), "required", format!("{field_path} is required"))
    } else if message.starts_with("invalid type: null") {
        let subject = if path.is_empty() { "A value".to_string() } else { path.clone() };
        crate::violation::Violation::new(path, "required", format!("{subject} is required"))
    } else {
        let text = if path.is_empty() { message } else { format!("{path}: {message}") };
        crate::violation::Violation::new(path, "invalid_value", text)
    };
    ServiceError::Violations(vec![violation])
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

/// The refusal [`GenericCrudService::upsert`] answers with.
pub const UPSERT_UNSUPPORTED: &str =
    "upsert is not available for this resource: a create body names no record to match. \
     Create the record, or update it by id.";

/// Place a service error on item `index` of a batch (see
/// [`RepositoryError::at_batch_index`]).
fn at_batch_index(err: ServiceError, index: usize) -> ServiceError {
    let item = |message: String| format!("item {index}: {message}");
    match err {
        ServiceError::Violations(violations) => ServiceError::Violations(
            violations
                .into_iter()
                .map(|mut v| {
                    v.path = crate::violation::batch_item_path(index, &v.path);
                    v
                })
                .collect(),
        ),
        ServiceError::Repository(e) => ServiceError::Repository(e.at_batch_index(index)),
        ServiceError::Validation(m) => ServiceError::Validation(item(m)),
        ServiceError::AlreadyExists(m) => ServiceError::AlreadyExists(item(m)),
        ServiceError::Internal(m) => ServiceError::Internal(item(m)),
        ServiceError::NotFound => ServiceError::Validation(item("not found".to_string())),
    }
}

/// The `HardDeleted` event of each id removed for good.
fn hard_deleted_events<E: PersistentEntity>(ids: &[String]) -> Vec<CrudEvent<E>> {
    ids.iter()
        .map(|id| CrudEvent::HardDeleted {
            entity_id: id.clone(),
            metadata: EventMetadata::new(id, std::any::type_name::<E>()),
        })
        .collect()
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

    fn violations_of(err: &ServiceError) -> Option<Vec<crate::violation::Violation>> {
        match err {
            ServiceError::Violations(v) | ServiceError::Repository(RepositoryError::Violations(v)) => Some(v.clone()),
            _ => None,
        }
    }

    fn status_of(err: &ServiceError) -> Option<axum::http::StatusCode> {
        Some(crate::http::service_error_status(err))
    }

    fn supports_upsert() -> bool {
        false
    }

    fn entity_name() -> &'static str {
        // The type's own name, never its module path: it reaches log lines and must not
        // describe the crate layout to anyone reading a response built from it.
        let full = std::any::type_name::<E>();
        full.rsplit("::").next().unwrap_or(full)
    }

    async fn fetch_related_json(&self, table: &str, ids: &[String]) -> Vec<serde_json::Value> {
        self.repository().fetch_related_json(table, ids).await
    }

    async fn list(
        &self,
        page: u32,
        limit: u32,
        filters: HashMap<String, String>,
    ) -> Result<(Vec<E>, u64), ServiceError> {
        self.list(page, limit, filters).await
    }

    async fn list_with_info(
        &self,
        page: u32,
        limit: u32,
        filters: HashMap<String, String>,
    ) -> Result<(Vec<E>, backbone_orm::repository::PaginationInfo), ServiceError> {
        self.list_with_info(page, limit, filters).await
    }

    async fn aggregate(
        &self,
        spec: &backbone_orm::repository::AggregateSpec,
        filters: HashMap<String, String>,
    ) -> Result<backbone_orm::repository::AggregateResult, ServiceError> {
        self.aggregate(spec, filters).await
    }

    fn table_name(&self) -> Option<&str> {
        self.repository.table_name()
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

    async fn preview_bulk_partial_update(
        &self,
        items: Vec<(String, HashMap<String, serde_json::Value>)>,
    ) -> Result<Option<Vec<crate::http::BulkPreviewRow<E, ServiceError>>>, ServiceError> {
        self.preview_bulk_partial_update(items).await.map(Some)
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
            guard: self.guard.clone(),
            hooks: self.hooks.clone(),
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

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Widget {
        id: String,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        note: Option<String>,
        secret_hash: String,
        /// A state-machine field: protected from generic writes, moved by verbs.
        #[serde(default)]
        state: String,
        /// A reference to an owner row, checked for visibility before a write sets it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        owner_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        deleted_at: Option<DateTime<Utc>>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    }

    impl crate::persistence::traits::PersistentEntity for Widget {
        fn write_protected_fields() -> &'static [&'static str] {
            &["secret_hash", "state"]
        }
        fn state_fields() -> &'static [&'static str] {
            &["state"]
        }
        fn references() -> &'static [crate::persistence::traits::Reference] {
            // The shape the schema generator emits: a `const`, since a `const fn` call is not
            // promoted to a `'static` borrow on its own.
            const REFERENCES: &[crate::persistence::traits::Reference] =
                &[crate::persistence::traits::Reference::new("owner_id", "owners", "id")];
            REFERENCES
        }
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

    #[derive(Deserialize)]
    struct CreateWidgetDto {
        name: String,
    }

    #[derive(Deserialize)]
    struct UpdateWidgetDto {
        name: String,
    }

    impl backbone_orm::EntityRepoMeta for Widget {
        fn column_types() -> HashMap<String, String> {
            HashMap::new()
        }
        fn search_fields() -> &'static [&'static str] {
            &["name"]
        }
    }

    /// A row id whose lookup fails the way a broken database does.
    const BROKEN_ID: &str = "00000000-0000-0000-0000-00000000dead";

    impl FromCreateDto<CreateWidgetDto> for Widget {
        fn from_create_dto(dto: CreateWidgetDto) -> ServiceResult<Self> {
            // A create names its owner as `owned-by:<id>`, so the create DTO stays one field.
            let owner_id = dto.name.strip_prefix("owned-by:").map(str::to_string);
            Ok(Widget {
                id: uuid::Uuid::new_v4().to_string(),
                name: dto.name,
                note: None,
                secret_hash: "h0".into(),
                state: "draft".into(),
                owner_id,
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
        /// Every `update_changed` call, as (before, after).
        changed_writes: std::sync::Mutex<Vec<(Widget, Widget)>>,
        /// Every full `update` call.
        full_writes: std::sync::atomic::AtomicUsize,
        /// Make the next changed-columns write find the row gone (a concurrent delete).
        vanish_on_write: std::sync::atomic::AtomicBool,
        /// Every `bulk_create` call.
        bulk_creates: std::sync::atomic::AtomicUsize,
        /// The owner ids the caller can see; every other owner reads as hidden.
        visible_owners: std::sync::Mutex<Vec<String>>,
    }

    impl InMemoryWidgetRepo {
        fn new() -> Self {
            Self {
                data: tokio::sync::Mutex::new(Vec::new()),
                changed_writes: std::sync::Mutex::new(Vec::new()),
                full_writes: std::sync::atomic::AtomicUsize::new(0),
                vanish_on_write: std::sync::atomic::AtomicBool::new(false),
                bulk_creates: std::sync::atomic::AtomicUsize::new(0),
                visible_owners: std::sync::Mutex::new(vec!["mine".to_string()]),
            }
        }
    }

    #[async_trait]
    impl crate::persistence::traits::CrudRepository<Widget> for InMemoryWidgetRepo {
        async fn reference_visible(
            &self,
            table: &str,
            column: &str,
            value: &serde_json::Value,
        ) -> Result<Option<bool>, RepositoryError> {
            assert_eq!((table, column), ("owners", "id"));
            let visible = self.visible_owners.lock().unwrap();
            Ok(Some(value.as_str().is_some_and(|v| visible.iter().any(|o| o == v))))
        }

        async fn create(&self, entity: Widget) -> Result<Widget, RepositoryError> {
            self.data.lock().await.push(entity.clone());
            Ok(entity)
        }

        async fn find_by_id(&self, id: &str) -> Result<Option<Widget>, RepositoryError> {
            if id == BROKEN_ID {
                return Err(RepositoryError::DatabaseError(
                    "relation \"secret_schema.widgets\" is broken".into(),
                ));
            }
            Ok(self
                .data
                .lock()
                .await
                .iter()
                .find(|w| w.id == id && w.deleted_at.is_none())
                .cloned())
        }

        async fn update(&self, entity: Widget) -> Result<Widget, RepositoryError> {
            self.full_writes
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let mut data = self.data.lock().await;
            data.retain(|w| w.id != entity.id);
            data.push(entity.clone());
            Ok(entity)
        }

        async fn update_changed(
            &self,
            before: &Widget,
            after: Widget,
        ) -> Result<Option<Widget>, RepositoryError> {
            self.changed_writes
                .lock()
                .unwrap()
                .push((before.clone(), after.clone()));
            if self
                .vanish_on_write
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                return Ok(None);
            }
            if after.name == "taken" {
                return Err(RepositoryError::AlreadyExists(
                    "duplicate key value violates unique constraint \"widgets_name_key\"".into(),
                ));
            }
            let mut data = self.data.lock().await;
            data.retain(|w| w.id != after.id);
            data.push(after.clone());
            Ok(Some(after))
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
            // Capped as the generated repositories cap a page.
            let limit = (limit as usize).min(100);
            let offset = (page.saturating_sub(1) as usize) * limit;
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

        async fn list_deleted_filtered(
            &self,
            page: u32,
            limit: u32,
            filters: HashMap<String, String>,
        ) -> Result<(Vec<Widget>, u64), RepositoryError> {
            let (rows, _) = self.list_deleted(page, limit).await?;
            let rows: Vec<Widget> = rows
                .into_iter()
                .filter(|w| filters.get("name").is_none_or(|n| *n == w.name))
                .collect();
            let total = rows.len() as u64;
            Ok((rows, total))
        }

        async fn bulk_create(&self, entities: Vec<Widget>) -> Result<Vec<Widget>, RepositoryError> {
            self.bulk_creates
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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

    /// Records every hook call, and refuses a create named `refuse-before`.
    struct RecordingHooks(std::sync::Mutex<Vec<String>>);

    #[async_trait]
    impl WriteHooks<Widget> for RecordingHooks {
        async fn before(&self, _ctx: &WriteCtx, change: &mut Change<'_, Widget>) -> ServiceResult<()> {
            self.0.lock().unwrap().push(format!("before {:?}", change.kind()));
            match change {
                Change::Created { after } if after.name == "refuse-before" => {
                    Err(ServiceError::Validation("refused by a hook".into()))
                }
                // A before-hook may fill a field of the row to be stored.
                Change::Created { after } => {
                    after.note = Some("filled".into());
                    Ok(())
                }
                _ => Ok(()),
            }
        }

        async fn after(&self, _ctx: &WriteCtx, change: &Change<'_, Widget>) -> ServiceResult<()> {
            self.0.lock().unwrap().push(format!("after {:?}", change.kind()));
            Ok(())
        }
    }

    #[tokio::test]
    async fn write_hooks_run_around_each_write_and_may_refuse_it() {
        let repo = Arc::new(InMemoryWidgetRepo::new());
        let hooks = Arc::new(RecordingHooks(Default::default()));
        let service: GenericCrudService<Widget, CreateWidgetDto, UpdateWidgetDto, InMemoryWidgetRepo> =
            GenericCrudService::with_repository(repo.clone()).with_write_hooks(hooks.clone());

        let widget = service.create(CreateWidgetDto { name: "sprocket".into() }).await.unwrap();
        assert_eq!(widget.note.as_deref(), Some("filled"), "a before-hook fills the row that is stored");
        service.update(&widget.id, UpdateWidgetDto { name: "cog".into() }).await.unwrap();
        service.soft_delete(&widget.id).await.unwrap();
        service.restore(&widget.id).await.unwrap();
        assert_eq!(
            *hooks.0.lock().unwrap(),
            vec![
                "before Created",
                "after Created",
                "before Updated",
                "after Updated",
                "before Deleted",
                "after Deleted",
                "before Restored",
                "after Restored",
            ]
        );

        let refused = service.create(CreateWidgetDto { name: "refuse-before".into() }).await;
        assert!(matches!(refused, Err(ServiceError::Validation(_))));
        assert_eq!(repo.data.lock().await.len(), 1, "a refused write never reaches the repository");
    }

    #[tokio::test]
    async fn a_service_without_hooks_behaves_as_before() {
        let repo = Arc::new(InMemoryWidgetRepo::new());
        let service: GenericCrudService<Widget, CreateWidgetDto, UpdateWidgetDto, InMemoryWidgetRepo> =
            GenericCrudService::with_repository(repo);
        let created = service.bulk_create(vec![CreateWidgetDto { name: "a".into() }]).await.unwrap();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].note, None);
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

    // ── Generic writes go through the changed-columns path ───────────────────

    #[tokio::test]
    async fn a_patch_writes_only_through_the_changed_columns_path() {
        let repo = Arc::new(InMemoryWidgetRepo::new());
        let service: GenericCrudService<
            Widget,
            CreateWidgetDto,
            UpdateWidgetDto,
            InMemoryWidgetRepo,
        > = GenericCrudService::with_repository(repo.clone());
        let w = service
            .create(CreateWidgetDto { name: "old".into() })
            .await
            .unwrap();

        let mut fields = HashMap::new();
        fields.insert("note".to_string(), serde_json::json!("edited"));
        let saved = service
            .partial_update(&w.id, fields)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.note.as_deref(), Some("edited"));

        let writes = repo.changed_writes.lock().unwrap().clone();
        assert_eq!(writes.len(), 1, "a PATCH must save through update_changed");
        assert_eq!(writes[0].0, w, "the write starts from the row as read");
        assert_eq!(
            repo.full_writes.load(std::sync::atomic::Ordering::SeqCst),
            0
        );
    }

    #[tokio::test]
    async fn a_put_writes_only_through_the_changed_columns_path() {
        let repo = Arc::new(InMemoryWidgetRepo::new());
        let service: GenericCrudService<
            Widget,
            CreateWidgetDto,
            UpdateWidgetDto,
            InMemoryWidgetRepo,
        > = GenericCrudService::with_repository(repo.clone());
        let w = service
            .create(CreateWidgetDto { name: "old".into() })
            .await
            .unwrap();
        service
            .update(&w.id, UpdateWidgetDto { name: "new".into() })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(repo.changed_writes.lock().unwrap().len(), 1);
        assert_eq!(
            repo.full_writes.load(std::sync::atomic::Ordering::SeqCst),
            0
        );
    }

    #[tokio::test]
    async fn a_write_whose_row_vanished_meanwhile_answers_not_found() {
        let repo = Arc::new(InMemoryWidgetRepo::new());
        let service: GenericCrudService<
            Widget,
            CreateWidgetDto,
            UpdateWidgetDto,
            InMemoryWidgetRepo,
        > = GenericCrudService::with_repository(repo.clone());
        let w = service
            .create(CreateWidgetDto { name: "old".into() })
            .await
            .unwrap();
        repo.vanish_on_write
            .store(true, std::sync::atomic::Ordering::SeqCst);

        let mut fields = HashMap::new();
        fields.insert("note".to_string(), serde_json::json!("edited"));
        assert!(service
            .partial_update(&w.id, fields)
            .await
            .unwrap()
            .is_none());
        assert!(service
            .update(&w.id, UpdateWidgetDto { name: "new".into() })
            .await
            .unwrap()
            .is_none());
    }

    // ── A transition may move the state field, and only that ─────────────────

    #[tokio::test]
    async fn a_transition_moves_its_protected_state_field() {
        let service = svc();
        let w = service
            .create(CreateWidgetDto { name: "w".into() })
            .await
            .unwrap();
        let mut fields = HashMap::new();
        fields.insert("state".to_string(), serde_json::json!("active"));
        let saved =
            crate::write_guard::as_transition("activate", service.partial_update(&w.id, fields))
                .await
                .expect("a verb moves its own state field")
                .unwrap();
        assert_eq!(saved.state, "active");
    }

    #[tokio::test]
    async fn a_transition_still_cannot_move_another_protected_field() {
        let service = svc();
        let w = service
            .create(CreateWidgetDto { name: "w".into() })
            .await
            .unwrap();
        let mut fields = HashMap::new();
        fields.insert("state".to_string(), serde_json::json!("active"));
        fields.insert("secret_hash".to_string(), serde_json::json!("h1"));
        let err =
            crate::write_guard::as_transition("activate", service.partial_update(&w.id, fields))
                .await
                .unwrap_err();
        match err {
            ServiceError::Violations(v) => {
                assert_eq!(v.len(), 1, "{v:?}");
                assert_eq!(
                    (v[0].path.as_str(), v[0].code.as_str()),
                    ("secret_hash", "field_not_writable")
                );
            }
            other => panic!("expected a violation, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_generic_write_still_cannot_move_the_state_field() {
        let service = svc();
        let w = service
            .create(CreateWidgetDto { name: "w".into() })
            .await
            .unwrap();
        let mut fields = HashMap::new();
        fields.insert("state".to_string(), serde_json::json!("active"));
        let err = service.partial_update(&w.id, fields).await.unwrap_err();
        assert!(
            matches!(err, ServiceError::Violations(ref v) if v[0].path == "state"),
            "{err:?}"
        );
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

    // ── Generic writes may not change protected or unknown fields ─────────

    /// A full update that carries the protected field, the way a form that
    /// replaces the whole record would.
    struct RekeyWidgetDto {
        name: String,
        secret_hash: String,
    }

    impl ApplyUpdateDto<RekeyWidgetDto> for Widget {
        fn apply_update(mut self, dto: RekeyWidgetDto) -> ServiceResult<Self> {
            self.name = dto.name;
            self.secret_hash = dto.secret_hash;
            Ok(self)
        }
    }

    type RekeyService = GenericCrudService<Widget, CreateWidgetDto, RekeyWidgetDto, InMemoryWidgetRepo>;

    fn patch(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    /// The refusal's message, whether the service refused with named
    /// violations or with a plain validation sentence.
    fn validation_message(res: ServiceResult<impl std::fmt::Debug>) -> String {
        match res {
            Err(ServiceError::Validation(m)) => m,
            Err(e @ ServiceError::Violations(_)) => e.to_string(),
            other => panic!("expected a validation error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn patch_rejects_a_field_the_entity_does_not_have() {
        let service = svc();
        let w = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap();

        let msg = validation_message(service.partial_update(&w.id, patch(&[("nmae", serde_json::json!("b"))])).await);
        assert!(msg.contains("unknown_field") && msg.contains("nmae"), "{msg}");
        assert_eq!(service.get_by_id(&w.id).await.unwrap().unwrap().name, "a");
    }

    #[tokio::test]
    async fn patch_rejects_changing_the_id() {
        let service = svc();
        let w = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap();

        let msg = validation_message(service.partial_update(&w.id, patch(&[("id", serde_json::json!("other"))])).await);
        assert!(msg.contains("field_not_writable") && msg.contains("id"), "{msg}");
        assert!(service.get_by_id(&w.id).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn patch_rejects_changing_a_field_the_entity_protects() {
        let service = svc();
        let w = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap();

        let msg = validation_message(
            service.partial_update(&w.id, patch(&[("secret_hash", serde_json::json!("h1"))])).await,
        );
        assert!(msg.contains("field_not_writable") && msg.contains("secret_hash"), "{msg}");
        assert_eq!(service.get_by_id(&w.id).await.unwrap().unwrap().secret_hash, "h0");
    }

    #[tokio::test]
    async fn patch_accepts_a_protected_field_echoed_unchanged() {
        let service = svc();
        let w = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap();

        let saved = service
            .partial_update(&w.id, patch(&[("name", serde_json::json!("b")), ("secret_hash", serde_json::json!("h0"))]))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.name, "b");
    }

    #[tokio::test]
    async fn patch_sets_and_clears_an_optional_field_that_is_not_serialized_when_empty() {
        let service = svc();
        let w = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap();

        let set = service.partial_update(&w.id, patch(&[("note", serde_json::json!("hi"))])).await.unwrap().unwrap();
        assert_eq!(set.note.as_deref(), Some("hi"));
        let cleared = service.partial_update(&w.id, patch(&[("note", serde_json::Value::Null)])).await.unwrap().unwrap();
        assert_eq!(cleared.note, None);
    }

    #[tokio::test]
    async fn put_rejects_changing_a_field_the_entity_protects() {
        let service: RekeyService = GenericCrudService::with_repository(Arc::new(InMemoryWidgetRepo::new()));
        let w = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap();

        let msg = validation_message(
            service.update(&w.id, RekeyWidgetDto { name: "b".into(), secret_hash: "h1".into() }).await,
        );
        assert!(msg.contains("field_not_writable") && msg.contains("secret_hash"), "{msg}");

        // The same form echoing the stored value is an ordinary edit.
        let saved = service
            .update(&w.id, RekeyWidgetDto { name: "b".into(), secret_hash: "h0".into() })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.name, "b");
    }

    #[tokio::test]
    async fn bulk_update_rejects_a_protected_change_before_writing_any_row() {
        let service: RekeyService = GenericCrudService::with_repository(Arc::new(InMemoryWidgetRepo::new()));
        let a = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap();
        let b = service.create(CreateWidgetDto { name: "b".into() }).await.unwrap();

        let res = service
            .bulk_update(vec![
                (a.id.clone(), RekeyWidgetDto { name: "a2".into(), secret_hash: "h0".into() }),
                (b.id.clone(), RekeyWidgetDto { name: "b2".into(), secret_hash: "h1".into() }),
            ])
            .await;
        assert!(validation_message(res).contains("field_not_writable"));
        assert_eq!(service.get_by_id(&a.id).await.unwrap().unwrap().name, "a");
    }

    #[tokio::test]
    async fn bulk_partial_update_rejects_a_protected_change_before_writing_any_row() {
        let service = svc();
        let a = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap();
        let b = service.create(CreateWidgetDto { name: "b".into() }).await.unwrap();

        let res = service
            .bulk_partial_update(vec![
                (a.id.clone(), patch(&[("name", serde_json::json!("a2"))])),
                (b.id.clone(), patch(&[("secret_hash", serde_json::json!("h1"))])),
            ])
            .await;
        assert!(validation_message(res).contains("field_not_writable"));
        assert_eq!(service.get_by_id(&a.id).await.unwrap().unwrap().name, "a");
    }

    #[tokio::test]
    async fn a_bulk_preview_says_what_each_row_would_do_and_writes_nothing() {
        let service = svc();
        let a = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap();
        let b = service.create(CreateWidgetDto { name: "b".into() }).await.unwrap();

        let rows = service
            .preview_bulk_partial_update(vec![
                (a.id.clone(), patch(&[("name", serde_json::json!("a2"))])),
                (b.id.clone(), patch(&[("secret_hash", serde_json::json!("h1"))])),
                ("missing".into(), patch(&[("name", serde_json::json!("x"))])),
            ])
            .await
            .unwrap();
        assert_eq!(rows.len(), 3);
        let (before, after) = rows[0].outcome.as_ref().unwrap();
        assert_eq!((before.name.as_str(), after.name.as_str()), ("a", "a2"));
        let refused = rows[1].outcome.as_ref().err().unwrap().to_string();
        assert!(refused.contains("field_not_writable"), "{refused}");
        assert!(rows[2].outcome.is_err(), "a row that is not there is refused, not dropped");
        // One refused row does not refuse the others, and nothing was written.
        assert_eq!(service.get_by_id(&a.id).await.unwrap().unwrap().name, "a");
    }

    #[tokio::test]
    async fn a_bulk_preview_refuses_a_batch_the_write_would_refuse_whole() {
        let service = svc();
        let a = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap();
        let res = service
            .preview_bulk_partial_update(vec![
                (a.id.clone(), patch(&[("name", serde_json::json!("a2"))])),
                (a.id.clone(), patch(&[("name", serde_json::json!("a3"))])),
            ])
            .await;
        assert!(validation_message(res.map(|r| r.len())).contains("duplicate id"));
    }

    #[tokio::test]
    async fn bulk_partial_update_rejects_an_unknown_field() {
        let service = svc();
        let a = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap();

        let res = service.bulk_partial_update(vec![(a.id.clone(), patch(&[("colour", serde_json::json!("red"))]))]).await;
        assert!(validation_message(res).contains("unknown_field"));
    }

    // ── Events and trash lifecycle ────────────────────────────────────────

    #[derive(Default)]
    struct RecordingPublisher {
        kinds: tokio::sync::Mutex<Vec<&'static str>>,
    }

    #[async_trait]
    impl CrudEventPublisher<Widget> for RecordingPublisher {
        async fn publish(&self, event: CrudEvent<Widget>) -> Result<(), backbone_messaging::EventError> {
            let kind = match event {
                CrudEvent::Updated { .. } => "updated",
                CrudEvent::Patched { .. } => "patched",
                _ => "other",
            };
            self.kinds.lock().await.push(kind);
            Ok(())
        }
    }

    #[tokio::test]
    async fn patch_publishes_an_updated_event() {
        let publisher = Arc::new(RecordingPublisher::default());
        let service = svc().with_event_publisher(publisher.clone());
        let w = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap();
        publisher.kinds.lock().await.clear();

        service.partial_update(&w.id, patch(&[("name", serde_json::json!("b"))])).await.unwrap();
        assert_eq!(*publisher.kinds.lock().await, vec!["updated"]);
    }

    /// A lifecycle that refuses every trash operation, so a test can tell
    /// whether the operation consulted it.
    struct RefuseTrash;

    #[async_trait]
    impl ServiceLifecycle<Widget> for RefuseTrash {
        async fn before_restore(&self, _entity: &Widget) -> ServiceResult<()> {
            Err(ServiceError::Validation("restore refused".into()))
        }
        async fn before_hard_delete(&self, _entity: &Widget) -> ServiceResult<()> {
            Err(ServiceError::Validation("hard delete refused".into()))
        }
        async fn before_restore_all(&self) -> ServiceResult<()> {
            Err(ServiceError::Validation("restore all refused".into()))
        }
        async fn before_empty_trash(&self) -> ServiceResult<()> {
            Err(ServiceError::Validation("empty trash refused".into()))
        }
    }

    #[tokio::test]
    async fn every_trash_operation_consults_the_lifecycle() {
        let repo = Arc::new(InMemoryWidgetRepo::new());
        let service: GenericCrudService<Widget, CreateWidgetDto, UpdateWidgetDto, InMemoryWidgetRepo> =
            GenericCrudService::with_lifecycle(repo, Arc::new(RefuseTrash));
        let w = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap();
        service.soft_delete(&w.id).await.unwrap();

        assert!(service.restore(&w.id).await.is_err(), "restore");
        assert!(service.bulk_restore(vec![w.id.clone()]).await.is_err(), "bulk restore");
        assert!(service.restore_all().await.is_err(), "restore all");
        assert!(service.hard_delete(&w.id).await.is_err(), "hard delete");
        assert!(service.bulk_permanent_delete(vec![w.id.clone()]).await.is_err(), "bulk permanent delete");
        assert!(service.empty_trash().await.is_err(), "empty trash");

        // Nothing moved: the row is still in the trash.
        let still = service.get_deleted_by_id(&w.id).await.unwrap().unwrap();
        assert!(still.deleted_at.is_some());
    }

    // ── Named violations and the write guard ──────────────────────────────

    #[tokio::test]
    async fn a_refused_field_is_a_violation_naming_its_path_and_code() {
        let service = svc();
        let w = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap();

        let err = service
            .partial_update(&w.id, patch(&[("secret_hash", serde_json::json!("h1")), ("colour", serde_json::json!("red"))]))
            .await
            .unwrap_err();
        let ServiceError::Violations(v) = &err else { panic!("expected violations, got {err:?}") };
        let pairs: Vec<(&str, &str)> = v.iter().map(|x| (x.path.as_str(), x.code.as_str())).collect();
        assert_eq!(pairs, vec![("colour", "unknown_field")]);
        // The sentence clients already read is unchanged.
        assert!(err.to_string().starts_with("validation failed: unknown_field: `colour`"), "{err}");

        let err = service.partial_update(&w.id, patch(&[("secret_hash", serde_json::json!("h1"))])).await.unwrap_err();
        let ServiceError::Violations(v) = &err else { panic!("expected violations, got {err:?}") };
        assert_eq!((v[0].path.as_str(), v[0].code.as_str()), ("secret_hash", "field_not_writable"));
    }

    /// Refuses any write whose resulting widget is named `forbidden`, and
    /// shadow-reports any named `watched`. Records every kind it was asked about.
    #[derive(Default)]
    struct NameGuard {
        seen: std::sync::Mutex<Vec<crate::write_guard::WriteKind>>,
    }

    #[async_trait]
    impl crate::write_guard::WriteGuard<Widget> for NameGuard {
        async fn check(&self, ctx: &crate::write_guard::WriteCtx<'_, Widget>) -> crate::write_guard::GuardOutcome {
            self.seen.lock().unwrap().push(ctx.kind);
            let row = ctx.after.or(ctx.before).expect("a write concerns a row");
            let mut out = crate::write_guard::GuardOutcome::default();
            match row.name.as_str() {
                "forbidden" => out.refuse.push(crate::violation::Violation::new("name", "name_forbidden", "this name is refused")),
                "watched" => out.shadow.push(crate::violation::Violation::new("name", "name_watched", "would be refused")),
                _ => {}
            }
            out
        }
    }

    fn guarded(guard: Arc<NameGuard>) -> GenericCrudService<Widget, CreateWidgetDto, UpdateWidgetDto, InMemoryWidgetRepo> {
        svc().with_guard(guard)
    }

    #[tokio::test]
    async fn a_guard_refusal_stops_create_update_and_patch_with_nothing_written() {
        let guard = Arc::new(NameGuard::default());
        let service = guarded(guard.clone());

        let err = service.create(CreateWidgetDto { name: "forbidden".into() }).await.unwrap_err();
        assert!(matches!(&err, ServiceError::Violations(v) if v[0].code == "name_forbidden"), "{err:?}");
        assert!(service.list(1, 20, Default::default()).await.unwrap().0.is_empty(), "nothing created");

        let w = service.create(CreateWidgetDto { name: "ok".into() }).await.unwrap();
        assert!(service.update(&w.id, UpdateWidgetDto { name: "forbidden".into() }).await.is_err());
        assert!(service.partial_update(&w.id, patch(&[("name", serde_json::json!("forbidden"))])).await.is_err());
        assert!(service
            .bulk_partial_update(vec![(w.id.clone(), patch(&[("name", serde_json::json!("forbidden"))]))])
            .await
            .is_err());
        assert_eq!(service.get_by_id(&w.id).await.unwrap().unwrap().name, "ok");
    }

    #[tokio::test]
    async fn a_shadowed_rule_lets_the_write_through() {
        let service = guarded(Arc::new(NameGuard::default()));
        let w = service.create(CreateWidgetDto { name: "watched".into() }).await.unwrap();
        assert_eq!(service.get_by_id(&w.id).await.unwrap().unwrap().name, "watched");
    }

    #[tokio::test]
    async fn the_guard_is_asked_about_every_write_kind() {
        use crate::write_guard::WriteKind::*;
        let guard = Arc::new(NameGuard::default());
        let service = guarded(guard.clone());
        let w = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap();
        service.update(&w.id, UpdateWidgetDto { name: "b".into() }).await.unwrap();
        service.partial_update(&w.id, patch(&[("name", serde_json::json!("c"))])).await.unwrap();
        service.soft_delete(&w.id).await.unwrap();
        service.restore(&w.id).await.unwrap();
        service.soft_delete(&w.id).await.unwrap();
        service.hard_delete(&w.id).await.unwrap();
        assert_eq!(*guard.seen.lock().unwrap(), vec![Create, Update, Update, Delete, Restore, Delete, HardDelete]);
    }

    #[test]
    fn the_generic_service_hands_its_violations_to_the_http_layer() {
        use crate::http::CrudService;
        type S = GenericCrudService<Widget, CreateWidgetDto, UpdateWidgetDto, InMemoryWidgetRepo>;
        let v = vec![crate::violation::Violation::new("name", "x", "y")];
        assert_eq!(<S as CrudService<Widget, CreateWidgetDto, UpdateWidgetDto>>::violations_of(&ServiceError::Violations(v.clone())), Some(v.clone()));
        assert_eq!(<S as CrudService<Widget, CreateWidgetDto, UpdateWidgetDto>>::violations_of(&ServiceError::NotFound), None);
        // A constraint the database refused with reaches the HTTP layer as the same 422.
        let db = ServiceError::Repository(RepositoryError::Violations(v.clone()));
        assert_eq!(<S as CrudService<Widget, CreateWidgetDto, UpdateWidgetDto>>::violations_of(&db), Some(v));
    }

    // ── References point only at rows the caller can see ─────────────────

    fn only_violation(res: ServiceResult<impl std::fmt::Debug>) -> crate::violation::Violation {
        match res {
            Err(ServiceError::Violations(mut v)) if v.len() == 1 => v.remove(0),
            other => panic!("expected one violation, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_reference_to_a_hidden_row_is_refused_on_every_write_kind() {
        let service = svc();
        // Create: a hidden owner is refused as missing, and nothing is written.
        let v = only_violation(service.create(CreateWidgetDto { name: "owned-by:theirs".into() }).await);
        assert_eq!((v.path.as_str(), v.code.as_str()), ("owner_id", "reference_missing"));
        assert!(!v.message.contains("owners"), "{}", v.message);
        assert_eq!(service.count().await.unwrap(), 0);
        // A visible owner and no owner are let through.
        let w = service.create(CreateWidgetDto { name: "owned-by:mine".into() }).await.unwrap();
        service.create(CreateWidgetDto { name: "plain".into() }).await.unwrap();

        // PATCH that moves the reference to a hidden row.
        let v = only_violation(service.partial_update(&w.id, patch(&[("owner_id", serde_json::json!("theirs"))])).await);
        assert_eq!((v.path.as_str(), v.code.as_str()), ("owner_id", "reference_missing"));
        assert_eq!(service.get_by_id(&w.id).await.unwrap().unwrap().owner_id.as_deref(), Some("mine"));
        // Bulk create and bulk PATCH name the item.
        let v = only_violation(
            service
                .bulk_create(vec![CreateWidgetDto { name: "x".into() }, CreateWidgetDto { name: "owned-by:theirs".into() }])
                .await,
        );
        assert_eq!((v.path.as_str(), v.code.as_str()), ("1/owner_id", "reference_missing"));
        let v = only_violation(
            service.bulk_partial_update(vec![(w.id.clone(), patch(&[("owner_id", serde_json::json!("theirs"))]))]).await,
        );
        assert_eq!(v.path, "0/owner_id");
    }

    #[tokio::test]
    async fn an_unchanged_reference_is_not_checked_again() {
        let service = svc();
        let w = service.create(CreateWidgetDto { name: "owned-by:mine".into() }).await.unwrap();
        // The owner becomes invisible; an edit that leaves it alone still goes through.
        service.repository().visible_owners.lock().unwrap().clear();
        let saved = service.partial_update(&w.id, patch(&[("note", serde_json::json!("x"))])).await.unwrap().unwrap();
        assert_eq!(saved.owner_id.as_deref(), Some("mine"));
        let saved = service.update(&w.id, UpdateWidgetDto { name: "renamed".into() }).await.unwrap().unwrap();
        assert_eq!(saved.name, "renamed");
    }

    // ── A patch value the entity cannot hold names its field ─────────────

    #[tokio::test]
    async fn a_patch_of_the_wrong_type_or_a_null_required_field_is_a_violation_on_its_path() {
        let service = svc();
        let w = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap();
        let v = only_violation(service.partial_update(&w.id, patch(&[("name", serde_json::json!(5))])).await);
        assert_eq!((v.path.as_str(), v.code.as_str()), ("name", "invalid_value"));
        let v = only_violation(service.partial_update(&w.id, patch(&[("name", serde_json::Value::Null)])).await);
        assert_eq!((v.path.as_str(), v.code.as_str()), ("name", "required"));
        assert_eq!(service.get_by_id(&w.id).await.unwrap().unwrap().name, "a");
    }

    #[tokio::test]
    async fn a_patch_setting_an_empty_protected_field_is_refused() {
        // An `@exclude_from_dto` field reaches the generic service as a protected one.
        let service = svc();
        let w = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap();
        let v = only_violation(service.partial_update(&w.id, patch(&[("secret_hash", serde_json::json!("set"))])).await);
        assert_eq!((v.path.as_str(), v.code.as_str()), ("secret_hash", "field_not_writable"));
    }

    #[tokio::test]
    async fn emptying_a_trash_larger_than_one_page_reaches_every_row() {
        let service = svc();
        let mut ids = Vec::new();
        for i in 0..150 {
            ids.push(service.create(CreateWidgetDto { name: format!("w{i}") }).await.unwrap().id);
        }
        service.bulk_soft_delete(ids).await.unwrap();
        // The in-memory repository caps a page at 100 as the generated ones do, so this proves
        // the trash is walked page by page rather than read as one oversized page.
        assert_eq!(service.empty_trash().await.unwrap(), 150);
        assert_eq!(service.count_deleted().await.unwrap(), 0);
    }

    #[test]
    fn the_entity_name_never_carries_the_module_path() {
        use crate::http::CrudService;
        type S = GenericCrudService<Widget, CreateWidgetDto, UpdateWidgetDto, InMemoryWidgetRepo>;
        assert_eq!(<S as CrudService<Widget, CreateWidgetDto, UpdateWidgetDto>>::entity_name(), "Widget");
    }

    // ── Trash-wide writes ask the guard about every row ───────────────────

    /// Trash two widgets, one renamed behind the service's back to a name the guard refuses.
    async fn trash_with_a_refused_row(
        service: &GenericCrudService<Widget, CreateWidgetDto, UpdateWidgetDto, InMemoryWidgetRepo>,
    ) -> (String, String) {
        let a = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap().id;
        let b = service.create(CreateWidgetDto { name: "b".into() }).await.unwrap().id;
        service.bulk_soft_delete(vec![a.clone(), b.clone()]).await.unwrap();
        let mut data = service.repository().data.lock().await;
        data.iter_mut().find(|w| w.id == b).unwrap().name = "forbidden".into();
        (a, b)
    }

    #[tokio::test]
    async fn restore_all_asks_the_guard_about_every_row_and_restores_none_when_one_is_refused() {
        let guard = Arc::new(NameGuard::default());
        let service = guarded(guard.clone());
        trash_with_a_refused_row(&service).await;

        let err = service.restore_all().await.unwrap_err();
        assert!(matches!(&err, ServiceError::Violations(v) if v[0].code == "name_forbidden"), "{err:?}");
        assert_eq!(service.count_deleted().await.unwrap(), 2, "nothing restored");
        assert!(guard.seen.lock().unwrap().contains(&crate::write_guard::WriteKind::Restore));
    }

    #[tokio::test]
    async fn empty_trash_asks_the_guard_about_every_row_and_deletes_none_when_one_is_refused() {
        let guard = Arc::new(NameGuard::default());
        let service = guarded(guard.clone());
        trash_with_a_refused_row(&service).await;

        let err = service.empty_trash().await.unwrap_err();
        assert!(matches!(&err, ServiceError::Violations(v) if v[0].code == "name_forbidden"), "{err:?}");
        assert_eq!(service.count_deleted().await.unwrap(), 2, "nothing deleted");
        assert!(guard.seen.lock().unwrap().contains(&crate::write_guard::WriteKind::HardDelete));

        // Without the refused row the trash empties.
        let service = svc();
        let a = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap().id;
        service.soft_delete(&a).await.unwrap();
        assert_eq!(service.empty_trash().await.unwrap(), 1);
        assert_eq!(service.count_deleted().await.unwrap(), 0);
    }

    // ── Upsert refuses instead of duplicating ──────────────────────────────

    #[tokio::test]
    async fn upsert_refuses_rather_than_creating_a_duplicate() {
        use crate::http::CrudService;
        type S = GenericCrudService<Widget, CreateWidgetDto, UpdateWidgetDto, InMemoryWidgetRepo>;
        let service = svc();
        assert!(service.upsert(CreateWidgetDto { name: "a".into() }).await.is_err());
        assert!(service.upsert(CreateWidgetDto { name: "a".into() }).await.is_err());
        assert_eq!(service.count().await.unwrap(), 0, "nothing created");
        assert!(!<S as CrudService<Widget, CreateWidgetDto, UpdateWidgetDto>>::supports_upsert());
    }

    // ── Bulk create: capped, one repository call, the failed item named ───

    #[tokio::test]
    async fn bulk_create_is_one_repository_write_and_names_the_refused_item() {
        let service = guarded(Arc::new(NameGuard::default()));
        let names = ["a", "forbidden", "c"];
        let err = service
            .bulk_create(names.iter().map(|n| CreateWidgetDto { name: n.to_string() }).collect())
            .await
            .unwrap_err();
        let ServiceError::Violations(v) = &err else { panic!("expected violations, got {err:?}") };
        assert_eq!((v[0].path.as_str(), v[0].code.as_str()), ("1/name", "name_forbidden"));
        assert_eq!(service.count().await.unwrap(), 0, "nothing created");

        let created = service
            .bulk_create(vec![CreateWidgetDto { name: "a".into() }, CreateWidgetDto { name: "b".into() }])
            .await
            .unwrap();
        assert_eq!(created.len(), 2);
        assert_eq!(
            service.repository().bulk_creates.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "the batch is written by one all-or-nothing repository call"
        );
    }

    #[tokio::test]
    async fn bulk_create_refuses_an_oversized_batch() {
        let service = svc();
        let dtos = (0..=MAX_BATCH_SIZE).map(|n| CreateWidgetDto { name: format!("w{n}") }).collect();
        assert!(matches!(service.bulk_create(dtos).await, Err(ServiceError::Validation(_))));
        assert_eq!(service.count().await.unwrap(), 0);
    }

    // ── Bulk updates write only the changed columns ────────────────────────

    #[tokio::test]
    async fn bulk_updates_write_through_the_changed_columns_path() {
        let service = svc();
        let a = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap().id;
        let b = service.create(CreateWidgetDto { name: "b".into() }).await.unwrap().id;
        service
            .bulk_update(vec![
                (a.clone(), UpdateWidgetDto { name: "a2".into() }),
                (b.clone(), UpdateWidgetDto { name: "b2".into() }),
            ])
            .await
            .unwrap();
        service
            .bulk_partial_update(vec![(a.clone(), patch(&[("name", serde_json::json!("a3"))]))])
            .await
            .unwrap();
        let repo = service.repository();
        assert_eq!(repo.changed_writes.lock().unwrap().len(), 3);
        assert_eq!(repo.full_writes.load(std::sync::atomic::Ordering::SeqCst), 0, "no full-row write");
        assert_eq!(service.get_by_id(&a).await.unwrap().unwrap().name, "a3");
    }

    // ── The trash list takes the list's filters ────────────────────────────

    #[tokio::test]
    async fn the_trash_list_applies_filters() {
        let service = svc();
        let a = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap().id;
        let b = service.create(CreateWidgetDto { name: "b".into() }).await.unwrap().id;
        service.bulk_soft_delete(vec![a, b]).await.unwrap();
        let filters = HashMap::from([("name".to_string(), "b".to_string())]);
        let (rows, total) = service.list_deleted_filtered(1, 20, filters).await.unwrap();
        assert_eq!((rows.len(), total), (1, 1));
        assert_eq!(rows[0].name, "b");
    }

    // ── The generic HTTP surface ───────────────────────────────────────────

    mod http_surface {
        use super::*;
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt;

        type Svc = GenericCrudService<Widget, CreateWidgetDto, UpdateWidgetDto, InMemoryWidgetRepo>;

        fn app() -> (axum::Router, Arc<Svc>) {
            let service = Arc::new(svc());
            let router = crate::http::BackboneCrudHandler::<Svc, Widget, CreateWidgetDto, UpdateWidgetDto, Widget>::routes(
                service.clone(),
                "/widgets",
            );
            (router, service)
        }

        async fn call(app: &axum::Router, method: &str, uri: &str, body: Option<&str>) -> (StatusCode, serde_json::Value) {
            let mut req = Request::builder().method(method).uri(uri);
            if body.is_some() {
                req = req.header("content-type", "application/json");
            }
            let req = req.body(Body::from(body.unwrap_or("").to_string())).unwrap();
            let res = app.clone().oneshot(req).await.unwrap();
            let status = res.status();
            let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
            let json = if bytes.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::from_slice(&bytes).unwrap_or_else(|_| {
                    panic!("not a JSON body: {}", String::from_utf8_lossy(&bytes))
                })
            };
            (status, json)
        }

        #[tokio::test]
        async fn a_malformed_path_id_is_a_400_naming_the_id() {
            let (app, _) = app();
            for (method, uri) in [("GET", "/widgets/not-a-uuid"), ("DELETE", "/widgets/not-a-uuid")] {
                let (status, body) = call(&app, method, uri, None).await;
                assert_eq!(status, StatusCode::BAD_REQUEST, "{method} {uri}");
                assert_eq!(body["violations"][0]["code"], "invalid_id");
                assert_eq!(body["violations"][0]["path"], "id");
            }
        }

        #[tokio::test]
        async fn a_missing_record_answers_a_coded_404_without_the_type_path() {
            let (app, _) = app();
            let missing = uuid::Uuid::new_v4();
            for (method, uri) in [
                ("GET", format!("/widgets/{missing}")),
                ("GET", format!("/widgets/{missing}/deleted")),
                ("DELETE", format!("/widgets/trash/{missing}")),
            ] {
                let (status, body) = call(&app, method, &uri, None).await;
                assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri}");
                assert_eq!(body["violations"][0]["code"], "not_found", "{body}");
                assert_eq!(body["error"], crate::http::NOT_FOUND_MESSAGE);
                assert!(!body.to_string().contains("::"), "{body}");
            }
        }

        #[tokio::test]
        async fn a_list_page_is_clamped_and_page_zero_reads_as_the_first() {
            let (app, service) = app();
            service.create(CreateWidgetDto { name: "a".into() }).await.unwrap();
            let (status, body) = call(&app, "GET", "/widgets?limit=1000&page=0", None).await;
            assert_eq!(status, StatusCode::OK, "{body}");
            assert_eq!(body["meta"]["limit"], crate::http::MAX_PER_PAGE);
            assert_eq!(body["meta"]["page"], 1);
            let (status, body) = call(&app, "GET", "/widgets/trash?limit=1000&page=0", None).await;
            assert_eq!(status, StatusCode::OK, "{body}");
            assert_eq!(body["meta"]["limit"], crate::http::MAX_PER_PAGE);
            // A page too deep is refused, coded and named.
            let (status, body) = call(&app, "GET", "/widgets?limit=100&page=1000", None).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
            assert_eq!(body["violations"][0]["code"], "invalid_query");
            assert_eq!(body["violations"][0]["path"], "page");
            assert_eq!(body["violations"][0]["params"]["reason"], "too_deep");
        }

        #[tokio::test]
        async fn a_list_parameter_the_request_got_wrong_is_refused_by_name() {
            let (app, service) = app();
            service.create(CreateWidgetDto { name: "a".into() }).await.unwrap();
            for (uri, parameter, reason) in [
                ("/widgets?page=abc", "page", "not_a_number"),
                ("/widgets?limit=-1", "limit", "not_a_number"),
                ("/widgets/trash?page=x", "page", "not_a_number"),
                ("/widgets/count?limit=x", "limit", "not_a_number"),
                ("/widgets/aggregate?page=x", "page", "not_a_number"),
                ("/widgets?sort_order=sideways", "sort_order", "invalid_value"),
                ("/widgets/trash?sort_order=up", "sort_order", "invalid_value"),
                ("/widgets?include=nothing", "include", "unknown_relation"),
                ("/widgets?with=nothing", "with", "unknown_relation"),
                ("/widgets/trash?include=nothing", "include", "unknown_relation"),
                ("/widgets?fields=id,na%20me", "fields", "invalid_field_name"),
            ] {
                let (status, body) = call(&app, "GET", uri, None).await;
                assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {body}");
                let v = &body["violations"][0];
                assert_eq!(v["code"], "invalid_query", "{uri}: {body}");
                assert_eq!(v["path"], parameter, "{uri}: {body}");
                assert_eq!(v["params"]["parameter"], parameter, "{uri}: {body}");
                assert_eq!(v["params"]["reason"], reason, "{uri}: {body}");
                assert!(v["message"].as_str().unwrap().contains(parameter), "{uri}: {body}");
                assert_eq!(body["error"], v["message"], "{uri}: {body}");
            }
            // The same parameters, well formed, are served.
            for uri in ["/widgets?page=1&limit=5&sort_order=DESC", "/widgets?fields=id,name", "/widgets/count"] {
                let (status, body) = call(&app, "GET", uri, None).await;
                assert_eq!(status, StatusCode::OK, "{uri}: {body}");
            }
            // A single record's `include` is read the same way.
            let w = service.create(CreateWidgetDto { name: "b".into() }).await.unwrap();
            let (status, body) = call(&app, "GET", &format!("/widgets/{}?include=nothing", w.id), None).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
            assert_eq!(body["violations"][0]["path"], "include");
        }

        #[tokio::test]
        async fn a_refusal_that_names_no_rule_still_carries_a_code() {
            let (app, service) = app();
            let w = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap();
            // A duplicate the service reports as such: 409, coded.
            let (status, body) = call(&app, "PATCH", &format!("/widgets/{}", w.id), Some(r#"{"name":"taken"}"#)).await;
            assert_eq!(status, StatusCode::CONFLICT);
            assert_eq!(body["violations"][0]["code"], "conflict", "{body}");
            // A patch of the wrong type: 422 on the field.
            let (status, body) = call(&app, "PATCH", &format!("/widgets/{}", w.id), Some(r#"{"name":7}"#)).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
            assert_eq!(body["violations"][0]["path"], "name");
        }

        #[tokio::test]
        async fn a_server_fault_answers_generically_and_keeps_the_detail_out() {
            let (app, _) = app();
            let (status, body) = call(&app, "GET", &format!("/widgets/{BROKEN_ID}"), None).await;
            assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
            assert_eq!(body["error"], crate::http::INTERNAL_ERROR_MESSAGE);
            assert!(!body.to_string().contains("secret_schema"), "{body}");
        }

        #[tokio::test]
        async fn a_duplicate_on_put_and_patch_is_a_409_like_on_create() {
            let (app, service) = app();
            let w = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap();
            let uri = format!("/widgets/{}", w.id);
            let (status, _) = call(&app, "PUT", &uri, Some(r#"{"name":"taken"}"#)).await;
            assert_eq!(status, StatusCode::CONFLICT);
            let (status, _) = call(&app, "PATCH", &uri, Some(r#"{"name":"taken"}"#)).await;
            assert_eq!(status, StatusCode::CONFLICT);
        }

        #[tokio::test]
        async fn an_unknown_key_is_refused_on_create_put_and_bulk() {
            let (app, service) = app();
            let (status, body) = call(&app, "POST", "/widgets", Some(r#"{"name":"a","colour":"red"}"#)).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
            assert_eq!(body["violations"][0]["code"], "unknown_field");
            assert_eq!(body["violations"][0]["path"], "colour");
            assert_eq!(service.count().await.unwrap(), 0, "nothing created");

            // What the record's own response carries is tolerated.
            let (status, _) = call(&app, "POST", "/widgets", Some(r#"{"name":"a","id":"x","metadata":{}}"#)).await;
            assert_eq!(status, StatusCode::CREATED);

            let w = service.list(1, 1, HashMap::new()).await.unwrap().0.remove(0);
            let (status, body) = call(&app, "PUT", &format!("/widgets/{}", w.id), Some(r#"{"name":"b","colour":"red"}"#)).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(body["violations"][0]["path"], "colour");

            let (status, body) = call(&app, "POST", "/widgets/bulk", Some(r#"[{"name":"x"},{"name":"y","colour":"red"}]"#)).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(body["violations"][0]["path"], "1/colour");

            let bulk = format!(r#"[{{"id":"{}","name":"z","colour":"red"}}]"#, w.id);
            let (status, body) = call(&app, "PUT", "/widgets/bulk", Some(&bulk)).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
            assert_eq!(body["violations"][0]["path"], "0/colour");
            assert_eq!(service.get_by_id(&w.id).await.unwrap().unwrap().name, "a", "nothing written");
        }

        #[tokio::test]
        async fn a_body_that_cannot_be_read_answers_in_the_json_envelope() {
            let (app, _) = app();
            let (status, body) = call(&app, "POST", "/widgets", Some(r#"{"name": "#)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST);
            assert_eq!(body["success"], false);
            assert_eq!(body["violations"][0]["code"], "malformed_body");

            let (status, body) = call(&app, "POST", "/widgets", Some(r#"{"name": 5}"#)).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(body["violations"][0]["path"], "name");
        }

        #[tokio::test]
        async fn the_generic_upsert_route_answers_501() {
            let (app, service) = app();
            let (status, _) = call(&app, "POST", "/widgets/upsert", Some(r#"{"name":"a"}"#)).await;
            assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
            assert_eq!(service.count().await.unwrap(), 0);
        }

        #[tokio::test]
        async fn the_trash_route_reads_the_filters() {
            let (app, service) = app();
            let a = service.create(CreateWidgetDto { name: "a".into() }).await.unwrap().id;
            let b = service.create(CreateWidgetDto { name: "b".into() }).await.unwrap().id;
            service.bulk_soft_delete(vec![a, b]).await.unwrap();
            let (status, body) = call(&app, "GET", "/widgets/trash?name=b", None).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(body["data"].as_array().unwrap().len(), 1);
            assert_eq!(body["data"][0]["name"], "b");
        }
    }
}
