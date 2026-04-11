//! gRPC layer and service implementations

use anyhow::Result;
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;

use crate::service::{ServiceError, ServiceResult};

/// gRPC request/response wrapper
#[derive(Debug, Serialize, Deserialize)]
pub struct GrpcResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T> GrpcResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(error: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error),
        }
    }
}

/// gRPC list request with pagination
#[derive(Debug, Serialize, Deserialize)]
pub struct GrpcListRequest {
    pub page: u32,
    pub limit: u32,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
    pub filters: Option<HashMap<String, String>>,
}

/// gRPC list response with metadata
#[derive(Debug, Serialize, Deserialize)]
pub struct GrpcListResponse<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub page: u32,
    pub limit: u32,
    pub total_pages: u32,
}

/// gRPC bulk create request
#[derive(Debug, Serialize, Deserialize)]
pub struct GrpcBulkCreateRequest<T> {
    pub items: Vec<T>,
}

/// gRPC bulk response
#[derive(Debug, Serialize, Deserialize)]
pub struct GrpcBulkResponse<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}

/// gRPC upsert request
#[derive(Debug, Serialize, Deserialize)]
pub struct GrpcUpsertRequest<T> {
    pub entity: T,
    pub create_if_not_exists: bool,
}

/// gRPC partial update request
#[derive(Debug, Serialize, Deserialize)]
pub struct GrpcPartialUpdateRequest {
    pub id: Uuid,
    pub fields: HashMap<String, String>,
}

/// Complete gRPC service trait for all 11 Backbone endpoints
pub trait BackboneGrpcService<T> {
    /// 1. List (paginated, filtered, sorted)
    fn list(&self, request: GrpcListRequest) -> Result<GrpcResponse<GrpcListResponse<T>>, anyhow::Error>;

    /// 2. Create
    fn create(&self, request: T) -> Result<GrpcResponse<T>, anyhow::Error>;

    /// 3. Get by ID
    fn get_by_id(&self, request: Uuid) -> Result<GrpcResponse<T>, anyhow::Error>;

    /// 4. Full update
    fn update(&self, request: T) -> Result<GrpcResponse<T>, anyhow::Error>;

    /// 5. Partial update
    fn partial_update(&self, request: GrpcPartialUpdateRequest) -> Result<GrpcResponse<T>, anyhow::Error>;

    /// 6. Soft delete
    fn soft_delete(&self, request: Uuid) -> Result<GrpcResponse<()>, anyhow::Error>;

    /// 7. Bulk create
    fn bulk_create(&self, request: GrpcBulkCreateRequest<T>) -> Result<GrpcResponse<GrpcBulkResponse<T>>, anyhow::Error>;

    /// 8. Upsert
    fn upsert(&self, request: GrpcUpsertRequest<T>) -> Result<GrpcResponse<T>, anyhow::Error>;

    /// 9. List deleted
    fn list_deleted(&self, request: GrpcListRequest) -> Result<GrpcResponse<GrpcListResponse<T>>, anyhow::Error>;

    /// 10. Restore
    fn restore(&self, request: Uuid) -> Result<GrpcResponse<T>, anyhow::Error>;

    /// 11. Empty trash
    fn empty_trash(&self, request: ()) -> Result<GrpcResponse<()>, anyhow::Error>;
}

/// Legacy gRPC service trait for backwards compatibility
pub trait GrpcService<T> {
    fn create(&self, request: T) -> Result<T, anyhow::Error>;
    fn get(&self, request: T) -> Result<T, anyhow::Error>;
    fn update(&self, request: T) -> Result<T, anyhow::Error>;
    fn delete(&self, request: T) -> Result<T, anyhow::Error>;
    fn list(&self, request: T) -> Result<T, anyhow::Error>;
}

// ─── GenericGrpcService ───────────────────────────────────────────────────────

/// Service contract required by `GenericGrpcService`.
///
/// `GenericCrudService<E,C,U,R>` satisfies this contract.
/// Any custom service wrapper that exposes the same 8 methods will work too.
#[async_trait]
pub trait GrpcCapableService<E, C, U>: Send + Sync + 'static
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

/// Generic gRPC service — mirrors `BackboneCrudHandler` for the gRPC transport.
///
/// Generated code emits a type alias:
///
/// ```rust,ignore
/// // Generated (was ~300 lines of proto dispatch boilerplate):
/// pub type OrderGrpcService = GenericGrpcService<
///     Order, CreateOrderDto, UpdateOrderDto, OrderService
/// >;
/// ```
///
/// Custom extensions go in the `// <<< CUSTOM` decorator wrapper.
pub struct GenericGrpcService<E, C, U, S>
where
    E: Clone + Send + Sync + 'static,
    C: Send + Sync + 'static,
    U: Send + Sync + 'static,
    S: GrpcCapableService<E, C, U>,
{
    service: Arc<S>,
    _phantom: PhantomData<(E, C, U)>,
}

impl<E, C, U, S> GenericGrpcService<E, C, U, S>
where
    E: Clone + Send + Sync + 'static,
    C: Send + Sync + 'static,
    U: Send + Sync + 'static,
    S: GrpcCapableService<E, C, U>,
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

    // ── Delegate methods — each maps directly to one gRPC RPC ────────────

    pub async fn rpc_list(
        &self,
        page: u32,
        limit: u32,
        filters: HashMap<String, String>,
    ) -> GrpcResponse<GrpcListResponse<E>>
    where
        E: Serialize + for<'de> serde::Deserialize<'de>,
    {
        match self.service.list(page, limit, filters).await {
            Ok((items, total)) => {
                let total_pages = if limit == 0 { 0 } else { ((total as f64) / (limit as f64)).ceil() as u32 };
                GrpcResponse::success(GrpcListResponse {
                    items,
                    total,
                    page,
                    limit,
                    total_pages,
                })
            }
            Err(e) => GrpcResponse::error(e.to_string()),
        }
    }

    pub async fn rpc_create(&self, dto: C) -> GrpcResponse<E>
    where
        E: Serialize + for<'de> serde::Deserialize<'de>,
    {
        match self.service.create(dto).await {
            Ok(entity) => GrpcResponse::success(entity),
            Err(e) => GrpcResponse::error(e.to_string()),
        }
    }

    pub async fn rpc_get_by_id(&self, id: &str) -> GrpcResponse<Option<E>>
    where
        E: Serialize + for<'de> serde::Deserialize<'de>,
    {
        match self.service.get_by_id(id).await {
            Ok(entity) => GrpcResponse::success(entity),
            Err(e) => GrpcResponse::error(e.to_string()),
        }
    }

    pub async fn rpc_update(&self, id: &str, dto: U) -> GrpcResponse<Option<E>>
    where
        E: Serialize + for<'de> serde::Deserialize<'de>,
    {
        match self.service.update(id, dto).await {
            Ok(entity) => GrpcResponse::success(entity),
            Err(e) => GrpcResponse::error(e.to_string()),
        }
    }

    pub async fn rpc_soft_delete(&self, id: &str) -> GrpcResponse<bool> {
        match self.service.soft_delete(id).await {
            Ok(deleted) => GrpcResponse::success(deleted),
            Err(e) => GrpcResponse::error(e.to_string()),
        }
    }

    pub async fn rpc_restore(&self, id: &str) -> GrpcResponse<Option<E>>
    where
        E: Serialize + for<'de> serde::Deserialize<'de>,
    {
        match self.service.restore(id).await {
            Ok(entity) => GrpcResponse::success(entity),
            Err(e) => GrpcResponse::error(e.to_string()),
        }
    }

    pub async fn rpc_list_deleted(
        &self,
        page: u32,
        limit: u32,
    ) -> GrpcResponse<GrpcListResponse<E>>
    where
        E: Serialize + for<'de> serde::Deserialize<'de>,
    {
        match self.service.list_deleted(page, limit).await {
            Ok((items, total)) => {
                let total_pages = if limit == 0 { 0 } else { ((total as f64) / (limit as f64)).ceil() as u32 };
                GrpcResponse::success(GrpcListResponse {
                    items,
                    total,
                    page,
                    limit,
                    total_pages,
                })
            }
            Err(e) => GrpcResponse::error(e.to_string()),
        }
    }

    pub async fn rpc_empty_trash(&self) -> GrpcResponse<u64> {
        match self.service.empty_trash().await {
            Ok(count) => GrpcResponse::success(count),
            Err(e) => GrpcResponse::error(e.to_string()),
        }
    }
}

/// gRPC service configuration
#[derive(Debug, Clone)]
pub struct GrpcConfig {
    pub host: String,
    pub port: u16,
    pub max_message_size: usize,
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 50051,
            max_message_size: 4 * 1024 * 1024, // 4MB
        }
    }
}