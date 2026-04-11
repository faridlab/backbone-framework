//! Generic use cases — one per CRUD operation, composable via hooks.
//!
//! Generated code emits a type alias per entity:
//!
//! ```rust,ignore
//! // Generated:
//! pub type CreateStoredFileUseCase = CreateUseCase<StoredFile, CreateStoredFileDto, StoredFileService>;
//! pub type UpdateStoredFileUseCase = UpdateUseCase<StoredFile, UpdateStoredFileDto, StoredFileService>;
//! // ... etc.
//! ```
//!
//! Custom behaviour is injected through `UseCaseHooks<E, DTO>` — no generated
//! code modification required.
//!
//! # Lifecycle (Create example)
//!
//! ```text
//! before_create(dto) → build entity → validate → after_create(entity) → persist → publish event
//! ```

use async_trait::async_trait;
use std::marker::PhantomData;
use std::sync::Arc;

use crate::validation::ValidationErrors;

// ─── Use case result ─────────────────────────────────────────────────────────

/// The standard error type for all use case operations.
#[derive(Debug)]
pub enum UseCaseError {
    Validation(ValidationErrors),
    NotFound(String),
    Forbidden(String),
    Conflict(String),
    Internal(String),
}

impl std::fmt::Display for UseCaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UseCaseError::Validation(errs) => write!(f, "validation failed: {errs}"),
            UseCaseError::NotFound(msg) => write!(f, "not found: {msg}"),
            UseCaseError::Forbidden(msg) => write!(f, "forbidden: {msg}"),
            UseCaseError::Conflict(msg) => write!(f, "conflict: {msg}"),
            UseCaseError::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl std::error::Error for UseCaseError {}

impl From<ValidationErrors> for UseCaseError {
    fn from(errs: ValidationErrors) -> Self {
        UseCaseError::Validation(errs)
    }
}

pub type UseCaseResult<T> = Result<T, UseCaseError>;

// ─── Hooks ───────────────────────────────────────────────────────────────────

/// Extension points for every CRUD use case.
///
/// The default implementation (`DefaultHooks`) does nothing.
/// Override individual methods in your custom decorator to inject behaviour.
#[async_trait]
pub trait UseCaseHooks<E: Send + Sync + 'static, DTO: Send + Sync + 'static>: Send + Sync {
    /// Called before the entity is built from `dto`.  Mutate the DTO if needed.
    async fn before_create(&self, dto: &mut DTO) -> UseCaseResult<()> {
        let _ = dto;
        Ok(())
    }

    /// Called after the entity is built, before it is persisted.
    async fn after_build(&self, entity: &mut E) -> UseCaseResult<()> {
        let _ = entity;
        Ok(())
    }

    /// Called after the entity is persisted.
    async fn after_create(&self, entity: &E) -> UseCaseResult<()> {
        let _ = entity;
        Ok(())
    }

    /// Called before an update is applied.
    async fn before_update(&self, entity: &E, dto: &mut DTO) -> UseCaseResult<()> {
        let _ = (entity, dto);
        Ok(())
    }

    /// Called after an update is persisted.
    async fn after_update(&self, entity: &E) -> UseCaseResult<()> {
        let _ = entity;
        Ok(())
    }

    /// Called before a delete.
    async fn before_delete(&self, entity: &E) -> UseCaseResult<()> {
        let _ = entity;
        Ok(())
    }

    /// Called after a delete.
    async fn after_delete(&self, id: &str) -> UseCaseResult<()> {
        let _ = id;
        Ok(())
    }
}

/// Default no-op hooks implementation.
pub struct DefaultHooks<E, DTO> {
    _phantom: PhantomData<(E, DTO)>,
}

impl<E, DTO> DefaultHooks<E, DTO> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<E, DTO> Default for DefaultHooks<E, DTO> {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<E: Send + Sync + 'static, DTO: Send + Sync + 'static> UseCaseHooks<E, DTO>
    for DefaultHooks<E, DTO>
{
    // All methods inherit the default (no-op) implementations.
}

// ─── Service contract ────────────────────────────────────────────────────────

/// The minimal persistence + lookup contract required by all use cases.
///
/// `GenericCrudService` from `service.rs` implements this, as will
/// any custom service that wraps it.
#[async_trait]
pub trait UseCaseService<E: Send + Sync + 'static>: Send + Sync {
    async fn find_by_id(&self, id: &str) -> UseCaseResult<Option<E>>;
    async fn persist(&self, entity: E) -> UseCaseResult<E>;
    async fn remove(&self, id: &str) -> UseCaseResult<()>;
}

// ─── Entity factory ──────────────────────────────────────────────────────────

/// Builds an entity from a Create DTO.
///
/// Generated code emits one impl per entity; custom code can replace it
/// by wrapping the use case with a `UseCaseHooks` implementation.
pub trait EntityFactory<E, DTO>: Send + Sync {
    fn build(&self, dto: DTO) -> UseCaseResult<E>;
}

/// Applies update fields from an Update DTO to an existing entity.
pub trait EntityUpdater<E, DTO>: Send + Sync {
    fn apply(&self, entity: E, dto: DTO) -> UseCaseResult<E>;
}

// ─── Create use case ─────────────────────────────────────────────────────────

/// Generic "create entity" use case.
///
/// Lifecycle: `hooks.before_create(dto)` → `factory.build(dto)` →
/// `hooks.after_build(entity)` → `service.persist(entity)` →
/// `hooks.after_create(entity)`
pub struct CreateUseCase<E, DTO, S> {
    service: Arc<S>,
    factory: Arc<dyn EntityFactory<E, DTO>>,
    hooks: Arc<dyn UseCaseHooks<E, DTO>>,
    _phantom: PhantomData<(E, DTO)>,
}

impl<E, DTO, S> CreateUseCase<E, DTO, S>
where
    E: Send + Sync + Clone + 'static,
    DTO: Send + Sync + 'static,
    S: UseCaseService<E>,
{
    pub fn new(
        service: Arc<S>,
        factory: Arc<dyn EntityFactory<E, DTO>>,
        hooks: Arc<dyn UseCaseHooks<E, DTO>>,
    ) -> Self {
        Self {
            service,
            factory,
            hooks,
            _phantom: PhantomData,
        }
    }

    pub fn with_default_hooks(
        service: Arc<S>,
        factory: Arc<dyn EntityFactory<E, DTO>>,
    ) -> Self
    where
        DTO: 'static,
    {
        Self::new(service, factory, Arc::new(DefaultHooks::new()))
    }

    pub async fn execute(&self, mut dto: DTO) -> UseCaseResult<E> {
        self.hooks.before_create(&mut dto).await?;
        let mut entity = self.factory.build(dto)?;
        self.hooks.after_build(&mut entity).await?;
        let entity = self.service.persist(entity).await?;
        self.hooks.after_create(&entity).await?;
        Ok(entity)
    }
}

// ─── Update use case ─────────────────────────────────────────────────────────

/// Generic "update entity" use case.
pub struct UpdateUseCase<E, DTO, S> {
    service: Arc<S>,
    updater: Arc<dyn EntityUpdater<E, DTO>>,
    hooks: Arc<dyn UseCaseHooks<E, DTO>>,
    _phantom: PhantomData<(E, DTO)>,
}

impl<E, DTO, S> UpdateUseCase<E, DTO, S>
where
    E: Send + Sync + Clone + 'static,
    DTO: Send + Sync + Clone + 'static,
    S: UseCaseService<E>,
{
    pub fn new(
        service: Arc<S>,
        updater: Arc<dyn EntityUpdater<E, DTO>>,
        hooks: Arc<dyn UseCaseHooks<E, DTO>>,
    ) -> Self {
        Self {
            service,
            updater,
            hooks,
            _phantom: PhantomData,
        }
    }

    pub async fn execute(&self, id: &str, mut dto: DTO) -> UseCaseResult<E> {
        let entity = self
            .service
            .find_by_id(id)
            .await?
            .ok_or_else(|| UseCaseError::NotFound(id.to_string()))?;

        self.hooks.before_update(&entity, &mut dto).await?;
        let updated = self.updater.apply(entity, dto)?;
        let persisted = self.service.persist(updated).await?;
        self.hooks.after_update(&persisted).await?;
        Ok(persisted)
    }
}

// ─── Get use case ────────────────────────────────────────────────────────────

/// Generic "get by id" use case.
pub struct GetUseCase<E, S> {
    service: Arc<S>,
    _phantom: PhantomData<E>,
}

impl<E, S> GetUseCase<E, S>
where
    E: Send + Sync + 'static,
    S: UseCaseService<E>,
{
    pub fn new(service: Arc<S>) -> Self {
        Self {
            service,
            _phantom: PhantomData,
        }
    }

    pub async fn execute(&self, id: &str) -> UseCaseResult<E> {
        self.service
            .find_by_id(id)
            .await?
            .ok_or_else(|| UseCaseError::NotFound(id.to_string()))
    }
}

// ─── Delete use case ─────────────────────────────────────────────────────────

/// Generic "delete entity" use case (soft-delete by default).
pub struct DeleteUseCase<E, S> {
    service: Arc<S>,
    hooks: Arc<dyn UseCaseHooks<E, ()>>,
    _phantom: PhantomData<E>,
}

impl<E, S> DeleteUseCase<E, S>
where
    E: Send + Sync + Clone + 'static,
    S: UseCaseService<E>,
{
    pub fn new(service: Arc<S>, hooks: Arc<dyn UseCaseHooks<E, ()>>) -> Self {
        Self {
            service,
            hooks,
            _phantom: PhantomData,
        }
    }

    pub fn with_default_hooks(service: Arc<S>) -> Self {
        Self::new(service, Arc::new(DefaultHooks::new()))
    }

    pub async fn execute(&self, id: &str) -> UseCaseResult<()> {
        let entity = self
            .service
            .find_by_id(id)
            .await?
            .ok_or_else(|| UseCaseError::NotFound(id.to_string()))?;

        self.hooks.before_delete(&entity).await?;
        self.service.remove(id).await?;
        self.hooks.after_delete(id).await?;
        Ok(())
    }
}

// ─── List use case ───────────────────────────────────────────────────────────

/// Paginated list parameters.
#[derive(Debug, Clone)]
pub struct ListParams {
    pub page: u64,
    pub limit: u64,
    pub filters: std::collections::HashMap<String, String>,
}

impl ListParams {
    pub fn new(page: u64, limit: u64) -> Self {
        Self {
            page,
            limit,
            filters: Default::default(),
        }
    }

    pub fn with_filter(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.filters.insert(key.into(), value.into());
        self
    }
}

impl Default for ListParams {
    fn default() -> Self {
        Self::new(1, 20)
    }
}

/// Paginated list result.
#[derive(Debug, Clone)]
pub struct ListResult<E> {
    pub items: Vec<E>,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
}

impl<E> ListResult<E> {
    pub fn new(items: Vec<E>, total: u64, page: u64, limit: u64) -> Self {
        Self {
            items,
            total,
            page,
            limit,
        }
    }

    pub fn total_pages(&self) -> u64 {
        if self.limit == 0 {
            return 0;
        }
        (self.total + self.limit - 1) / self.limit
    }
}

/// Service contract for the list use case (separate because it needs pagination).
#[async_trait]
pub trait ListService<E: Send + Sync + 'static>: Send + Sync {
    async fn list(&self, params: ListParams) -> UseCaseResult<ListResult<E>>;
}

/// Generic paginated list use case.
pub struct ListUseCase<E, S> {
    service: Arc<S>,
    _phantom: PhantomData<E>,
}

impl<E, S> ListUseCase<E, S>
where
    E: Send + Sync + 'static,
    S: ListService<E>,
{
    pub fn new(service: Arc<S>) -> Self {
        Self {
            service,
            _phantom: PhantomData,
        }
    }

    pub async fn execute(&self, params: ListParams) -> UseCaseResult<ListResult<E>> {
        self.service.list(params).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct Foo {
        id: String,
        name: String,
    }

    #[derive(Debug, Clone)]
    struct CreateFooDto {
        name: String,
    }

    struct FooFactory;
    impl EntityFactory<Foo, CreateFooDto> for FooFactory {
        fn build(&self, dto: CreateFooDto) -> UseCaseResult<Foo> {
            Ok(Foo {
                id: "new-id".into(),
                name: dto.name,
            })
        }
    }

    struct FooService {
        store: tokio::sync::Mutex<Vec<Foo>>,
    }

    impl FooService {
        fn new() -> Self {
            Self {
                store: tokio::sync::Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl UseCaseService<Foo> for FooService {
        async fn find_by_id(&self, id: &str) -> UseCaseResult<Option<Foo>> {
            let store = self.store.lock().await;
            Ok(store.iter().find(|f| f.id == id).cloned())
        }

        async fn persist(&self, entity: Foo) -> UseCaseResult<Foo> {
            let mut store = self.store.lock().await;
            store.retain(|f| f.id != entity.id);
            store.push(entity.clone());
            Ok(entity)
        }

        async fn remove(&self, id: &str) -> UseCaseResult<()> {
            let mut store = self.store.lock().await;
            store.retain(|f| f.id != id);
            Ok(())
        }
    }

    #[tokio::test]
    async fn create_use_case_persists_entity() {
        let service = Arc::new(FooService::new());
        let use_case =
            CreateUseCase::with_default_hooks(service.clone(), Arc::new(FooFactory));

        let result = use_case
            .execute(CreateFooDto {
                name: "hello".into(),
            })
            .await
            .unwrap();

        assert_eq!(result.name, "hello");

        let found = service.find_by_id("new-id").await.unwrap();
        assert!(found.is_some());
    }

    #[tokio::test]
    async fn get_use_case_returns_not_found() {
        let service = Arc::new(FooService::new());
        let use_case = GetUseCase::new(service);
        let err = use_case.execute("missing").await.unwrap_err();
        assert!(matches!(err, UseCaseError::NotFound(_)));
    }

    #[tokio::test]
    async fn delete_use_case_removes_entity() {
        let service = Arc::new(FooService::new());
        service
            .persist(Foo {
                id: "x".into(),
                name: "X".into(),
            })
            .await
            .unwrap();

        let use_case = DeleteUseCase::with_default_hooks(service.clone());
        use_case.execute("x").await.unwrap();

        assert!(service.find_by_id("x").await.unwrap().is_none());
    }
}
