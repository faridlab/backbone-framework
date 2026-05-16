//! HTTP layer and REST endpoint implementations
//!
//! This module provides generic HTTP/REST components for the Backbone CRUD system:
//! - `CrudService` trait: Core async trait for entity CRUD operations
//! - `BackboneCrudHandler`: Generic Axum router builder for all 11 endpoints
//! - Response types: `ApiResponse`, `PaginatedResponse`, `BulkResponse`

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

// ============================================================
// Response Types
// ============================================================

/// Standard API response wrapper
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T> ApiResponse<T> {
    /// Create a success response with data and an optional message
    pub fn success(data: T, message: Option<String>) -> Self {
        Self {
            success: true,
            data: Some(data),
            message,
            error: None,
        }
    }

    /// Create a success response without a message (convenience method)
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: None,
            error: None,
        }
    }

    pub fn success_with_message(data: T, message: impl Into<String>) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: Some(message.into()),
            error: None,
        }
    }

    pub fn error(error: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            message: None,
            error: Some(error.into()),
        }
    }

    pub fn not_found(entity: &str, id: &str) -> Self {
        Self {
            success: false,
            data: None,
            message: None,
            error: Some(format!("{} with id '{}' not found", entity, id)),
        }
    }
}

/// Generic list query parameters
#[derive(Debug, Deserialize, Default, Clone)]
pub struct ListQueryParams {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default)]
    pub sort_by: Option<String>,
    #[serde(default)]
    pub sort_order: Option<String>,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(flatten)]
    pub filters: HashMap<String, String>,
}

fn default_page() -> u32 { 1 }
fn default_limit() -> u32 { 20 }

/// Pagination response metadata
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PaginationResponse {
    pub total: u64,
    pub page: u32,
    pub limit: u32,
    pub total_pages: u32,
}

impl PaginationResponse {
    pub fn new(total: u64, page: u32, limit: u32) -> Self {
        let total_pages = if limit == 0 { 0 } else { ((total as f64) / (limit as f64)).ceil() as u32 };
        Self { total, page, limit, total_pages }
    }
}

/// Generic paginated response (without success wrapper)
#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub meta: PaginationResponse,
}

/// Paginated API response with success flag, data array, and metadata at top level
/// This is the preferred response format for list endpoints
#[derive(Debug, Serialize)]
pub struct PaginatedApiResponse<T> {
    pub success: bool,
    pub data: Vec<T>,
    pub meta: PaginationResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T> PaginatedApiResponse<T> {
    /// Create a successful paginated response
    pub fn ok(data: Vec<T>, total: u64, page: u32, limit: u32) -> Self {
        Self {
            success: true,
            data,
            meta: PaginationResponse::new(total, page, limit),
            error: None,
        }
    }

    /// Create from a PaginatedResponse
    pub fn from_paginated(resp: PaginatedResponse<T>) -> Self {
        Self {
            success: true,
            data: resp.data,
            meta: resp.meta,
            error: None,
        }
    }

    /// Create an error response
    pub fn error(error: impl Into<String>) -> Self {
        Self {
            success: false,
            data: Vec::new(),
            meta: PaginationResponse::new(0, 0, 0),
            error: Some(error.into()),
        }
    }
}

/// Bulk request for multiple entities
#[derive(Debug, Serialize, Deserialize)]
pub struct BulkCreateRequest<T> {
    pub items: Vec<T>,
}

/// Bulk response
#[derive(Debug, Serialize, Deserialize)]
pub struct BulkResponse<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}

/// Upsert request (update or insert)
#[derive(Debug, Serialize, Deserialize)]
pub struct UpsertRequest<T> {
    pub entity: T,
    pub create_if_not_exists: bool,
}

/// Filtering and sorting options
#[derive(Debug, Serialize, Deserialize)]
pub struct FilterOptions {
    pub filters: HashMap<String, String>,
    pub sort_by: Option<String>,
    pub sort_order: Option<SortOrder>,
}

/// Sort order enum
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    #[default]
    Asc,
    Desc,
}

/// Extended pagination request with filtering and sorting
#[derive(Debug, Serialize, Deserialize)]
pub struct ListRequest {
    pub page: Option<u32>,
    pub limit: Option<u32>,
    pub sort_by: Option<String>,
    pub sort_order: Option<SortOrder>,
    pub filters: Option<HashMap<String, String>>,
}

impl Default for ListRequest {
    fn default() -> Self {
        Self {
            page: Some(1),
            limit: Some(20),
            sort_by: None,
            sort_order: None,
            filters: None,
        }
    }
}

// ============================================================
// CrudService Trait - Core Generic CRUD Interface
// ============================================================

/// Core trait that application services must implement for generic CRUD operations.
///
/// This trait provides the contract for all 11 standard Backbone endpoints.
/// Module-specific services implement this trait with their entity types.
///
/// # Type Parameters
/// - `Entity`: The domain entity type
/// - `CreateDto`: DTO for create operations
/// - `UpdateDto`: DTO for update operations
///
/// # Example
/// ```ignore
/// #[async_trait]
/// impl CrudService<User, CreateUserDto, UpdateUserDto> for UserCrudService {
///     type Error = ApplicationError;
///     fn entity_name() -> &'static str { "User" }
///     // ... implement all methods
/// }
/// ```
#[async_trait::async_trait]
pub trait CrudService<Entity, CreateDto, UpdateDto>: Send + Sync
where
    Entity: Send + Sync,
    CreateDto: Send + Sync,
    UpdateDto: Send + Sync,
{
    /// Error type for service operations
    type Error: std::error::Error + Send + Sync;

    /// Entity name for error messages (e.g., "User", "Role")
    fn entity_name() -> &'static str;

    /// 1. List entities with pagination and filters
    async fn list(&self, page: u32, limit: u32, filters: HashMap<String, String>) -> Result<(Vec<Entity>, u64), Self::Error>;

    /// 2. Create a new entity
    async fn create(&self, dto: CreateDto) -> Result<Entity, Self::Error>;

    /// 3. Get entity by ID
    async fn get_by_id(&self, id: &str) -> Result<Option<Entity>, Self::Error>;

    /// 4. Full update of entity
    async fn update(&self, id: &str, dto: UpdateDto) -> Result<Option<Entity>, Self::Error>;

    /// 5. Partial update with specific fields
    async fn partial_update(&self, id: &str, fields: HashMap<String, serde_json::Value>) -> Result<Option<Entity>, Self::Error>;

    /// 6. Soft delete entity
    async fn soft_delete(&self, id: &str) -> Result<bool, Self::Error>;

    /// 7. Bulk create multiple entities
    async fn bulk_create(&self, items: Vec<CreateDto>) -> Result<Vec<Entity>, Self::Error>;

    /// 8. Upsert (create or update)
    async fn upsert(&self, dto: CreateDto) -> Result<Entity, Self::Error>;

    /// 9. List deleted entities (trash)
    async fn list_deleted(&self, page: u32, limit: u32) -> Result<(Vec<Entity>, u64), Self::Error>;

    /// 10. Restore soft-deleted entity
    async fn restore(&self, id: &str) -> Result<Option<Entity>, Self::Error>;

    /// 11. Permanently delete all soft-deleted entities
    async fn empty_trash(&self) -> Result<u64, Self::Error>;

    /// 12. Get deleted entity by ID (from trash)
    async fn get_deleted_by_id(&self, id: &str) -> Result<Option<Entity>, Self::Error>;

    /// 13. Permanently delete a single soft-deleted entity by ID
    async fn permanent_delete(&self, id: &str) -> Result<bool, Self::Error>;

    /// 14. List deleted entities with filters
    async fn list_deleted_filtered(&self, page: u32, limit: u32, filters: HashMap<String, String>) -> Result<(Vec<Entity>, u64), Self::Error>;

    /// 15. Count active (non-deleted) entities
    async fn count_active(&self) -> Result<u64, Self::Error>;

    /// 16. Count deleted entities in trash
    async fn count_deleted(&self) -> Result<u64, Self::Error>;
}

// ============================================================
// BackboneCrudHandler - Generic Axum Router Builder
// ============================================================

/// Generic Backbone CRUD handler that provides all 11 endpoints as an Axum router.
///
/// This handler wraps a `CrudService` implementation and generates all standard
/// Backbone endpoints automatically.
///
/// # Type Parameters
/// - `S`: Service implementing `CrudService`
/// - `E`: Entity type
/// - `C`: Create DTO type
/// - `U`: Update DTO type
/// - `R`: Response DTO type (must implement `From<E>`)
///
/// # Example
/// ```ignore
/// let user_crud = UserCrudService::new(user_service);
/// let routes = BackboneCrudHandler::<UserCrudService, UserAggregate, CreateUserDto, UpdateUserDto, UserResponseDto>
///     ::routes(Arc::new(user_crud), "/api/v1/users");
/// ```
pub struct BackboneCrudHandler<S, E, C, U, R>
where
    S: CrudService<E, C, U> + 'static,
    E: Serialize + Send + Sync + 'static,
    C: DeserializeOwned + Send + Sync + 'static,
    U: DeserializeOwned + Send + Sync + 'static,
    R: From<E> + Serialize + Send + Sync + 'static,
{
    service: Arc<S>,
    _phantom: std::marker::PhantomData<(E, C, U, R)>,
}

impl<S, E, C, U, R> BackboneCrudHandler<S, E, C, U, R>
where
    S: CrudService<E, C, U> + 'static,
    E: Serialize + Send + Sync + Clone + 'static,
    C: DeserializeOwned + Send + Sync + 'static,
    U: DeserializeOwned + Send + Sync + 'static,
    R: From<E> + Serialize + Send + Sync + 'static,
{
    pub fn new(service: Arc<S>) -> Self {
        Self {
            service,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Create Axum router with all 16 Backbone endpoints (reads + writes).
    ///
    /// Equivalent to `read_routes(...).merge(write_routes(...))`. Use the
    /// split variants when reads and writes need different middleware
    /// (e.g., public reads + authenticated writes).
    ///
    /// # Routes Created
    /// 1. `GET {base_path}` - List with pagination
    /// 2. `POST {base_path}` - Create
    /// 3. `GET {base_path}/:id` - Get by ID
    /// 4. `PUT {base_path}/:id` - Full update
    /// 5. `PATCH {base_path}/:id` - Partial update
    /// 6. `DELETE {base_path}/:id` - Soft delete
    /// 7. `POST {base_path}/bulk` - Bulk create
    /// 8. `POST {base_path}/upsert` - Upsert
    /// 9. `GET {base_path}/trash` - List deleted (with filters)
    /// 10. `POST {base_path}/:id/restore` - Restore
    /// 11. `DELETE {base_path}/empty` - Empty trash
    /// 12. `GET {base_path}/:id/deleted` - Get deleted by ID
    /// 13. `DELETE {base_path}/trash/:id` - Permanent delete from trash
    /// 14. (Uses route 9 with filters)
    /// 15. `GET {base_path}/count` - Count active entities
    /// 16. `GET {base_path}/trash/count` - Count deleted entities
    pub fn routes(service: Arc<S>, base_path: &str) -> axum::Router<()>
    where
        S: Clone,
    {
        Self::read_routes(service.clone(), base_path)
            .merge(Self::write_routes(service, base_path))
    }

    /// Create Axum router with only the read (GET) endpoints.
    ///
    /// Safe to expose publicly (e.g., for reference data like countries or
    /// categories) without auth middleware. Pair with `write_routes` under
    /// an auth layer when mutations must be restricted.
    ///
    /// # Routes Created
    /// - `GET {base_path}` - List with pagination
    /// - `GET {base_path}/:id` - Get by ID
    /// - `GET {base_path}/trash` - List deleted (with filters)
    /// - `GET {base_path}/:id/deleted` - Get deleted by ID
    /// - `GET {base_path}/count` - Count active entities
    /// - `GET {base_path}/trash/count` - Count deleted entities
    pub fn read_routes(service: Arc<S>, base_path: &str) -> axum::Router<()>
    where
        S: Clone,
    {
        use axum::{
            extract::{Path, Query},
            routing::get,
            Router,
        };

        let handler = Arc::new(Self::new(service));

        Router::new()
            // GET /collection - List
            .route(base_path, get({
                let h = handler.clone();
                move |query: Query<ListQueryParams>| async move {
                    Self::list_handler(h, query).await
                }
            }))
            // GET /collection/trash - List deleted
            .route(&format!("{}/trash", base_path), get({
                let h = handler.clone();
                move |query: Query<ListQueryParams>| async move {
                    Self::list_deleted_handler(h, query).await
                }
            }))
            // GET /collection/:id - Get by ID
            .route(&format!("{}/:id", base_path), get({
                let h = handler.clone();
                move |path: Path<String>| async move {
                    Self::get_handler(h, path).await
                }
            }))
            // GET /collection/:id/deleted - Get deleted by ID
            .route(&format!("{}/:id/deleted", base_path), get({
                let h = handler.clone();
                move |path: Path<String>| async move {
                    Self::get_deleted_handler(h, path).await
                }
            }))
            // GET /collection/count - Count active entities
            .route(&format!("{}/count", base_path), get({
                let h = handler.clone();
                move || async move {
                    Self::count_active_handler(h).await
                }
            }))
            // GET /collection/trash/count - Count deleted entities
            .route(&format!("{}/trash/count", base_path), get({
                let h = handler.clone();
                move || async move {
                    Self::count_deleted_handler(h).await
                }
            }))
    }

    /// Create Axum router with only the write (mutation) endpoints.
    ///
    /// These routes should not be publicly exposed. Wrap them with an auth
    /// middleware (see `BackboneCrudHandler` module docs) before nesting
    /// into the application router.
    ///
    /// # Routes Created
    /// - `POST {base_path}` - Create
    /// - `POST {base_path}/bulk` - Bulk create
    /// - `POST {base_path}/upsert` - Upsert
    /// - `DELETE {base_path}/empty` - Empty trash
    /// - `DELETE {base_path}/trash/:id` - Permanent delete from trash
    /// - `PUT {base_path}/:id` - Full update
    /// - `PATCH {base_path}/:id` - Partial update
    /// - `DELETE {base_path}/:id` - Soft delete
    /// - `POST {base_path}/:id/restore` - Restore
    pub fn write_routes(service: Arc<S>, base_path: &str) -> axum::Router<()>
    where
        S: Clone,
    {
        use axum::{
            extract::Path,
            routing::{delete, patch, post, put},
            Json, Router,
        };

        let handler = Arc::new(Self::new(service));

        Router::new()
            // POST /collection - Create
            .route(base_path, post({
                let h = handler.clone();
                move |body: Json<C>| async move {
                    Self::create_handler(h, body).await
                }
            }))
            // POST /collection/bulk - Bulk create
            .route(&format!("{}/bulk", base_path), post({
                let h = handler.clone();
                move |body: Json<Vec<C>>| async move {
                    Self::bulk_create_handler(h, body).await
                }
            }))
            // POST /collection/upsert - Upsert
            .route(&format!("{}/upsert", base_path), post({
                let h = handler.clone();
                move |body: Json<C>| async move {
                    Self::upsert_handler(h, body).await
                }
            }))
            // DELETE /collection/empty - Empty trash
            .route(&format!("{}/empty", base_path), delete({
                let h = handler.clone();
                move || async move {
                    Self::empty_trash_handler(h).await
                }
            }))
            // DELETE /collection/trash/:id - Permanent delete from trash
            .route(&format!("{}/trash/:id", base_path), delete({
                let h = handler.clone();
                move |path: Path<String>| async move {
                    Self::permanent_delete_handler(h, path).await
                }
            }))
            // PUT /collection/:id - Full update
            .route(&format!("{}/:id", base_path), put({
                let h = handler.clone();
                move |path: Path<String>, body: Json<U>| async move {
                    Self::update_handler(h, path, body).await
                }
            }))
            // PATCH /collection/:id - Partial update
            .route(&format!("{}/:id", base_path), patch({
                let h = handler.clone();
                move |path: Path<String>, body: Json<HashMap<String, serde_json::Value>>| async move {
                    Self::partial_update_handler(h, path, body).await
                }
            }))
            // DELETE /collection/:id - Soft delete
            .route(&format!("{}/:id", base_path), delete({
                let h = handler.clone();
                move |path: Path<String>| async move {
                    Self::delete_handler(h, path).await
                }
            }))
            // POST /collection/:id/restore - Restore
            .route(&format!("{}/:id/restore", base_path), post({
                let h = handler.clone();
                move |path: Path<String>| async move {
                    Self::restore_handler(h, path).await
                }
            }))
    }

    // ============================================================
    // Handler Implementations
    // ============================================================

    async fn list_handler(
        handler: Arc<Self>,
        axum::extract::Query(params): axum::extract::Query<ListQueryParams>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        // Merge search and status into filters HashMap
        let mut filters = params.filters.clone();
        if let Some(search) = params.search {
            filters.insert("search".to_string(), search);
        }
        if let Some(status) = params.status {
            filters.insert("status".to_string(), status);
        }

        match handler.service.list(params.page, params.limit, filters).await {
            Ok((entities, total)) => {
                let items: Vec<R> = entities.into_iter().map(R::from).collect();
                let response = PaginatedApiResponse::ok(items, total, params.page, params.limit);
                (StatusCode::OK, Json(response))
            }
            Err(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(PaginatedApiResponse::<R>::error(e.to_string())))
            }
        }
    }

    async fn create_handler(
        handler: Arc<Self>,
        axum::Json(dto): axum::Json<C>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        match handler.service.create(dto).await {
            Ok(entity) => {
                let response: R = entity.into();
                (StatusCode::CREATED, Json(ApiResponse::ok(response)))
            }
            Err(e) => {
                let error_str = e.to_string();
                if error_str.contains("conflict") || error_str.contains("already exists") {
                    (StatusCode::CONFLICT, Json(ApiResponse::<R>::error(error_str)))
                } else {
                    (StatusCode::BAD_REQUEST, Json(ApiResponse::<R>::error(error_str)))
                }
            }
        }
    }

    async fn get_handler(
        handler: Arc<Self>,
        axum::extract::Path(id): axum::extract::Path<String>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        match handler.service.get_by_id(&id).await {
            Ok(Some(entity)) => {
                let response: R = entity.into();
                (StatusCode::OK, Json(ApiResponse::ok(response)))
            }
            Ok(None) => {
                (StatusCode::NOT_FOUND, Json(ApiResponse::<R>::not_found(S::entity_name(), &id)))
            }
            Err(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::<R>::error(e.to_string())))
            }
        }
    }

    async fn update_handler(
        handler: Arc<Self>,
        axum::extract::Path(id): axum::extract::Path<String>,
        axum::Json(dto): axum::Json<U>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        match handler.service.update(&id, dto).await {
            Ok(Some(entity)) => {
                let response: R = entity.into();
                (StatusCode::OK, Json(ApiResponse::ok(response)))
            }
            Ok(None) => {
                (StatusCode::NOT_FOUND, Json(ApiResponse::<R>::not_found(S::entity_name(), &id)))
            }
            Err(e) => {
                (StatusCode::BAD_REQUEST, Json(ApiResponse::<R>::error(e.to_string())))
            }
        }
    }

    async fn partial_update_handler(
        handler: Arc<Self>,
        axum::extract::Path(id): axum::extract::Path<String>,
        axum::Json(fields): axum::Json<HashMap<String, serde_json::Value>>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        // Normalize incoming JSON keys to snake_case before forwarding to the
        // service-layer merge. Entities serialize with Rust's snake_case field
        // names by default, so a client sending camelCase (e.g. `isVip`) would
        // otherwise produce a merged JSON object with both `is_vip` (from the
        // existing entity) and `isVip` (from the patch). On deserialize back
        // to the entity, the unknown camelCase key is silently dropped and the
        // edit is lost. Normalization makes this class of bug impossible.
        //
        // The conversion is idempotent — already-snake_case keys pass through
        // unchanged — so clients that already conform see no behavior change.
        let fields: HashMap<String, serde_json::Value> = fields
            .into_iter()
            .map(|(k, v)| (camel_to_snake_case(&k), v))
            .collect();

        match handler.service.partial_update(&id, fields).await {
            Ok(Some(entity)) => {
                let response: R = entity.into();
                (StatusCode::OK, Json(ApiResponse::ok(response)))
            }
            Ok(None) => {
                (StatusCode::NOT_FOUND, Json(ApiResponse::<R>::not_found(S::entity_name(), &id)))
            }
            Err(e) => {
                (StatusCode::BAD_REQUEST, Json(ApiResponse::<R>::error(e.to_string())))
            }
        }
    }

    async fn delete_handler(
        handler: Arc<Self>,
        axum::extract::Path(id): axum::extract::Path<String>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        match handler.service.soft_delete(&id).await {
            Ok(true) => {
                // Return 200 OK with success response instead of 204 No Content
                // to ensure proper JSON response handling
                (StatusCode::OK, Json(ApiResponse::<()>::success_with_message((), "Entity deleted successfully")))
            }
            Ok(false) => {
                (StatusCode::NOT_FOUND, Json(ApiResponse::<()>::not_found(S::entity_name(), &id)))
            }
            Err(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::<()>::error(e.to_string())))
            }
        }
    }

    async fn bulk_create_handler(
        handler: Arc<Self>,
        axum::Json(items): axum::Json<Vec<C>>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        match handler.service.bulk_create(items).await {
            Ok(entities) => {
                let result_items: Vec<R> = entities.into_iter().map(R::from).collect();
                let total = result_items.len();
                let response = BulkResponse {
                    items: result_items,
                    total,
                    failed: 0,
                    errors: vec![],
                };
                (StatusCode::CREATED, Json(ApiResponse::ok(response)))
            }
            Err(e) => {
                (StatusCode::BAD_REQUEST, Json(ApiResponse::<BulkResponse<R>>::error(e.to_string())))
            }
        }
    }

    async fn upsert_handler(
        handler: Arc<Self>,
        axum::Json(dto): axum::Json<C>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        match handler.service.upsert(dto).await {
            Ok(entity) => {
                let response: R = entity.into();
                (StatusCode::OK, Json(ApiResponse::ok(response)))
            }
            Err(e) => {
                (StatusCode::BAD_REQUEST, Json(ApiResponse::<R>::error(e.to_string())))
            }
        }
    }

    async fn list_deleted_handler(
        handler: Arc<Self>,
        axum::extract::Query(params): axum::extract::Query<ListQueryParams>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        match handler.service.list_deleted(params.page, params.limit).await {
            Ok((entities, total)) => {
                let items: Vec<R> = entities.into_iter().map(R::from).collect();
                let response = PaginatedApiResponse::ok(items, total, params.page, params.limit);
                (StatusCode::OK, Json(response))
            }
            Err(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(PaginatedApiResponse::<R>::error(e.to_string())))
            }
        }
    }

    async fn restore_handler(
        handler: Arc<Self>,
        axum::extract::Path(id): axum::extract::Path<String>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        match handler.service.restore(&id).await {
            Ok(Some(entity)) => {
                let response: R = entity.into();
                (StatusCode::OK, Json(ApiResponse::success_with_message(response, "Entity restored successfully")))
            }
            Ok(None) => {
                (StatusCode::NOT_FOUND, Json(ApiResponse::<R>::not_found(S::entity_name(), &id)))
            }
            Err(e) => {
                (StatusCode::BAD_REQUEST, Json(ApiResponse::<R>::error(e.to_string())))
            }
        }
    }

    async fn empty_trash_handler(
        handler: Arc<Self>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        match handler.service.empty_trash().await {
            Ok(count) => {
                (StatusCode::OK, Json(ApiResponse::success_with_message(
                    serde_json::json!({ "deleted_count": count }),
                    format!("Successfully deleted {} items from trash", count)
                )))
            }
            Err(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::<serde_json::Value>::error(e.to_string())))
            }
        }
    }

    /// 12. GET /collection/:id/deleted - Get deleted entity by ID
    async fn get_deleted_handler(
        handler: Arc<Self>,
        axum::extract::Path(id): axum::extract::Path<String>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        match handler.service.get_deleted_by_id(&id).await {
            Ok(Some(entity)) => {
                let response: R = entity.into();
                (StatusCode::OK, Json(ApiResponse::ok(response)))
            }
            Ok(None) => {
                (StatusCode::NOT_FOUND, Json(ApiResponse::<R>::not_found(&format!("Deleted {}", S::entity_name()), &id)))
            }
            Err(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::<R>::error(e.to_string())))
            }
        }
    }

    /// 13. DELETE /collection/trash/:id - Permanently delete from trash
    async fn permanent_delete_handler(
        handler: Arc<Self>,
        axum::extract::Path(id): axum::extract::Path<String>,
    ) -> axum::response::Response {
        use axum::{http::StatusCode, Json, response::IntoResponse};

        // First check if the entity exists in trash
        match handler.service.get_deleted_by_id(&id).await {
            Ok(Some(_)) => {
                // Entity exists in trash, proceed with permanent delete
                match handler.service.permanent_delete(&id).await {
                    Ok(true) => {
                        // 204 No Content - successful deletion with no body
                        StatusCode::NO_CONTENT.into_response()
                    }
                    Ok(false) => {
                        (StatusCode::NOT_FOUND, Json(ApiResponse::<R>::error(
                            format!("Failed to permanently delete {}", S::entity_name())
                        ))).into_response()
                    }
                    Err(e) => {
                        (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::<R>::error(e.to_string()))).into_response()
                    }
                }
            }
            Ok(None) => {
                (StatusCode::NOT_FOUND, Json(ApiResponse::<R>::not_found(
                    &format!("{} in trash", S::entity_name()), &id
                ))).into_response()
            }
            Err(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::<R>::error(e.to_string()))).into_response()
            }
        }
    }

    /// 15. GET /collection/count - Count active entities
    async fn count_active_handler(
        handler: Arc<Self>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        match handler.service.count_active().await {
            Ok(count) => {
                (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({ "count": count }))))
            }
            Err(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::<serde_json::Value>::error(e.to_string())))
            }
        }
    }

    /// 16. GET /collection/trash/count - Count deleted entities
    async fn count_deleted_handler(
        handler: Arc<Self>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        match handler.service.count_deleted().await {
            Ok(count) => {
                (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({ "count": count }))))
            }
            Err(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::<serde_json::Value>::error(e.to_string())))
            }
        }
    }
}

// ============================================================
// BackboneHttpHandler Trait - Synchronous HTTP Service Interface
// ============================================================

/// Synchronous trait for HTTP handlers implementing Backbone CRUD operations.
///
/// This trait provides a synchronous interface for services that need to implement
/// the 11 standard Backbone CRUD endpoints. Unlike `CrudService` which is async,
/// this trait is designed for simpler use cases where async is not required.
///
/// # Type Parameters
/// - `T`: The entity type
///
/// # Example
/// ```ignore
/// impl BackboneHttpHandler<User> for UserService {
///     fn list(&self, request: ListRequest) -> Result<ApiResponse<Vec<User>>> {
///         let users = self.users.lock().unwrap();
///         let filtered = users.iter().filter(|u| !u.is_deleted()).cloned().collect();
///         Ok(ApiResponse::success(filtered, Some("Users listed".to_string())))
///     }
///     // ... implement other methods
/// }
/// ```
pub trait BackboneHttpHandler<T>: Send + Sync {
    /// 1. List entities with pagination and filtering
    fn list(&self, request: ListRequest) -> anyhow::Result<ApiResponse<Vec<T>>>;

    /// 2. Create a new entity
    fn create(&self, request: T) -> anyhow::Result<ApiResponse<T>>;

    /// 3. Get entity by ID
    fn get_by_id(&self, id: &uuid::Uuid) -> anyhow::Result<ApiResponse<T>>;

    /// 4. Full update of entity
    fn update(&self, id: &uuid::Uuid, request: T) -> anyhow::Result<ApiResponse<T>>;

    /// 5. Partial update with specific fields
    fn partial_update(&self, id: &uuid::Uuid, fields: HashMap<String, serde_json::Value>) -> anyhow::Result<ApiResponse<T>>;

    /// 6. Soft delete entity
    fn soft_delete(&self, id: &uuid::Uuid) -> anyhow::Result<ApiResponse<()>>;

    /// 7. Bulk create multiple entities
    fn bulk_create(&self, request: BulkCreateRequest<T>) -> anyhow::Result<ApiResponse<BulkResponse<T>>>;

    /// 8. Upsert (create or update)
    fn upsert(&self, request: UpsertRequest<T>) -> anyhow::Result<ApiResponse<T>>;

    /// 9. List deleted entities (trash)
    fn list_deleted(&self, request: ListRequest) -> anyhow::Result<ApiResponse<Vec<T>>>;

    /// 10. Restore soft-deleted entity
    fn restore(&self, id: &uuid::Uuid) -> anyhow::Result<ApiResponse<T>>;

    /// 11. Permanently delete all soft-deleted entities
    fn empty_trash(&self) -> anyhow::Result<ApiResponse<()>>;

    /// 12. Get deleted entity by ID (from trash)
    fn get_deleted_by_id(&self, id: &uuid::Uuid) -> anyhow::Result<ApiResponse<T>>;
}

// ============================================================
// Legacy Types (Backwards Compatibility)
// ============================================================

/// Pagination request (legacy)
#[derive(Debug, Serialize, Deserialize)]
pub struct PaginationRequest {
    pub page: u32,
    pub limit: u32,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
}

// ============================================================
// Internal Helpers
// ============================================================

/// Convert a single JSON key from `camelCase` / `PascalCase` to `snake_case`.
///
/// Idempotent on already-`snake_case` input: `is_vip` → `is_vip`.
/// Single words pass through unchanged: `name` → `name`.
/// Boundary detection inserts an underscore between a lowercase/digit and the
/// following uppercase, and between a run of uppercase and a following
/// uppercase-then-lowercase pair (so `IOError` → `io_error`, not `i_o_error`).
///
/// Used by `BackboneCrudHandler::partial_update_handler` to normalize PATCH
/// payload keys so the service-layer JSON merge aligns with snake_case entity
/// fields regardless of the client's serialization convention.
fn camel_to_snake_case(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    let mut result = String::with_capacity(key.len() + 2);
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() {
            let prev = if i > 0 { chars[i - 1] } else { '\0' };
            let next = chars.get(i + 1).copied().unwrap_or('\0');
            // Insert separator before this uppercase letter when crossing a
            // word boundary. Skip if the result is empty or already ends with
            // an underscore (e.g. `_Foo` should stay `_foo`, not `__foo`).
            let crosses_lower = prev.is_ascii_lowercase() || prev.is_ascii_digit();
            let crosses_acronym = prev.is_ascii_uppercase() && next.is_ascii_lowercase();
            if (crosses_lower || crosses_acronym)
                && !result.is_empty()
                && !result.ends_with('_')
            {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case_input_passes_through_unchanged() {
        assert_eq!(camel_to_snake_case("is_vip"), "is_vip");
        assert_eq!(camel_to_snake_case("flag_reason"), "flag_reason");
        assert_eq!(camel_to_snake_case("loyalty_tier_v2"), "loyalty_tier_v2");
    }

    #[test]
    fn single_word_unchanged() {
        assert_eq!(camel_to_snake_case("name"), "name");
        assert_eq!(camel_to_snake_case("id"), "id");
        assert_eq!(camel_to_snake_case(""), "");
    }

    #[test]
    fn camel_case_converts() {
        assert_eq!(camel_to_snake_case("isVip"), "is_vip");
        assert_eq!(camel_to_snake_case("userId"), "user_id");
        assert_eq!(camel_to_snake_case("customerSegment"), "customer_segment");
        assert_eq!(camel_to_snake_case("blacklistReason"), "blacklist_reason");
    }

    #[test]
    fn pascal_case_converts() {
        assert_eq!(camel_to_snake_case("IsVip"), "is_vip");
        assert_eq!(camel_to_snake_case("UserId"), "user_id");
    }

    #[test]
    fn acronym_runs_stay_together() {
        // The transition from an uppercase run to a lowercase letter starts a
        // new word at the last uppercase, not between every pair.
        assert_eq!(camel_to_snake_case("IOError"), "io_error");
        assert_eq!(camel_to_snake_case("httpURL"), "http_url");
        assert_eq!(camel_to_snake_case("ABC"), "abc");
        assert_eq!(camel_to_snake_case("XMLHttpRequest"), "xml_http_request");
    }

    #[test]
    fn digits_count_as_lowercase_for_boundary() {
        assert_eq!(camel_to_snake_case("v2Field"), "v2_field");
        assert_eq!(camel_to_snake_case("field2Name"), "field2_name");
    }

    #[test]
    fn underscores_not_doubled() {
        assert_eq!(camel_to_snake_case("_Foo"), "_foo");
        assert_eq!(camel_to_snake_case("foo_Bar"), "foo_bar");
    }
}