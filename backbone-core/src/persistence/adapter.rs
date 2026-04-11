//! CrudService Adapter
//!
//! Bridges the `CrudRepository` trait (from persistence) to the `CrudService` trait
//! (from HTTP layer), enabling automatic HTTP endpoint generation for any repository.
//!
//! # Usage
//!
//! ```ignore
//! use backbone_core::persistence::{InMemoryRepository, CrudServiceAdapter};
//! use backbone_core::{BackboneCrudHandler};
//!
//! // Create repository
//! let repo = InMemoryRepository::<User>::new();
//!
//! // Wrap with CrudServiceAdapter
//! let service = CrudServiceAdapter::new(
//!     Arc::new(repo),
//!     |dto: CreateUserDto| User { /* convert DTO to entity */ },
//!     |entity: &mut User, dto: UpdateUserDto| { /* apply update */ },
//! );
//!
//! // Generate all 12 HTTP endpoints
//! let routes = BackboneCrudHandler::<_, User, CreateUserDto, UpdateUserDto, UserResponse>
//!     ::routes(Arc::new(service), "/api/v1/users");
//! ```

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use super::traits::{CrudRepository, PersistentEntity, RepositoryError};
use crate::http::CrudService;

/// Type alias for create mapper function
type CreateMapperFn<C, E> = Box<dyn Fn(C) -> E + Send + Sync>;

/// Type alias for update mapper function
type UpdateMapperFn<E, U> = Box<dyn Fn(&mut E, U) + Send + Sync>;

/// Adapter that wraps a `CrudRepository` to implement `CrudService`
///
/// This enables using any repository implementation with the `BackboneCrudHandler`
/// to automatically generate all 12 HTTP endpoints.
///
/// # Type Parameters
///
/// - `R`: Repository implementing `CrudRepository<E>`
/// - `E`: Entity type (must implement `PersistentEntity`)
/// - `C`: Create DTO type
/// - `U`: Update DTO type
pub struct CrudServiceAdapter<R, E, C, U>
where
    R: CrudRepository<E> + Send + Sync,
    E: PersistentEntity,
    C: Send + Sync,
    U: Send + Sync,
{
    repository: Arc<R>,
    /// Function to convert CreateDto to Entity
    create_mapper: CreateMapperFn<C, E>,
    /// Function to apply UpdateDto to Entity
    update_mapper: UpdateMapperFn<E, U>,
    /// Entity name for error messages (reserved for future use)
    #[allow(dead_code)]
    entity_name: &'static str,
}

impl<R, E, C, U> CrudServiceAdapter<R, E, C, U>
where
    R: CrudRepository<E> + Send + Sync,
    E: PersistentEntity,
    C: Send + Sync,
    U: Send + Sync,
{
    /// Create a new adapter with custom mappers
    pub fn new<F1, F2>(
        repository: Arc<R>,
        entity_name: &'static str,
        create_mapper: F1,
        update_mapper: F2,
    ) -> Self
    where
        F1: Fn(C) -> E + Send + Sync + 'static,
        F2: Fn(&mut E, U) + Send + Sync + 'static,
    {
        Self {
            repository,
            create_mapper: Box::new(create_mapper),
            update_mapper: Box::new(update_mapper),
            entity_name,
        }
    }

    /// Get a reference to the underlying repository
    pub fn repository(&self) -> &R {
        &self.repository
    }
}

/// Error type for CrudServiceAdapter
#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("Repository error: {0}")]
    Repository(#[from] RepositoryError),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Not found: {0}")]
    NotFound(String),
}

impl From<AdapterError> for String {
    fn from(e: AdapterError) -> Self {
        e.to_string()
    }
}

#[async_trait]
impl<R, E, C, U> CrudService<E, C, U> for CrudServiceAdapter<R, E, C, U>
where
    R: CrudRepository<E> + Send + Sync + 'static,
    E: PersistentEntity + Clone + 'static,
    C: Send + Sync + 'static,
    U: Send + Sync + 'static,
{
    type Error = AdapterError;

    fn entity_name() -> &'static str {
        // Note: This is a static method, so we can't access self.entity_name
        // For dynamic entity names, consider using a different error approach
        "Entity"
    }

    async fn list(
        &self,
        page: u32,
        limit: u32,
        _filters: HashMap<String, String>,
    ) -> Result<(Vec<E>, u64), Self::Error> {
        // Note: Filter support requires the repository to implement SearchableRepository.
        // For repositories that only implement CrudRepository, filters are ignored.
        // Use SearchableCrudServiceAdapter for full filter support.
        Ok(self.repository.list(page, limit).await?)
    }

    async fn create(&self, dto: C) -> Result<E, Self::Error> {
        let entity = (self.create_mapper)(dto);
        Ok(self.repository.create(entity).await?)
    }

    async fn get_by_id(&self, id: &str) -> Result<Option<E>, Self::Error> {
        Ok(self.repository.find_by_id(id).await?)
    }

    async fn update(&self, id: &str, dto: U) -> Result<Option<E>, Self::Error> {
        let entity = self.repository.find_by_id(id).await?;

        let Some(mut entity) = entity else {
            return Ok(None);
        };

        (self.update_mapper)(&mut entity, dto);
        Ok(Some(self.repository.update(entity).await?))
    }

    async fn partial_update(
        &self,
        id: &str,
        _fields: HashMap<String, serde_json::Value>,
    ) -> Result<Option<E>, Self::Error> {
        // Partial update requires entity to implement PartialUpdatable
        // For now, just return the entity as-is (no-op)
        // Modules can override this behavior
        Ok(self.repository.find_by_id(id).await?)
    }

    async fn soft_delete(&self, id: &str) -> Result<bool, Self::Error> {
        Ok(self.repository.soft_delete(id).await?)
    }

    async fn bulk_create(&self, items: Vec<C>) -> Result<Vec<E>, Self::Error> {
        let entities: Vec<E> = items.into_iter().map(&*self.create_mapper).collect();
        Ok(self.repository.bulk_create(entities).await?)
    }

    async fn upsert(&self, dto: C) -> Result<E, Self::Error> {
        let entity = (self.create_mapper)(dto);
        let id = entity.entity_id();

        // Check if exists
        if let Some(existing) = self.repository.find_by_id(&id).await? {
            // Update existing
            Ok(self.repository.update(existing).await?)
        } else {
            // Create new
            Ok(self.repository.create(entity).await?)
        }
    }

    async fn list_deleted(&self, page: u32, limit: u32) -> Result<(Vec<E>, u64), Self::Error> {
        Ok(self.repository.list_deleted(page, limit).await?)
    }

    async fn restore(&self, id: &str) -> Result<Option<E>, Self::Error> {
        Ok(self.repository.restore(id).await?)
    }

    async fn empty_trash(&self) -> Result<u64, Self::Error> {
        Ok(self.repository.empty_trash().await?)
    }

    async fn get_deleted_by_id(&self, id: &str) -> Result<Option<E>, Self::Error> {
        Ok(self.repository.find_by_id_including_deleted(id).await?)
    }

    async fn permanent_delete(&self, id: &str) -> Result<bool, Self::Error> {
        Ok(self.repository.hard_delete(id).await?)
    }

    async fn list_deleted_filtered(&self, page: u32, limit: u32, filters: std::collections::HashMap<String, String>) -> Result<(Vec<E>, u64), Self::Error> {
        // Default implementation: ignore filters and use regular list_deleted
        let _ = filters;
        Ok(self.repository.list_deleted(page, limit).await?)
    }

    async fn count_active(&self) -> Result<u64, Self::Error> {
        Ok(self.repository.count().await?)
    }

    async fn count_deleted(&self) -> Result<u64, Self::Error> {
        Ok(self.repository.count_deleted().await?)
    }
}

// ============================================================
// Simple Adapter for Identity Mapping
// ============================================================

/// Simple adapter for entities where Create/Update DTOs are the same as Entity
///
/// Use this when your DTOs are identical to your entity type.
pub struct SimpleCrudServiceAdapter<R, E>
where
    R: CrudRepository<E> + Send + Sync,
    E: PersistentEntity + Clone,
{
    repository: Arc<R>,
    /// Entity name for error messages (reserved for future use)
    #[allow(dead_code)]
    entity_name: &'static str,
    _phantom: std::marker::PhantomData<E>,
}

impl<R, E> SimpleCrudServiceAdapter<R, E>
where
    R: CrudRepository<E> + Send + Sync,
    E: PersistentEntity + Clone,
{
    pub fn new(repository: Arc<R>, entity_name: &'static str) -> Self {
        Self {
            repository,
            entity_name,
            _phantom: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<R, E> CrudService<E, E, E> for SimpleCrudServiceAdapter<R, E>
where
    R: CrudRepository<E> + Send + Sync + 'static,
    E: PersistentEntity + Clone + 'static,
{
    type Error = AdapterError;

    fn entity_name() -> &'static str {
        "Entity"
    }

    async fn list(
        &self,
        page: u32,
        limit: u32,
        _filters: HashMap<String, String>,
    ) -> Result<(Vec<E>, u64), Self::Error> {
        Ok(self.repository.list(page, limit).await?)
    }

    async fn create(&self, entity: E) -> Result<E, Self::Error> {
        Ok(self.repository.create(entity).await?)
    }

    async fn get_by_id(&self, id: &str) -> Result<Option<E>, Self::Error> {
        Ok(self.repository.find_by_id(id).await?)
    }

    async fn update(&self, id: &str, entity: E) -> Result<Option<E>, Self::Error> {
        if self.repository.find_by_id(id).await?.is_none() {
            return Ok(None);
        }
        Ok(Some(self.repository.update(entity).await?))
    }

    async fn partial_update(
        &self,
        id: &str,
        _fields: HashMap<String, serde_json::Value>,
    ) -> Result<Option<E>, Self::Error> {
        Ok(self.repository.find_by_id(id).await?)
    }

    async fn soft_delete(&self, id: &str) -> Result<bool, Self::Error> {
        Ok(self.repository.soft_delete(id).await?)
    }

    async fn bulk_create(&self, items: Vec<E>) -> Result<Vec<E>, Self::Error> {
        Ok(self.repository.bulk_create(items).await?)
    }

    async fn upsert(&self, entity: E) -> Result<E, Self::Error> {
        let id = entity.entity_id();
        if self.repository.find_by_id(&id).await?.is_some() {
            Ok(self.repository.update(entity).await?)
        } else {
            Ok(self.repository.create(entity).await?)
        }
    }

    async fn list_deleted(&self, page: u32, limit: u32) -> Result<(Vec<E>, u64), Self::Error> {
        Ok(self.repository.list_deleted(page, limit).await?)
    }

    async fn restore(&self, id: &str) -> Result<Option<E>, Self::Error> {
        Ok(self.repository.restore(id).await?)
    }

    async fn empty_trash(&self) -> Result<u64, Self::Error> {
        Ok(self.repository.empty_trash().await?)
    }

    async fn get_deleted_by_id(&self, id: &str) -> Result<Option<E>, Self::Error> {
        Ok(self.repository.find_by_id_including_deleted(id).await?)
    }

    async fn permanent_delete(&self, id: &str) -> Result<bool, Self::Error> {
        Ok(self.repository.hard_delete(id).await?)
    }

    async fn list_deleted_filtered(&self, page: u32, limit: u32, filters: std::collections::HashMap<String, String>) -> Result<(Vec<E>, u64), Self::Error> {
        // Default implementation: ignore filters and use regular list_deleted
        let _ = filters;
        Ok(self.repository.list_deleted(page, limit).await?)
    }

    async fn count_active(&self) -> Result<u64, Self::Error> {
        Ok(self.repository.count().await?)
    }

    async fn count_deleted(&self) -> Result<u64, Self::Error> {
        Ok(self.repository.count_deleted().await?)
    }
}

// ============================================================
// Searchable Adapter with Filter Support
// ============================================================

use super::traits::SearchableRepository;

/// Adapter for repositories that implement `SearchableRepository`
///
/// This adapter provides full filter support in the `list` method by leveraging
/// the `SearchableRepository::search` method.
///
/// # Example
///
/// ```ignore
/// use backbone_core::persistence::{InMemoryRepository, SearchableCrudServiceAdapter};
///
/// let repo = Arc::new(InMemoryRepository::<User>::new());
/// let service = SearchableCrudServiceAdapter::new(
///     repo,
///     "User",
///     |dto: CreateUserDto| User::from(dto),
///     |entity: &mut User, dto: UpdateUserDto| entity.apply_update(dto),
/// );
/// ```
pub struct SearchableCrudServiceAdapter<R, E, C, U>
where
    R: SearchableRepository<E> + Send + Sync,
    E: PersistentEntity,
    C: Send + Sync,
    U: Send + Sync,
{
    repository: Arc<R>,
    create_mapper: CreateMapperFn<C, E>,
    update_mapper: UpdateMapperFn<E, U>,
    #[allow(dead_code)]
    entity_name: &'static str,
}

impl<R, E, C, U> SearchableCrudServiceAdapter<R, E, C, U>
where
    R: SearchableRepository<E> + Send + Sync,
    E: PersistentEntity,
    C: Send + Sync,
    U: Send + Sync,
{
    /// Create a new searchable adapter with custom mappers
    pub fn new<F1, F2>(
        repository: Arc<R>,
        entity_name: &'static str,
        create_mapper: F1,
        update_mapper: F2,
    ) -> Self
    where
        F1: Fn(C) -> E + Send + Sync + 'static,
        F2: Fn(&mut E, U) + Send + Sync + 'static,
    {
        Self {
            repository,
            create_mapper: Box::new(create_mapper),
            update_mapper: Box::new(update_mapper),
            entity_name,
        }
    }

    /// Get a reference to the underlying repository
    pub fn repository(&self) -> &R {
        &self.repository
    }
}

#[async_trait]
impl<R, E, C, U> CrudService<E, C, U> for SearchableCrudServiceAdapter<R, E, C, U>
where
    R: SearchableRepository<E> + Send + Sync + 'static,
    E: PersistentEntity + Clone + 'static,
    C: Send + Sync + 'static,
    U: Send + Sync + 'static,
{
    type Error = AdapterError;

    fn entity_name() -> &'static str {
        "Entity"
    }

    async fn list(
        &self,
        page: u32,
        limit: u32,
        filters: HashMap<String, String>,
    ) -> Result<(Vec<E>, u64), Self::Error> {
        // Use SearchableRepository::search for full filter support
        if filters.is_empty() {
            Ok(self.repository.list(page, limit).await?)
        } else {
            Ok(self.repository.search(filters, page, limit).await?)
        }
    }

    async fn create(&self, dto: C) -> Result<E, Self::Error> {
        let entity = (self.create_mapper)(dto);
        Ok(self.repository.create(entity).await?)
    }

    async fn get_by_id(&self, id: &str) -> Result<Option<E>, Self::Error> {
        Ok(self.repository.find_by_id(id).await?)
    }

    async fn update(&self, id: &str, dto: U) -> Result<Option<E>, Self::Error> {
        let entity = self.repository.find_by_id(id).await?;

        let Some(mut entity) = entity else {
            return Ok(None);
        };

        (self.update_mapper)(&mut entity, dto);
        Ok(Some(self.repository.update(entity).await?))
    }

    async fn partial_update(
        &self,
        id: &str,
        _fields: HashMap<String, serde_json::Value>,
    ) -> Result<Option<E>, Self::Error> {
        Ok(self.repository.find_by_id(id).await?)
    }

    async fn soft_delete(&self, id: &str) -> Result<bool, Self::Error> {
        Ok(self.repository.soft_delete(id).await?)
    }

    async fn bulk_create(&self, items: Vec<C>) -> Result<Vec<E>, Self::Error> {
        let entities: Vec<E> = items.into_iter().map(&*self.create_mapper).collect();
        Ok(self.repository.bulk_create(entities).await?)
    }

    async fn upsert(&self, dto: C) -> Result<E, Self::Error> {
        let entity = (self.create_mapper)(dto);
        let id = entity.entity_id();

        if let Some(existing) = self.repository.find_by_id(&id).await? {
            Ok(self.repository.update(existing).await?)
        } else {
            Ok(self.repository.create(entity).await?)
        }
    }

    async fn list_deleted(&self, page: u32, limit: u32) -> Result<(Vec<E>, u64), Self::Error> {
        Ok(self.repository.list_deleted(page, limit).await?)
    }

    async fn restore(&self, id: &str) -> Result<Option<E>, Self::Error> {
        Ok(self.repository.restore(id).await?)
    }

    async fn empty_trash(&self) -> Result<u64, Self::Error> {
        Ok(self.repository.empty_trash().await?)
    }

    async fn get_deleted_by_id(&self, id: &str) -> Result<Option<E>, Self::Error> {
        Ok(self.repository.find_by_id_including_deleted(id).await?)
    }

    async fn permanent_delete(&self, id: &str) -> Result<bool, Self::Error> {
        Ok(self.repository.hard_delete(id).await?)
    }

    async fn list_deleted_filtered(&self, page: u32, limit: u32, filters: std::collections::HashMap<String, String>) -> Result<(Vec<E>, u64), Self::Error> {
        // Default implementation: ignore filters and use regular list_deleted
        let _ = filters;
        Ok(self.repository.list_deleted(page, limit).await?)
    }

    async fn count_active(&self) -> Result<u64, Self::Error> {
        Ok(self.repository.count().await?)
    }

    async fn count_deleted(&self) -> Result<u64, Self::Error> {
        Ok(self.repository.count_deleted().await?)
    }
}
