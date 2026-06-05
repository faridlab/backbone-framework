//! HTTP layer and REST endpoint implementations
//!
//! This module provides generic HTTP/REST components for the Backbone CRUD system:
//! - `CrudService` trait: Core async trait for entity CRUD operations
//! - `BackboneCrudHandler`: Generic Axum router builder for all 11 endpoints
//! - Response types: `ApiResponse`, `PaginatedResponse`, `BulkResponse`

use crate::extractors::JsonOrForm;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

// ============================================================
// Response Types
// ============================================================

/// Standard API response wrapper
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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

/// Reserved response-shaping query keys (carved out of the filter grammar).
const RESERVED_QUERY_KEYS: [&str; 3] = ["fields", "include", "with"];

/// Parse the reserved `fields` key (comma-separated) into trimmed field names.
/// Absent/empty → empty vec, meaning "no projection — return every field".
fn sparse_fields(query: &HashMap<String, String>) -> Vec<String> {
    query
        .get("fields")
        .map(|s| {
            s.split(',')
                .map(str::trim)
                .filter(|f| !f.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Serialize a response DTO to a JSON value (for sparse-field projection).
/// Serialization is infallible for the generated plain-serde DTOs; fall back to null.
fn to_response_value<R: Serialize>(r: R) -> serde_json::Value {
    serde_json::to_value(r).unwrap_or(serde_json::Value::Null)
}

/// Apply a sparse fieldset to an already-serialized response object: keep only the
/// requested top-level keys plus the always-on core (`id`). Non-objects and an empty
/// request are returned unchanged; unknown requested keys are ignored.
fn project_sparse(mut value: serde_json::Value, fields: &[String]) -> serde_json::Value {
    if fields.is_empty() {
        return value;
    }
    if let serde_json::Value::Object(map) = &mut value {
        map.retain(|k, _| k == "id" || fields.iter().any(|f| f == k));
    }
    value
}

/// Per-request access scope for field-level security, injected by the application's
/// auth middleware as an axum `Extension`. `Platform` sees every field (e.g. a
/// superadmin / root); `Tenant(id)` is scoped to one owner id, compared against an
/// entity's `@owner` field to decide whether `@private` fields are visible.
#[derive(Debug, Clone)]
pub enum AccessScope {
    /// Full visibility — platform/root caller.
    Platform,
    /// Scoped to a single owner/tenant id (e.g. the caller's provider id).
    Tenant(String),
}

/// Enforce field-level security on an already-serialized response object: strip the
/// entity's `@private` fields unless the caller may see them. Visibility rule —
/// `Platform` → all; `Tenant(id)` → only when the row's `@owner` field equals `id`;
/// absent scope → treated as non-owner (fail-closed). Runs BEFORE sparse projection
/// so the security ceiling always beats a `?fields=` request.
fn apply_field_security(
    mut value: serde_json::Value,
    scope: Option<&AccessScope>,
    private_fields: &[&str],
    owner_field: Option<&str>,
) -> serde_json::Value {
    if private_fields.is_empty() {
        return value;
    }
    let can_see_private = match scope {
        Some(AccessScope::Platform) => true,
        Some(AccessScope::Tenant(id)) => owner_field
            .and_then(|f| value.get(f))
            .and_then(|v| v.as_str())
            .is_some_and(|owner| owner == id),
        None => false,
    };
    if !can_see_private {
        if let serde_json::Value::Object(map) = &mut value {
            for f in private_fields {
                map.remove(*f);
            }
        }
    }
    value
}

/// Hard cap on page size. Mirrors the clamp in
/// `backbone_orm::PaginationParams::new` (`per_page.clamp(1, 100)`); kept here so
/// the offset-depth check below computes the same effective offset the DB uses.
pub const MAX_PER_PAGE: u32 = 100;

/// Maximum number of rows offset pagination is allowed to skip. Requests that
/// would page deeper than this are rejected with `400` so clients narrow their
/// query with filters instead of deep-scanning the table (`OFFSET` cost grows
/// with depth). At `MAX_PER_PAGE` this allows ~100 pages.
pub const MAX_PAGINATION_OFFSET: u32 = 10_000;

/// Reject list requests that page beyond [`MAX_PAGINATION_OFFSET`].
///
/// Returns `Some(message)` describing the violation, or `None` when the request
/// is within bounds. The page size is clamped to [`MAX_PER_PAGE`] first so the
/// computed offset matches what the repository actually issues to Postgres.
fn pagination_depth_error(page: u32, limit: u32) -> Option<String> {
    let effective_limit = limit.clamp(1, MAX_PER_PAGE);
    let offset = page.max(1).saturating_sub(1).saturating_mul(effective_limit);
    if offset > MAX_PAGINATION_OFFSET {
        Some(format!(
            "Result set too deep: offset {offset} exceeds the maximum of \
             {MAX_PAGINATION_OFFSET}. Please add filters to narrow your search."
        ))
    } else {
        None
    }
}

/// Maximum number of ids / items a single batch request may contain. Larger
/// payloads are rejected with `400` to bound memory and transaction size.
///
/// Re-exported from the service layer, which enforces the same cap for non-HTTP
/// callers — see [`crate::service::MAX_BATCH_SIZE`].
pub use crate::service::MAX_BATCH_SIZE;

/// Reject batch requests larger than [`MAX_BATCH_SIZE`]. Returns `Some(message)`
/// when over the limit, `None` otherwise.
fn batch_size_error(count: usize) -> Option<String> {
    if count > MAX_BATCH_SIZE {
        Some(format!(
            "Batch too large: {count} items exceeds the maximum of {MAX_BATCH_SIZE}."
        ))
    } else {
        None
    }
}

/// Classify a list/query error as a client fault (bad filter or sort key) vs a
/// genuine server fault.
///
/// Unknown query params flow into the filter map and are injected as column
/// names, so a typo or a stray param (e.g. camelCase `sortOrder`) surfaces as a
/// Postgres `column "..." does not exist` (SQLSTATE 42703) or an
/// `invalid input syntax` cast error. Those are caused by the request, so they
/// should be a 400, not a 500.
fn is_bad_query_error(msg: &str) -> bool {
    let m = msg.to_lowercase();
    m.contains("does not exist")
        || m.contains("invalid input syntax")
        || m.contains("42703")
}

/// Pagination response metadata
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub meta: PaginationResponse,
}

/// Paginated API response with success flag, data array, and metadata at top level
/// This is the preferred response format for list endpoints
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BulkCreateRequest<T> {
    pub items: Vec<T>,
}

/// Bulk response
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BulkResponse<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}

/// Upsert request (update or insert)
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpsertRequest<T> {
    pub entity: T,
    pub create_if_not_exists: bool,
}

/// Request body carrying a list of entity ids — used by bulk soft-delete, bulk
/// restore, and bulk permanent-delete endpoints.
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BatchIdsRequest {
    pub ids: Vec<String>,
}

/// One element of the `PUT {resource}/bulk` array: an id plus the (flattened)
/// full update DTO for that row.
#[derive(Debug, Deserialize)]
#[serde(bound = "U: DeserializeOwned")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BulkUpdateItem<U> {
    pub id: String,
    // The flattened DTO is opaque to the schema (it is generic over the concrete
    // update type); documented as an open object — downstream specs reference the
    // concrete `Update<Entity>` schema directly.
    #[serde(flatten)]
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub data: U,
}

/// One element of the per-id form of a bulk PATCH request.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BulkPatchItem {
    pub id: String,
    pub patch: HashMap<String, serde_json::Value>,
}

/// Body for `PATCH {resource}/bulk`. Accepts either a single patch applied to
/// many ids, or a distinct patch per id. Shape is auto-detected.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum BulkPatchRequest {
    /// `{ "ids": [..], "patch": { .. } }` — same change applied to every id.
    Shared {
        ids: Vec<String>,
        patch: HashMap<String, serde_json::Value>,
    },
    /// `{ "items": [ { "id": .., "patch": { .. } } ] }` — per-id changes.
    PerItem { items: Vec<BulkPatchItem> },
}

impl BulkPatchRequest {
    /// Flatten into `(id, field_map)` pairs the service layer consumes.
    fn into_items(self) -> Vec<(String, HashMap<String, serde_json::Value>)> {
        match self {
            BulkPatchRequest::Shared { ids, patch } => {
                ids.into_iter().map(|id| (id, patch.clone())).collect()
            }
            BulkPatchRequest::PerItem { items } => {
                items.into_iter().map(|it| (it.id, it.patch)).collect()
            }
        }
    }
}

/// Filtering and sorting options
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FilterOptions {
    pub filters: HashMap<String, String>,
    pub sort_by: Option<String>,
    pub sort_order: Option<SortOrder>,
}

/// Sort order enum
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum SortOrder {
    #[default]
    Asc,
    Desc,
}

/// Extended pagination request with filtering and sorting
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
    // `'static` is required so the default batch methods (which hold these types
    // across `.await` points) satisfy async-trait's `'async_trait` bound.
    Entity: Send + Sync + 'static,
    CreateDto: Send + Sync + 'static,
    UpdateDto: Send + Sync + 'static,
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

    // ── Atomic batch operations ───────────────────────────────────────────────
    //
    // The default implementations are best-effort loops over the single-row
    // methods, provided so non-generic implementors keep compiling. The blanket
    // impl on `GenericCrudService` overrides them with transactional,
    // all-or-nothing versions (which is what every generated service runs).

    /// 17. Soft-delete many entities by id. Returns the number affected.
    async fn bulk_soft_delete(&self, ids: Vec<String>) -> Result<u64, Self::Error> {
        let mut n = 0;
        for id in ids {
            if self.soft_delete(&id).await? {
                n += 1;
            }
        }
        Ok(n)
    }

    /// 18. Restore many soft-deleted entities by id. Returns the restored rows.
    async fn bulk_restore(&self, ids: Vec<String>) -> Result<Vec<Entity>, Self::Error> {
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(e) = self.restore(&id).await? {
                out.push(e);
            }
        }
        Ok(out)
    }

    /// 19. Permanently delete many soft-deleted entities by id.
    async fn bulk_permanent_delete(&self, ids: Vec<String>) -> Result<u64, Self::Error> {
        let mut n = 0;
        for id in ids {
            if self.permanent_delete(&id).await? {
                n += 1;
            }
        }
        Ok(n)
    }

    /// 20. Restore every soft-deleted entity. Returns the number restored.
    ///
    /// Default is a no-op (`0`) — only the `GenericCrudService` blanket impl can
    /// perform this without an id list.
    async fn restore_all(&self) -> Result<u64, Self::Error> {
        Ok(0)
    }

    /// 21. Full-update many entities. Each item is `(id, UpdateDto)`.
    async fn bulk_update(&self, items: Vec<(String, UpdateDto)>) -> Result<Vec<Entity>, Self::Error> {
        let mut out = Vec::with_capacity(items.len());
        for (id, dto) in items {
            if let Some(e) = self.update(&id, dto).await? {
                out.push(e);
            }
        }
        Ok(out)
    }

    /// 22. Partial-update many entities. Each item is `(id, field_map)`.
    async fn bulk_partial_update(
        &self,
        items: Vec<(String, HashMap<String, serde_json::Value>)>,
    ) -> Result<Vec<Entity>, Self::Error> {
        let mut out = Vec::with_capacity(items.len());
        for (id, fields) in items {
            if let Some(e) = self.partial_update(&id, fields).await? {
                out.push(e);
            }
        }
        Ok(out)
    }
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
    E: Serialize + Send + Sync + Clone + backbone_orm::EntityRepoMeta + 'static,
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
            Extension, Router,
        };

        let handler = Arc::new(Self::new(service));

        Router::new()
            // GET /collection - List
            .route(base_path, get({
                let h = handler.clone();
                move |query: Query<ListQueryParams>, access: Option<Extension<AccessScope>>| async move {
                    Self::list_handler(h, query, access).await
                }
            }))
            // GET /collection/trash - List deleted
            .route(&format!("{}/trash", base_path), get({
                let h = handler.clone();
                move |query: Query<ListQueryParams>, access: Option<Extension<AccessScope>>| async move {
                    Self::list_deleted_handler(h, query, access).await
                }
            }))
            // GET /collection/:id - Get by ID
            .route(&format!("{}/:id", base_path), get({
                let h = handler.clone();
                move |path: Path<String>, query: Query<ListQueryParams>, access: Option<Extension<AccessScope>>| async move {
                    Self::get_handler(h, path, query, access).await
                }
            }))
            // GET /collection/:id/deleted - Get deleted by ID
            .route(&format!("{}/:id/deleted", base_path), get({
                let h = handler.clone();
                move |path: Path<String>, query: Query<ListQueryParams>, access: Option<Extension<AccessScope>>| async move {
                    Self::get_deleted_handler(h, path, query, access).await
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
            Router,
        };

        let handler = Arc::new(Self::new(service));

        Router::new()
            // POST /collection - Create
            .route(base_path, post({
                let h = handler.clone();
                move |body: JsonOrForm<C>| async move {
                    Self::create_handler(h, body).await
                }
            }))
            // POST /collection/bulk - Bulk create
            .route(&format!("{}/bulk", base_path), post({
                let h = handler.clone();
                move |body: JsonOrForm<Vec<C>>| async move {
                    Self::bulk_create_handler(h, body).await
                }
            }))
            // POST /collection/upsert - Upsert
            .route(&format!("{}/upsert", base_path), post({
                let h = handler.clone();
                move |body: JsonOrForm<C>| async move {
                    Self::upsert_handler(h, body).await
                }
            }))
            // POST /collection/delete/bulk - Bulk soft delete by ids
            .route(&format!("{}/delete/bulk", base_path), post({
                let h = handler.clone();
                move |body: JsonOrForm<BatchIdsRequest>| async move {
                    Self::bulk_delete_handler(h, body).await
                }
            }))
            // POST /collection/restore/bulk - Bulk restore by ids
            .route(&format!("{}/restore/bulk", base_path), post({
                let h = handler.clone();
                move |body: JsonOrForm<BatchIdsRequest>| async move {
                    Self::bulk_restore_handler(h, body).await
                }
            }))
            // POST /collection/restore/all - Restore all soft-deleted
            .route(&format!("{}/restore/all", base_path), post({
                let h = handler.clone();
                move || async move {
                    Self::restore_all_handler(h).await
                }
            }))
            // DELETE /collection/trash/bulk - Bulk hard delete by ids
            // (registered alongside /trash/:id; matchit prioritises the static segment)
            .route(&format!("{}/trash/bulk", base_path), delete({
                let h = handler.clone();
                move |body: JsonOrForm<BatchIdsRequest>| async move {
                    Self::bulk_permanent_delete_handler(h, body).await
                }
            }))
            // PUT /collection/bulk - Bulk full update
            .route(&format!("{}/bulk", base_path), put({
                let h = handler.clone();
                move |body: JsonOrForm<Vec<BulkUpdateItem<U>>>| async move {
                    Self::bulk_update_handler(h, body).await
                }
            }))
            // PATCH /collection/bulk - Bulk partial update (shared or per-id)
            .route(&format!("{}/bulk", base_path), patch({
                let h = handler.clone();
                move |body: JsonOrForm<BulkPatchRequest>| async move {
                    Self::bulk_patch_handler(h, body).await
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
                move |path: Path<String>, body: JsonOrForm<U>| async move {
                    Self::update_handler(h, path, body).await
                }
            }))
            // PATCH /collection/:id - Partial update
            .route(&format!("{}/:id", base_path), patch({
                let h = handler.clone();
                move |path: Path<String>, body: JsonOrForm<HashMap<String, serde_json::Value>>| async move {
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
        access: Option<axum::Extension<AccessScope>>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        if let Some(err) = pagination_depth_error(params.page, params.limit) {
            return (StatusCode::BAD_REQUEST, Json(PaginatedApiResponse::<serde_json::Value>::error(err)));
        }

        // Sparse fieldset (`?fields=a,b,c`) is response-shaping, not a filter — read it,
        // then drop the reserved keys so they're never passed to the repository.
        let fields = sparse_fields(&params.filters);
        let scope = access.map(|axum::Extension(s)| s);

        // Merge search and status into filters HashMap
        let mut filters = params.filters.clone();
        for key in RESERVED_QUERY_KEYS {
            filters.remove(key);
        }
        if let Some(search) = params.search {
            filters.insert("search".to_string(), search);
        }
        if let Some(status) = params.status {
            filters.insert("status".to_string(), status);
        }

        match handler.service.list(params.page, params.limit, filters).await {
            Ok((entities, total)) => {
                let items: Vec<serde_json::Value> = entities
                    .into_iter()
                    .map(|e| {
                        let secured = apply_field_security(
                            to_response_value(R::from(e)),
                            scope.as_ref(),
                            E::private_fields(),
                            E::owner_field(),
                        );
                        project_sparse(secured, &fields)
                    })
                    .collect();
                let response = PaginatedApiResponse::ok(items, total, params.page, params.limit);
                (StatusCode::OK, Json(response))
            }
            Err(e) => {
                let msg = e.to_string();
                if is_bad_query_error(&msg) {
                    (StatusCode::BAD_REQUEST, Json(PaginatedApiResponse::<serde_json::Value>::error(
                        format!("Invalid query parameter or filter: {msg}"),
                    )))
                } else {
                    (StatusCode::INTERNAL_SERVER_ERROR, Json(PaginatedApiResponse::<serde_json::Value>::error(msg)))
                }
            }
        }
    }

    async fn create_handler(
        handler: Arc<Self>,
        JsonOrForm(dto): JsonOrForm<C>,
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
        axum::extract::Query(params): axum::extract::Query<ListQueryParams>,
        access: Option<axum::Extension<AccessScope>>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        let fields = sparse_fields(&params.filters);
        let scope = access.map(|axum::Extension(s)| s);

        match handler.service.get_by_id(&id).await {
            Ok(Some(entity)) => {
                let secured = apply_field_security(
                    to_response_value(R::from(entity)),
                    scope.as_ref(),
                    E::private_fields(),
                    E::owner_field(),
                );
                let value = project_sparse(secured, &fields);
                (StatusCode::OK, Json(ApiResponse::ok(value)))
            }
            Ok(None) => {
                (StatusCode::NOT_FOUND, Json(ApiResponse::<serde_json::Value>::not_found(S::entity_name(), &id)))
            }
            Err(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::<serde_json::Value>::error(e.to_string())))
            }
        }
    }

    async fn update_handler(
        handler: Arc<Self>,
        axum::extract::Path(id): axum::extract::Path<String>,
        JsonOrForm(dto): JsonOrForm<U>,
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
        JsonOrForm(fields): JsonOrForm<HashMap<String, serde_json::Value>>,
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
        JsonOrForm(items): JsonOrForm<Vec<C>>,
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

    // ── Batch handlers ────────────────────────────────────────────────────────

    async fn bulk_delete_handler(
        handler: Arc<Self>,
        JsonOrForm(req): JsonOrForm<BatchIdsRequest>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        if let Some(err) = batch_size_error(req.ids.len()) {
            return (StatusCode::BAD_REQUEST, Json(ApiResponse::<serde_json::Value>::error(err)));
        }
        match handler.service.bulk_soft_delete(req.ids).await {
            Ok(count) => (StatusCode::OK, Json(ApiResponse::success_with_message(
                serde_json::json!({ "soft_deleted": count }),
                format!("Soft-deleted {count} item(s)"),
            ))),
            Err(e) => (StatusCode::BAD_REQUEST, Json(ApiResponse::<serde_json::Value>::error(e.to_string()))),
        }
    }

    async fn bulk_restore_handler(
        handler: Arc<Self>,
        JsonOrForm(req): JsonOrForm<BatchIdsRequest>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        if let Some(err) = batch_size_error(req.ids.len()) {
            return (StatusCode::BAD_REQUEST, Json(ApiResponse::<BulkResponse<R>>::error(err)));
        }
        match handler.service.bulk_restore(req.ids).await {
            Ok(entities) => {
                let items: Vec<R> = entities.into_iter().map(R::from).collect();
                let total = items.len();
                (StatusCode::OK, Json(ApiResponse::ok(BulkResponse { items, total, failed: 0, errors: vec![] })))
            }
            Err(e) => (StatusCode::BAD_REQUEST, Json(ApiResponse::<BulkResponse<R>>::error(e.to_string()))),
        }
    }

    async fn restore_all_handler(
        handler: Arc<Self>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        match handler.service.restore_all().await {
            Ok(count) => (StatusCode::OK, Json(ApiResponse::success_with_message(
                serde_json::json!({ "restored": count }),
                format!("Restored {count} item(s) from trash"),
            ))),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::<serde_json::Value>::error(e.to_string()))),
        }
    }

    async fn bulk_permanent_delete_handler(
        handler: Arc<Self>,
        JsonOrForm(req): JsonOrForm<BatchIdsRequest>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        if let Some(err) = batch_size_error(req.ids.len()) {
            return (StatusCode::BAD_REQUEST, Json(ApiResponse::<serde_json::Value>::error(err)));
        }
        match handler.service.bulk_permanent_delete(req.ids).await {
            Ok(count) => (StatusCode::OK, Json(ApiResponse::success_with_message(
                serde_json::json!({ "permanently_deleted": count }),
                format!("Permanently deleted {count} item(s)"),
            ))),
            Err(e) => (StatusCode::BAD_REQUEST, Json(ApiResponse::<serde_json::Value>::error(e.to_string()))),
        }
    }

    async fn bulk_update_handler(
        handler: Arc<Self>,
        JsonOrForm(items): JsonOrForm<Vec<BulkUpdateItem<U>>>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        if let Some(err) = batch_size_error(items.len()) {
            return (StatusCode::BAD_REQUEST, Json(ApiResponse::<BulkResponse<R>>::error(err)));
        }
        let items: Vec<(String, U)> = items.into_iter().map(|it| (it.id, it.data)).collect();
        match handler.service.bulk_update(items).await {
            Ok(entities) => {
                let items: Vec<R> = entities.into_iter().map(R::from).collect();
                let total = items.len();
                (StatusCode::OK, Json(ApiResponse::ok(BulkResponse { items, total, failed: 0, errors: vec![] })))
            }
            Err(e) => (StatusCode::BAD_REQUEST, Json(ApiResponse::<BulkResponse<R>>::error(e.to_string()))),
        }
    }

    async fn bulk_patch_handler(
        handler: Arc<Self>,
        JsonOrForm(req): JsonOrForm<BulkPatchRequest>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        let items = req.into_items();
        if let Some(err) = batch_size_error(items.len()) {
            return (StatusCode::BAD_REQUEST, Json(ApiResponse::<BulkResponse<R>>::error(err)));
        }
        // Normalize patch keys to snake_case (mirrors partial_update_handler).
        let items: Vec<(String, HashMap<String, serde_json::Value>)> = items
            .into_iter()
            .map(|(id, fields)| {
                let fields = fields
                    .into_iter()
                    .map(|(k, v)| (camel_to_snake_case(&k), v))
                    .collect();
                (id, fields)
            })
            .collect();
        match handler.service.bulk_partial_update(items).await {
            Ok(entities) => {
                let items: Vec<R> = entities.into_iter().map(R::from).collect();
                let total = items.len();
                (StatusCode::OK, Json(ApiResponse::ok(BulkResponse { items, total, failed: 0, errors: vec![] })))
            }
            Err(e) => (StatusCode::BAD_REQUEST, Json(ApiResponse::<BulkResponse<R>>::error(e.to_string()))),
        }
    }

    async fn upsert_handler(
        handler: Arc<Self>,
        JsonOrForm(dto): JsonOrForm<C>,
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
        access: Option<axum::Extension<AccessScope>>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        if let Some(err) = pagination_depth_error(params.page, params.limit) {
            return (StatusCode::BAD_REQUEST, Json(PaginatedApiResponse::<serde_json::Value>::error(err)));
        }

        let fields = sparse_fields(&params.filters);
        let scope = access.map(|axum::Extension(s)| s);

        match handler.service.list_deleted(params.page, params.limit).await {
            Ok((entities, total)) => {
                let items: Vec<serde_json::Value> = entities
                    .into_iter()
                    .map(|e| {
                        let secured = apply_field_security(
                            to_response_value(R::from(e)),
                            scope.as_ref(),
                            E::private_fields(),
                            E::owner_field(),
                        );
                        project_sparse(secured, &fields)
                    })
                    .collect();
                let response = PaginatedApiResponse::ok(items, total, params.page, params.limit);
                (StatusCode::OK, Json(response))
            }
            Err(e) => {
                let msg = e.to_string();
                if is_bad_query_error(&msg) {
                    (StatusCode::BAD_REQUEST, Json(PaginatedApiResponse::<serde_json::Value>::error(
                        format!("Invalid query parameter or filter: {msg}"),
                    )))
                } else {
                    (StatusCode::INTERNAL_SERVER_ERROR, Json(PaginatedApiResponse::<serde_json::Value>::error(msg)))
                }
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
        axum::extract::Query(params): axum::extract::Query<ListQueryParams>,
        access: Option<axum::Extension<AccessScope>>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        let fields = sparse_fields(&params.filters);
        let scope = access.map(|axum::Extension(s)| s);

        match handler.service.get_deleted_by_id(&id).await {
            Ok(Some(entity)) => {
                let secured = apply_field_security(
                    to_response_value(R::from(entity)),
                    scope.as_ref(),
                    E::private_fields(),
                    E::owner_field(),
                );
                let value = project_sparse(secured, &fields);
                (StatusCode::OK, Json(ApiResponse::ok(value)))
            }
            Ok(None) => {
                (StatusCode::NOT_FOUND, Json(ApiResponse::<serde_json::Value>::not_found(&format!("Deleted {}", S::entity_name()), &id)))
            }
            Err(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::<serde_json::Value>::error(e.to_string())))
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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

    #[test]
    fn shallow_pages_are_allowed() {
        // page 1 has zero offset regardless of size
        assert!(pagination_depth_error(1, 100).is_none());
        // last in-bounds page at max size: offset = 100 * 100 = 10_000 (== cap)
        assert!(pagination_depth_error(101, 100).is_none());
    }

    #[test]
    fn pages_past_the_cap_are_rejected() {
        // page 102 at size 100 → offset 10_100 > 10_000
        assert!(pagination_depth_error(102, 100).is_some());
        // small page size still rejected once deep enough: 1001 * 10 = 10_010
        assert!(pagination_depth_error(1002, 10).is_some());
    }

    #[test]
    fn oversized_page_size_is_clamped_before_the_check() {
        // limit=200 is clamped to 100, so the effective offset is 101*100=10_100
        assert!(pagination_depth_error(102, 200).is_some());
        // and a request that would only be in-bounds *because* of the clamp passes
        assert!(pagination_depth_error(101, 200).is_none());
    }

    #[test]
    fn huge_page_number_does_not_overflow() {
        // saturating arithmetic keeps this a clean rejection, not a panic
        assert!(pagination_depth_error(u32::MAX, u32::MAX).is_some());
    }

    // ── Batch operations ──────────────────────────────────────────────────────

    #[test]
    fn batch_size_within_limit_is_allowed() {
        assert!(batch_size_error(0).is_none());
        assert!(batch_size_error(MAX_BATCH_SIZE).is_none());
    }

    #[test]
    fn batch_size_over_limit_is_rejected() {
        assert!(batch_size_error(MAX_BATCH_SIZE + 1).is_some());
    }

    #[test]
    fn bulk_patch_request_parses_shared_shape() {
        let json = r#"{ "ids": ["a", "b"], "patch": { "status": "void" } }"#;
        let req: BulkPatchRequest = serde_json::from_str(json).unwrap();
        let items = req.into_items();
        assert_eq!(items.len(), 2);
        // Both ids carry the same patch.
        assert_eq!(items[0].1.get("status").unwrap(), "void");
        assert_eq!(items[1].1.get("status").unwrap(), "void");
        assert_eq!(items[0].0, "a");
        assert_eq!(items[1].0, "b");
    }

    #[test]
    fn bulk_patch_request_parses_per_item_shape() {
        let json = r#"{ "items": [
            { "id": "a", "patch": { "status": "void" } },
            { "id": "b", "patch": { "note": "late" } }
        ] }"#;
        let req: BulkPatchRequest = serde_json::from_str(json).unwrap();
        let items = req.into_items();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].0, "a");
        assert_eq!(items[0].1.get("status").unwrap(), "void");
        assert_eq!(items[1].0, "b");
        assert_eq!(items[1].1.get("note").unwrap(), "late");
    }

    #[test]
    fn batch_ids_request_parses() {
        let req: BatchIdsRequest = serde_json::from_str(r#"{ "ids": ["x", "y", "z"] }"#).unwrap();
        assert_eq!(req.ids, vec!["x", "y", "z"]);
    }

    // ── Sparse fieldsets (?fields=a,b,c) ──

    fn fields(q: &[(&str, &str)]) -> Vec<String> {
        let map: HashMap<String, String> =
            q.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        sparse_fields(&map)
    }

    #[test]
    fn sparse_fields_parses_comma_list_and_trims() {
        assert_eq!(fields(&[("fields", "id, name ,basePrice")]), vec!["id", "name", "basePrice"]);
    }

    #[test]
    fn sparse_fields_absent_or_empty_is_no_projection() {
        assert!(fields(&[]).is_empty());
        assert!(fields(&[("fields", "")]).is_empty());
        assert!(fields(&[("fields", " , ")]).is_empty());
    }

    #[test]
    fn project_keeps_requested_keys_plus_id() {
        let v = serde_json::json!({ "id": "1", "name": "n", "basePrice": 10, "hppPerUnit": 5 });
        let out = project_sparse(v, &["name".into(), "basePrice".into()]);
        assert_eq!(out, serde_json::json!({ "id": "1", "name": "n", "basePrice": 10 }));
    }

    #[test]
    fn project_always_includes_id_even_if_not_requested() {
        let v = serde_json::json!({ "id": "1", "name": "n" });
        let out = project_sparse(v, &["name".into()]);
        assert_eq!(out, serde_json::json!({ "id": "1", "name": "n" }));
    }

    #[test]
    fn project_ignores_unknown_keys() {
        let v = serde_json::json!({ "id": "1", "name": "n" });
        let out = project_sparse(v, &["name".into(), "nope".into()]);
        assert_eq!(out, serde_json::json!({ "id": "1", "name": "n" }));
    }

    #[test]
    fn project_empty_fields_returns_full_object() {
        let v = serde_json::json!({ "id": "1", "name": "n", "x": 2 });
        let out = project_sparse(v.clone(), &[]);
        assert_eq!(out, v);
    }

    #[test]
    fn project_non_object_returned_unchanged() {
        let v = serde_json::json!("scalar");
        assert_eq!(project_sparse(v.clone(), &["name".into()]), v);
    }

    // ── Field-level security (@private / @owner) ──

    fn row() -> serde_json::Value {
        serde_json::json!({ "id": "1", "providerId": "prov-A", "name": "n", "hppPerUnit": 5 })
    }
    const PRIV: &[&str] = &["hppPerUnit"];

    #[test]
    fn security_no_private_fields_is_noop() {
        let v = row();
        assert_eq!(apply_field_security(v.clone(), None, &[], Some("providerId")), v);
    }

    #[test]
    fn security_platform_sees_private() {
        let out = apply_field_security(row(), Some(&AccessScope::Platform), PRIV, Some("providerId"));
        assert!(out.get("hppPerUnit").is_some());
    }

    #[test]
    fn security_owner_tenant_sees_private() {
        let out = apply_field_security(
            row(),
            Some(&AccessScope::Tenant("prov-A".into())),
            PRIV,
            Some("providerId"),
        );
        assert!(out.get("hppPerUnit").is_some());
    }

    #[test]
    fn security_other_tenant_stripped() {
        let out = apply_field_security(
            row(),
            Some(&AccessScope::Tenant("prov-B".into())),
            PRIV,
            Some("providerId"),
        );
        assert!(out.get("hppPerUnit").is_none());
        assert!(out.get("name").is_some());
    }

    #[test]
    fn security_absent_scope_fails_closed() {
        let out = apply_field_security(row(), None, PRIV, Some("providerId"));
        assert!(out.get("hppPerUnit").is_none());
    }

    #[test]
    fn security_null_owner_only_platform_sees() {
        let v = serde_json::json!({ "id": "1", "providerId": null, "hppPerUnit": 5 });
        let tenant = apply_field_security(v.clone(), Some(&AccessScope::Tenant("prov-A".into())), PRIV, Some("providerId"));
        assert!(tenant.get("hppPerUnit").is_none());
        let plat = apply_field_security(v, Some(&AccessScope::Platform), PRIV, Some("providerId"));
        assert!(plat.get("hppPerUnit").is_some());
    }
}