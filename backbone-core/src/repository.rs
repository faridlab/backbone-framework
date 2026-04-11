//! Repository traits and common implementations

use async_trait::async_trait;
use anyhow::Result;
use uuid::Uuid;
use std::collections::HashMap;

// ─── Domain repository pagination types ──────────────────────────────────────

/// Pagination parameters used by generated domain repository traits.
///
/// Generated files use a type alias:
/// `pub type OrderPaginationParams = DomainPaginationParams;`
#[derive(Debug, Clone, Default)]
pub struct DomainPaginationParams {
    pub page: u32,
    pub per_page: u32,
}

impl DomainPaginationParams {
    pub fn new(page: u32, per_page: u32) -> Self {
        Self { page, per_page }
    }

    pub fn offset(&self) -> u64 {
        ((self.page.saturating_sub(1)) * self.per_page) as u64
    }

    pub fn limit(&self) -> u64 {
        self.per_page as u64
    }
}

/// Paginated result returned by generated domain repository traits.
///
/// Generated files use a type alias:
/// `pub type OrderPaginatedResult = DomainPaginatedResult<Order>;`
#[derive(Debug, Clone)]
pub struct DomainPaginatedResult<E> {
    pub data: Vec<E>,
    pub total: u64,
    pub page: u32,
    pub per_page: u32,
}

/// Generic repository trait with basic CRUD operations
#[async_trait]
pub trait Repository<T> {
    async fn create(&self, entity: &T) -> Result<T>;
    async fn find_by_id(&self, id: &Uuid) -> Result<Option<T>>;
    async fn update(&self, entity: &T) -> Result<T>;
    async fn delete(&self, id: &Uuid) -> Result<bool>;
    async fn list(&self, page: u32, limit: u32) -> Result<Vec<T>>;
}

/// Searchable repository trait
#[async_trait]
pub trait SearchableRepository<T>: Repository<T> {
    async fn search(&self, criteria: HashMap<String, String>, page: u32, limit: u32) -> Result<Vec<T>>;
    async fn count(&self, criteria: HashMap<String, String>) -> Result<u64>;
}

/// Soft deletable repository trait
#[async_trait]
pub trait SoftDeletableRepository<T>: Repository<T> {
    async fn soft_delete(&self, id: &Uuid) -> Result<bool>;
    async fn restore(&self, id: &Uuid) -> Result<bool>;
    async fn list_deleted(&self, page: u32, limit: u32) -> Result<Vec<T>>;
    async fn permanent_delete_all(&self) -> Result<u64>;
}

/// Paginated repository trait
#[async_trait]
pub trait PaginatedRepository<T>: Repository<T> {
    async fn paginate(&self, page: u32, limit: u32) -> Result<(Vec<T>, u64)>;
}

/// Bulk operations repository trait
#[async_trait]
pub trait BulkRepository<T>: Repository<T> {
    async fn bulk_create(&self, entities: Vec<T>) -> Result<Vec<T>>;
    async fn bulk_update(&self, ids: &[Uuid], updates: HashMap<String, String>) -> Result<usize>;
    async fn bulk_delete(&self, ids: &[Uuid]) -> Result<u64>;
}

/// Combination trait for full CRUD functionality
#[async_trait]
pub trait CrudRepository<T>:
    Repository<T> +
    SearchableRepository<T> +
    SoftDeletableRepository<T> +
    PaginatedRepository<T> +
    BulkRepository<T> +
    Send + Sync
{
}