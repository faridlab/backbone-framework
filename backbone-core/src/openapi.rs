//! OpenAPI/Swagger support (feature `openapi`).
//!
//! `backbone-core`'s HTTP handlers are generic over the entity, DTO, and response
//! types (`BackboneCrudHandler<S, E, C, U, R>`), and the mount path (`base_path`)
//! is chosen at runtime. utoipa's `#[utoipa::path]` macro, by contrast, needs
//! *concrete* types and *literal* paths — so this crate cannot emit a finished,
//! per-entity spec on its own.
//!
//! What it ships instead, behind the `openapi` feature:
//!
//! - `utoipa::ToSchema` derives on the shared, concrete envelope and request types
//!   (`ApiResponse<T>`, `PaginatedApiResponse<T>`, `BulkResponse<T>`,
//!   `BatchIdsRequest`, `BulkPatchRequest`, …). These are the components every
//!   Backbone CRUD service has in common.
//! - [`BackboneComponents`], a reusable [`utoipa::OpenApi`] document that registers
//!   the non-generic component schemas. Downstream entity crates merge it into
//!   their own `#[derive(OpenApi)]` aggregator and add their concrete
//!   `#[utoipa::path]` handlers plus per-entity generic schemas
//!   (`ApiResponse<Product>`, `PaginatedApiResponse<Product>`, …).
//!
//! See `docs/openapi.md` for a worked downstream example and instructions on
//! serving the resulting spec with Swagger UI, Redoc, or Scalar.

use utoipa::OpenApi;

/// Reusable OpenAPI component schemas shared by every Backbone CRUD service.
///
/// This registers only the **non-generic** component schemas. Generic envelopes
/// such as `ApiResponse<T>` and `PaginatedApiResponse<T>` require a concrete entity
/// type, so they are registered downstream alongside the entity's own DTO schemas.
///
/// # Example
///
/// Merge into a downstream aggregator and add your concrete paths + entity schemas:
///
/// ```ignore
/// use utoipa::OpenApi;
/// use backbone_core::{ApiResponse, PaginatedApiResponse, BackboneComponents};
///
/// #[derive(OpenApi)]
/// #[openapi(
///     paths(list_products, get_product, create_product),
///     components(schemas(
///         Product,
///         ApiResponse<Product>,
///         PaginatedApiResponse<Product>,
///     )),
///     nest((path = "/", api = BackboneComponents)),
/// )]
/// struct ProductApiDoc;
/// ```
#[derive(OpenApi)]
#[openapi(components(schemas(
    crate::http::PaginationResponse,
    crate::http::SortOrder,
    crate::http::FilterOptions,
    crate::http::ListRequest,
    crate::http::PaginationRequest,
    crate::http::BatchIdsRequest,
    crate::http::BulkPatchItem,
    crate::http::BulkPatchRequest,
)))]
pub struct BackboneComponents;
