//! Generic CQRS commands, queries, and their handlers.
//!
//! Generated code emits type aliases only — zero logic per entity:
//!
//! ```rust,ignore
//! // Generated (replaces ~200 lines per entity):
//! pub type CreateOrderCommand  = GenericCreateCommand<Order, CreateOrderDto>;
//! pub type UpdateOrderCommand  = GenericUpdateCommand<Order, UpdateOrderDto>;
//! pub type DeleteOrderCommand  = GenericDeleteCommand<Order>;
//! pub type GetOrderQuery       = GenericGetQuery<Order>;
//! pub type ListOrderQuery      = GenericListQuery<Order, OrderFilters>;
//!
//! pub type OrderCommandHandler = GenericCommandHandler<Order, CreateOrderDto, UpdateOrderDto, OrderService>;
//! pub type OrderQueryHandler   = GenericQueryHandler<Order, OrderFilters, OrderService>;
//! ```
//!
//! Custom command/query types (beyond CRUD) live in the `// <<< CUSTOM` zone
//! and implement the standard `Command` / `Query` traits from `command.rs` / `query.rs`.

use async_trait::async_trait;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;

use crate::service::{ServiceError, ServiceResult};

// ─── Generic commands ─────────────────────────────────────────────────────────

/// Generic "create entity" command.
///
/// `E` — entity type (phantom, for type-safety at dispatch)
/// `DTO` — the create DTO carried by the command
#[derive(Debug, Clone)]
pub struct GenericCreateCommand<E, DTO> {
    pub payload: DTO,
    pub correlation_id: Option<String>,
    _phantom: PhantomData<E>,
}

impl<E, DTO> GenericCreateCommand<E, DTO> {
    pub fn new(payload: DTO) -> Self {
        Self {
            payload,
            correlation_id: None,
            _phantom: PhantomData,
        }
    }

    pub fn with_correlation(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = Some(id.into());
        self
    }
}

impl<E: Send + Sync, DTO: Send + Sync> crate::command::Command
    for GenericCreateCommand<E, DTO>
{
    type Result = E;
}

/// Generic "full update entity" command.
#[derive(Debug, Clone)]
pub struct GenericUpdateCommand<E, DTO> {
    pub id: String,
    pub payload: DTO,
    pub correlation_id: Option<String>,
    _phantom: PhantomData<E>,
}

impl<E, DTO> GenericUpdateCommand<E, DTO> {
    pub fn new(id: impl Into<String>, payload: DTO) -> Self {
        Self {
            id: id.into(),
            payload,
            correlation_id: None,
            _phantom: PhantomData,
        }
    }
}

impl<E: Send + Sync, DTO: Send + Sync> crate::command::Command
    for GenericUpdateCommand<E, DTO>
{
    type Result = Option<E>;
}

/// Generic "soft-delete entity" command.
#[derive(Debug, Clone)]
pub struct GenericDeleteCommand<E> {
    pub id: String,
    _phantom: PhantomData<E>,
}

impl<E> GenericDeleteCommand<E> {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            _phantom: PhantomData,
        }
    }
}

impl<E: Send + Sync> crate::command::Command for GenericDeleteCommand<E> {
    type Result = bool;
}

/// Generic "restore soft-deleted entity" command.
#[derive(Debug, Clone)]
pub struct GenericRestoreCommand<E> {
    pub id: String,
    _phantom: PhantomData<E>,
}

impl<E> GenericRestoreCommand<E> {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            _phantom: PhantomData,
        }
    }
}

impl<E: Send + Sync> crate::command::Command for GenericRestoreCommand<E> {
    type Result = Option<E>;
}

// ─── Generic queries ──────────────────────────────────────────────────────────

/// Generic "get entity by id" query.
#[derive(Debug, Clone)]
pub struct GenericGetQuery<E> {
    pub id: String,
    _phantom: PhantomData<E>,
}

impl<E> GenericGetQuery<E> {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            _phantom: PhantomData,
        }
    }
}

impl<E: Send + Sync> crate::query::Query for GenericGetQuery<E> {
    type Result = Option<E>;
}

/// Generic "list entities" query.
///
/// `F` — filter type (module-defined struct or `HashMap<String, String>`)
#[derive(Debug, Clone)]
pub struct GenericListQuery<E, F = HashMap<String, String>> {
    pub page: u32,
    pub limit: u32,
    pub filters: F,
    _phantom: PhantomData<E>,
}

impl<E, F: Default> GenericListQuery<E, F> {
    pub fn new(page: u32, limit: u32) -> Self {
        Self {
            page,
            limit,
            filters: F::default(),
            _phantom: PhantomData,
        }
    }

    pub fn with_filters(mut self, filters: F) -> Self {
        self.filters = filters;
        self
    }
}

impl<E: Send + Sync, F: Send + Sync> crate::query::Query for GenericListQuery<E, F> {
    type Result = (Vec<E>, u64);
}

/// Generic "list deleted entities" query.
#[derive(Debug, Clone)]
pub struct GenericListDeletedQuery<E> {
    pub page: u32,
    pub limit: u32,
    _phantom: PhantomData<E>,
}

impl<E> GenericListDeletedQuery<E> {
    pub fn new(page: u32, limit: u32) -> Self {
        Self {
            page,
            limit,
            _phantom: PhantomData,
        }
    }
}

impl<E: Send + Sync> crate::query::Query for GenericListDeletedQuery<E> {
    type Result = (Vec<E>, u64);
}

// ─── Generic command handler ──────────────────────────────────────────────────

/// Handles all standard CRUD commands for entity `E`.
///
/// Wraps `GenericCrudService<E,C,U,R>` and implements the `CommandHandler` trait
/// for each of the 4 generic command types.
///
/// `E` — entity  `C` — create DTO  `U` — update DTO  `S` — service
pub struct GenericCommandHandler<E, C, U, S>
where
    E: Clone + Send + Sync + 'static,
    C: Clone + Send + Sync + 'static,
    U: Clone + Send + Sync + 'static,
    S: Send + Sync + 'static,
{
    service: Arc<S>,
    _phantom: PhantomData<(E, C, U)>,
}

impl<E, C, U, S> GenericCommandHandler<E, C, U, S>
where
    E: Clone + Send + Sync + 'static,
    C: Clone + Send + Sync + 'static,
    U: Clone + Send + Sync + 'static,
    S: Send + Sync + 'static,
{
    pub fn new(service: Arc<S>) -> Self {
        Self {
            service,
            _phantom: PhantomData,
        }
    }
}

/// Service contract required by the generic command handler.
#[async_trait]
pub trait CqrsService<E, C, U>: Send + Sync + 'static
where
    E: Clone + Send + Sync + 'static,
    C: Send + Sync + 'static,
    U: Send + Sync + 'static,
{
    async fn create(&self, dto: C) -> ServiceResult<E>;
    async fn update(&self, id: &str, dto: U) -> ServiceResult<Option<E>>;
    async fn soft_delete(&self, id: &str) -> ServiceResult<bool>;
    async fn restore(&self, id: &str) -> ServiceResult<Option<E>>;
    async fn get_by_id(&self, id: &str) -> ServiceResult<Option<E>>;
    async fn list(&self, page: u32, limit: u32, filters: HashMap<String, String>) -> ServiceResult<(Vec<E>, u64)>;
    async fn list_deleted(&self, page: u32, limit: u32) -> ServiceResult<(Vec<E>, u64)>;
}

// CommandHandler impl for CreateCommand
#[async_trait]
impl<E, C, U, S> crate::command::CommandHandler<GenericCreateCommand<E, C>>
    for GenericCommandHandler<E, C, U, S>
where
    E: Clone + Send + Sync + 'static,
    C: Clone + Send + Sync + 'static,
    U: Clone + Send + Sync + 'static,
    S: CqrsService<E, C, U>,
{
    type Error = ServiceError;

    async fn handle(&self, command: GenericCreateCommand<E, C>) -> Result<E, Self::Error> {
        self.service.create(command.payload).await
    }
}

// CommandHandler impl for UpdateCommand
#[async_trait]
impl<E, C, U, S> crate::command::CommandHandler<GenericUpdateCommand<E, U>>
    for GenericCommandHandler<E, C, U, S>
where
    E: Clone + Send + Sync + 'static,
    C: Clone + Send + Sync + 'static,
    U: Clone + Send + Sync + 'static,
    S: CqrsService<E, C, U>,
{
    type Error = ServiceError;

    async fn handle(&self, command: GenericUpdateCommand<E, U>) -> Result<Option<E>, Self::Error> {
        self.service.update(&command.id, command.payload).await
    }
}

// CommandHandler impl for DeleteCommand
#[async_trait]
impl<E, C, U, S> crate::command::CommandHandler<GenericDeleteCommand<E>>
    for GenericCommandHandler<E, C, U, S>
where
    E: Clone + Send + Sync + 'static,
    C: Clone + Send + Sync + 'static,
    U: Clone + Send + Sync + 'static,
    S: CqrsService<E, C, U>,
{
    type Error = ServiceError;

    async fn handle(&self, command: GenericDeleteCommand<E>) -> Result<bool, Self::Error> {
        self.service.soft_delete(&command.id).await
    }
}

// ─── Generic query handler ────────────────────────────────────────────────────

/// Handles all standard CRUD queries for entity `E`.
pub struct GenericQueryHandler<E, F, S>
where
    E: Clone + Send + Sync + 'static,
    F: Clone + Send + Sync + Default + 'static,
    S: Send + Sync + 'static,
{
    service: Arc<S>,
    _phantom: PhantomData<(E, F)>,
}

impl<E, F, S> GenericQueryHandler<E, F, S>
where
    E: Clone + Send + Sync + 'static,
    F: Clone + Send + Sync + Default + 'static,
    S: Send + Sync + 'static,
{
    pub fn new(service: Arc<S>) -> Self {
        Self {
            service,
            _phantom: PhantomData,
        }
    }
}

/// Minimal read-only service contract for query handlers.
/// Implemented by `GenericCrudService` and any custom service wrapper.
#[async_trait]
pub trait CqrsReadService<E>: Send + Sync + 'static
where
    E: Clone + Send + Sync + 'static,
{
    async fn get_by_id(&self, id: &str) -> ServiceResult<Option<E>>;
    async fn list(&self, page: u32, limit: u32, filters: HashMap<String, String>) -> ServiceResult<(Vec<E>, u64)>;
    async fn list_deleted(&self, page: u32, limit: u32) -> ServiceResult<(Vec<E>, u64)>;
}

// QueryHandler impl for GetQuery
#[async_trait]
impl<E, F, S> crate::query::QueryHandler<GenericGetQuery<E>>
    for GenericQueryHandler<E, F, S>
where
    E: Clone + Send + Sync + 'static,
    F: Clone + Send + Sync + Default + 'static,
    S: CqrsReadService<E>,
{
    type Error = ServiceError;

    async fn handle(&self, query: GenericGetQuery<E>) -> Result<Option<E>, Self::Error> {
        self.service.get_by_id(&query.id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_command_carries_payload() {
        let cmd: GenericCreateCommand<String, i32> =
            GenericCreateCommand::new(42).with_correlation("corr-1");
        assert_eq!(cmd.payload, 42);
        assert_eq!(cmd.correlation_id.as_deref(), Some("corr-1"));
    }

    #[test]
    fn delete_command_carries_id() {
        let cmd: GenericDeleteCommand<String> = GenericDeleteCommand::new("entity-1");
        assert_eq!(cmd.id, "entity-1");
    }

    #[test]
    fn list_query_default_filters() {
        let q: GenericListQuery<String> = GenericListQuery::new(1, 20);
        assert_eq!(q.page, 1);
        assert_eq!(q.limit, 20);
        assert!(q.filters.is_empty());
    }
}
