//! Generic GraphQL resolver — mirrors `BackboneCrudHandler` for the GraphQL transport.
//!
//! Generated code emits a type alias per entity:
//!
//! ```rust,ignore
//! // Generated (was ~300 lines of async-graphql boilerplate):
//! pub type OrderGraphQLResolver = GenericGraphQLResolver<
//!     Order, CreateOrderDto, UpdateOrderDto, OrderService
//! >;
//! ```
//!
//! The resolver exposes the standard 11 CRUD operations as GraphQL
//! queries and mutations.  Entity-specific computed fields and
//! subscriptions go in the `// <<< CUSTOM` decorator.
//!
//! # Note on framework coupling
//!
//! This module deliberately avoids a hard dependency on `async-graphql` or
//! any other GraphQL library.  The `GenericGraphQLResolver` is a thin
//! service-holder; concrete GraphQL objects (#[Object] / #[SimpleObject])
//! are generated at the module level where the GraphQL library is available.
//! This keeps `backbone-core` dependency-free for GraphQL concerns.

use async_trait::async_trait;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;

use crate::service::ServiceResult;

// ─── Service contract ─────────────────────────────────────────────────────────

/// The minimal contract `GenericGraphQLResolver` needs from a service.
///
/// Identical contract to `GrpcCapableService` — implementing once satisfies both.
#[async_trait]
pub trait GraphQLCapableService<E, C, U>: Send + Sync + 'static
where
    E: Clone + Send + Sync + 'static,
    C: Send + Sync + 'static,
    U: Send + Sync + 'static,
{
    async fn list(&self, page: u32, limit: u32, filters: HashMap<String, String>) -> ServiceResult<(Vec<E>, u64)>;
    async fn create(&self, dto: C) -> ServiceResult<E>;
    async fn get_by_id(&self, id: &str) -> ServiceResult<Option<E>>;
    async fn update(&self, id: &str, dto: U) -> ServiceResult<Option<E>>;
    async fn soft_delete(&self, id: &str) -> ServiceResult<bool>;
    async fn restore(&self, id: &str) -> ServiceResult<Option<E>>;
    async fn list_deleted(&self, page: u32, limit: u32) -> ServiceResult<(Vec<E>, u64)>;
    async fn empty_trash(&self) -> ServiceResult<u64>;
}

// ─── GenericGraphQLResolver ───────────────────────────────────────────────────

/// Generic GraphQL resolver.
///
/// This struct holds the service and provides named methods that match
/// GraphQL field names (`query_list`, `mutation_create`, etc.).
/// The actual `#[Object]` / `#[Subscription]` annotations are placed in
/// generated wrapper code that calls these methods — keeping the generic
/// base free from `async-graphql` attributes.
///
/// # Extension pattern
///
/// ```rust,ignore
/// // Generated type alias (1 line):
/// pub type OrderGqlResolver = GenericGraphQLResolver<Order, CreateOrderDto, UpdateOrderDto, OrderService>;
///
/// // Custom decorator adds entity-specific fields:
/// pub struct OrderGqlResolverCustom {
///     base: Arc<OrderGqlResolver>,
///     pricing: Arc<PricingService>,
/// }
/// #[Object]
/// impl OrderGqlResolverCustom {
///     // Delegate standard fields to base
///     async fn order(&self, id: ID) -> Option<Order> {
///         self.base.query_get_by_id(&id.to_string()).await.ok().flatten()
///     }
///     // Add entity-specific field
///     async fn order_price(&self, id: ID) -> f64 {
///         self.pricing.calculate(&id.to_string()).await
///     }
/// }
/// ```
pub struct GenericGraphQLResolver<E, C, U, S>
where
    E: Clone + Send + Sync + 'static,
    C: Send + Sync + 'static,
    U: Send + Sync + 'static,
    S: GraphQLCapableService<E, C, U>,
{
    service: Arc<S>,
    _phantom: PhantomData<(E, C, U)>,
}

impl<E, C, U, S> GenericGraphQLResolver<E, C, U, S>
where
    E: Clone + Send + Sync + 'static,
    C: Send + Sync + 'static,
    U: Send + Sync + 'static,
    S: GraphQLCapableService<E, C, U>,
{
    pub fn new(service: Arc<S>) -> Self {
        Self {
            service,
            _phantom: PhantomData,
        }
    }

    pub fn service(&self) -> &Arc<S> {
        &self.service
    }

    // ── Query operations ──────────────────────────────────────────────────

    /// GraphQL query: `entity(id: ID!): EntityType`
    pub async fn query_get_by_id(&self, id: &str) -> ServiceResult<Option<E>> {
        self.service.get_by_id(id).await
    }

    /// GraphQL query: `entities(page: Int, limit: Int, filters: JSON): EntityListResult`
    pub async fn query_list(
        &self,
        page: u32,
        limit: u32,
        filters: HashMap<String, String>,
    ) -> ServiceResult<GraphQLListResult<E>> {
        let (items, total) = self.service.list(page, limit, filters).await?;
        Ok(GraphQLListResult { items, total, page, limit })
    }

    /// GraphQL query: `deletedEntities(page: Int, limit: Int): EntityListResult`
    pub async fn query_list_deleted(
        &self,
        page: u32,
        limit: u32,
    ) -> ServiceResult<GraphQLListResult<E>> {
        let (items, total) = self.service.list_deleted(page, limit).await?;
        Ok(GraphQLListResult { items, total, page, limit })
    }

    // ── Mutation operations ───────────────────────────────────────────────

    /// GraphQL mutation: `createEntity(input: CreateInput!): EntityType!`
    pub async fn mutation_create(&self, dto: C) -> ServiceResult<E> {
        self.service.create(dto).await
    }

    /// GraphQL mutation: `updateEntity(id: ID!, input: UpdateInput!): EntityType`
    pub async fn mutation_update(&self, id: &str, dto: U) -> ServiceResult<Option<E>> {
        self.service.update(id, dto).await
    }

    /// GraphQL mutation: `deleteEntity(id: ID!): Boolean!`
    pub async fn mutation_delete(&self, id: &str) -> ServiceResult<bool> {
        self.service.soft_delete(id).await
    }

    /// GraphQL mutation: `restoreEntity(id: ID!): EntityType`
    pub async fn mutation_restore(&self, id: &str) -> ServiceResult<Option<E>> {
        self.service.restore(id).await
    }

    /// GraphQL mutation: `emptyEntityTrash: Int!`
    pub async fn mutation_empty_trash(&self) -> ServiceResult<u64> {
        self.service.empty_trash().await
    }
}

// Make resolver cloneable for sharing across query/mutation roots.
impl<E, C, U, S> Clone for GenericGraphQLResolver<E, C, U, S>
where
    E: Clone + Send + Sync + 'static,
    C: Send + Sync + 'static,
    U: Send + Sync + 'static,
    S: GraphQLCapableService<E, C, U>,
{
    fn clone(&self) -> Self {
        Self {
            service: self.service.clone(),
            _phantom: PhantomData,
        }
    }
}

// ─── Response types ───────────────────────────────────────────────────────────

/// Paginated list result for GraphQL responses.
///
/// Maps 1:1 to `PaginatedApiResponse` from the HTTP layer — same fields, no HTTP
/// coupling. Module code can annotate this with `#[SimpleObject]` if needed.
#[derive(Debug, Clone)]
pub struct GraphQLListResult<E> {
    pub items: Vec<E>,
    pub total: u64,
    pub page: u32,
    pub limit: u32,
}

impl<E> GraphQLListResult<E> {
    pub fn total_pages(&self) -> u32 {
        if self.limit == 0 {
            return 0;
        }
        ((self.total as f64) / (self.limit as f64)).ceil() as u32
    }
}

// ─── GraphQL filter helpers ───────────────────────────────────────────────────

/// Standard pagination input shared by all generated GraphQL list queries.
#[derive(Debug, Clone)]
pub struct GraphQLPaginationInput {
    pub page: Option<u32>,
    pub limit: Option<u32>,
}

impl GraphQLPaginationInput {
    pub fn page(&self) -> u32 {
        self.page.unwrap_or(1)
    }

    pub fn limit(&self) -> u32 {
        self.limit.unwrap_or(20)
    }
}

impl Default for GraphQLPaginationInput {
    fn default() -> Self {
        Self {
            page: Some(1),
            limit: Some(20),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Debug, Clone)]
    struct Item {
        id: String,
        name: String,
    }

    struct CreateItemInput { name: String }
    struct UpdateItemInput { name: String }

    struct FakeItemService {
        store: Mutex<Vec<Item>>,
    }

    impl FakeItemService {
        fn new() -> Self {
            Self {
                store: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl GraphQLCapableService<Item, CreateItemInput, UpdateItemInput> for FakeItemService {
        async fn list(&self, _p: u32, _l: u32, _f: HashMap<String, String>) -> ServiceResult<(Vec<Item>, u64)> {
            let store = self.store.lock().unwrap();
            let items: Vec<_> = store.iter().filter(|i| !i.id.starts_with("del-")).cloned().collect();
            let total = items.len() as u64;
            Ok((items, total))
        }
        async fn create(&self, dto: CreateItemInput) -> ServiceResult<Item> {
            let item = Item { id: uuid::Uuid::new_v4().to_string(), name: dto.name };
            self.store.lock().unwrap().push(item.clone());
            Ok(item)
        }
        async fn get_by_id(&self, id: &str) -> ServiceResult<Option<Item>> {
            Ok(self.store.lock().unwrap().iter().find(|i| i.id == id).cloned())
        }
        async fn update(&self, id: &str, dto: UpdateItemInput) -> ServiceResult<Option<Item>> {
            let mut store = self.store.lock().unwrap();
            if let Some(item) = store.iter_mut().find(|i| i.id == id) {
                item.name = dto.name;
                return Ok(Some(item.clone()));
            }
            Ok(None)
        }
        async fn soft_delete(&self, id: &str) -> ServiceResult<bool> {
            let mut store = self.store.lock().unwrap();
            if let Some(item) = store.iter_mut().find(|i| i.id == id) {
                item.id = format!("del-{}", item.id);
                return Ok(true);
            }
            Ok(false)
        }
        async fn restore(&self, _id: &str) -> ServiceResult<Option<Item>> { Ok(None) }
        async fn list_deleted(&self, _p: u32, _l: u32) -> ServiceResult<(Vec<Item>, u64)> { Ok((vec![], 0)) }
        async fn empty_trash(&self) -> ServiceResult<u64> { Ok(0) }
    }

    #[tokio::test]
    async fn create_and_query_roundtrip() {
        let service = Arc::new(FakeItemService::new());
        let resolver = GenericGraphQLResolver::new(service);

        resolver.mutation_create(CreateItemInput { name: "widget".into() }).await.unwrap();

        let result = resolver.query_list(1, 20, Default::default()).await.unwrap();
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].name, "widget");
        assert_eq!(result.total, 1);
    }

    #[tokio::test]
    async fn list_result_total_pages() {
        let result: GraphQLListResult<i32> = GraphQLListResult {
            items: vec![],
            total: 50,
            page: 1,
            limit: 20,
        };
        assert_eq!(result.total_pages(), 3);
    }
}
