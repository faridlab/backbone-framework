//! HTTP layer and REST endpoint implementations
//!
//! This module provides generic HTTP/REST components for the Backbone CRUD system:
//! - `CrudService` trait: Core async trait for entity CRUD operations
//! - `BackboneCrudHandler`: Generic Axum router builder for all 11 endpoints
//! - Response types: `ApiResponse`, `PaginatedResponse`, `BulkResponse`

use crate::extractors::{JsonOrForm, StrictJsonOrForm};
pub use crate::permission::{
    guarded, guarded_all, permission_refusal, unauthenticated_refusal, CrudPermissions,
};
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
    /// One entry per broken rule, beside the `error` sentence, when the
    /// service refused the write for named reasons. Absent otherwise, so a
    /// body without violations is unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub violations: Option<Vec<crate::violation::Violation>>,
}

impl<T> ApiResponse<T> {
    /// A refused write: the sentence for clients that read `error`, and each
    /// broken rule for clients that mark fields.
    pub fn rejected(error: impl Into<String>, violations: Vec<crate::violation::Violation>) -> Self {
        Self {
            success: false,
            data: None,
            message: None,
            error: Some(error.into()),
            violations: (!violations.is_empty()).then_some(violations),
        }
    }

    /// Create a success response with data and an optional message
    pub fn success(data: T, message: Option<String>) -> Self {
        Self {
            success: true,
            data: Some(data),
            message,
            error: None,
            violations: None,
        }
    }

    /// Create a success response without a message (convenience method)
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: None,
            error: None,
            violations: None,
        }
    }

    pub fn success_with_message(data: T, message: impl Into<String>) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: Some(message.into()),
            error: None,
            violations: None,
        }
    }

    pub fn error(error: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            message: None,
            error: Some(error.into()),
            violations: None,
        }
    }

    /// A missing record: [`NOT_FOUND_MESSAGE`] and a `not_found` violation, the same body for
    /// every resource. `entity` and `id` go to the debug log only — a type name can carry the
    /// crate's module path, and the body has no use for either.
    pub fn not_found(entity: &str, id: &str) -> Self {
        tracing::debug!(target: "backbone::http", entity, id, "record not found");
        Self::rejected(NOT_FOUND_MESSAGE, vec![not_found_violation()])
    }
}

/// The sentence a 404 answers with.
pub const NOT_FOUND_MESSAGE: &str = "Not found";

fn not_found_violation() -> crate::violation::Violation {
    crate::violation::Violation::new("", "not_found", NOT_FOUND_MESSAGE)
}

/// The code a client refusal that names no rule answers with, by status: every 4xx body
/// carries a machine-readable code, whatever layer refused.
fn status_code_violation(status: axum::http::StatusCode, message: &str) -> Option<crate::violation::Violation> {
    use axum::http::StatusCode;
    let code = match status {
        StatusCode::NOT_FOUND => return Some(not_found_violation()),
        StatusCode::CONFLICT => "conflict",
        StatusCode::BAD_REQUEST => "invalid_request",
        StatusCode::UNPROCESSABLE_ENTITY => "invalid_value",
        _ => return None,
    };
    Some(crate::violation::Violation::new("", code, message))
}

/// The sentence a 500 answers with. The real error is logged, never sent: it can carry SQL,
/// table names and driver text a client has no use for and an attacker does.
pub const INTERNAL_ERROR_MESSAGE: &str = "An internal error occurred. It has been logged.";

/// The response to a failed call at `status`: 422 listing the broken rules when there are
/// any, the error's sentence for any other client fault, and a generic sentence for a server
/// fault, whose detail goes to the log instead.
fn error_at<T>(
    status: axum::http::StatusCode,
    violations: Option<Vec<crate::violation::Violation>>,
    err: &impl std::fmt::Display,
) -> (axum::http::StatusCode, axum::Json<ApiResponse<T>>) {
    if status.is_server_error() {
        tracing::error!(target: "backbone::http", status = status.as_u16(), error = %err, "request failed");
        return (status, axum::Json(ApiResponse::error(INTERNAL_ERROR_MESSAGE)));
    }
    match violations {
        Some(v) if !v.is_empty() => (status, axum::Json(ApiResponse::rejected(err.to_string(), v))),
        _ if status == axum::http::StatusCode::NOT_FOUND => {
            tracing::debug!(target: "backbone::http", error = %err, "not found");
            (status, axum::Json(ApiResponse::rejected(NOT_FOUND_MESSAGE, vec![not_found_violation()])))
        }
        _ => {
            let message = err.to_string();
            match status_code_violation(status, &message) {
                Some(v) => (status, axum::Json(ApiResponse::rejected(message, vec![v]))),
                None => (status, axum::Json(ApiResponse::error(message))),
            }
        }
    }
}

/// Bind the request's correlation id for the writes it makes, so the events they stage (and
/// the logs and audit rows of the request) share one id.
///
/// A composing service that already attributes the request (its guard binds a
/// [`RequestAuditContext`](backbone_orm::RequestAuditContext) carrying the correlation id, which
/// it echoes on the response) is left alone: that id is the one the hooks read, so events, audit
/// rows and the response header agree. Otherwise the id is the request's `x-correlation-id`
/// header when it carries a usable one (printable ASCII, at most 128 characters), else a fresh
/// uuid. The generic write routes and the generated transition routes carry this layer; a
/// hand-written route that writes through the service can add it with
/// `axum::middleware::from_fn`.
pub async fn correlation_middleware(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let attributed = backbone_orm::audit_context::current_request_audit()
        .is_some_and(|audit| !audit.correlation_id.is_empty());
    if attributed {
        return next.run(req).await;
    }
    let id = req
        .headers()
        .get("x-correlation-id")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty() && v.len() <= 128 && v.bytes().all(|b| b.is_ascii_graphic()))
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    crate::write_hooks::with_correlation_id(id, next.run(req)).await
}

/// The status a [`ServiceError`](crate::service::ServiceError) answers with, by variant.
///
/// A refusal for named reasons — a protected field, a broken rule, a constraint the database
/// refused — is the caller's to fix: 422. A plain validation error is 400, a missing record
/// 404, a duplicate or conflict 409, and only what is left is a 500.
pub fn service_error_status(err: &crate::service::ServiceError) -> axum::http::StatusCode {
    use crate::persistence::traits::RepositoryError;
    use crate::service::ServiceError;
    use axum::http::StatusCode;
    match err {
        ServiceError::Violations(_) | ServiceError::Repository(RepositoryError::Violations(_)) => {
            StatusCode::UNPROCESSABLE_ENTITY
        }
        ServiceError::Validation(_)
        | ServiceError::Repository(RepositoryError::ValidationError(_)) => StatusCode::BAD_REQUEST,
        ServiceError::NotFound | ServiceError::Repository(RepositoryError::NotFound) => {
            StatusCode::NOT_FOUND
        }
        ServiceError::AlreadyExists(_)
        | ServiceError::Repository(RepositoryError::AlreadyExists(_))
        | ServiceError::Repository(RepositoryError::Conflict(_)) => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// The response for a failed [`ServiceError`](crate::service::ServiceError), for handlers outside
/// the generic CRUD handler (a generated state-machine transition, a custom route) that call the
/// service directly — the same answer the generic handler gives (see [`service_error_status`]).
/// A 500 carries [`INTERNAL_ERROR_MESSAGE`]; the error itself is logged.
pub fn service_error_response<T>(
    err: &crate::service::ServiceError,
) -> (axum::http::StatusCode, axum::Json<ApiResponse<T>>) {
    use crate::persistence::traits::RepositoryError;
    use crate::service::ServiceError;
    let violations = match err {
        ServiceError::Violations(v) | ServiceError::Repository(RepositoryError::Violations(v)) => {
            Some(v.clone())
        }
        _ => None,
    };
    error_at(service_error_status(err), violations, err)
}

/// The response to a failed write when the error's kind is unknown: 422 listing the broken
/// rules when there are any, `fallback` otherwise (a server fault answers generically).
#[cfg(test)]
fn write_error<T>(
    violations: Option<Vec<crate::violation::Violation>>,
    err: &impl std::fmt::Display,
    fallback: axum::http::StatusCode,
) -> (axum::http::StatusCode, axum::Json<ApiResponse<T>>) {
    let status = if violations.is_some() { axum::http::StatusCode::UNPROCESSABLE_ENTITY } else { fallback };
    error_at(status, violations, err)
}

/// A path id that is not a UUID, refused before it reaches a query: 400 with an `invalid_id`
/// violation on `path`. Every generic repository keys rows by UUID, so such an id can name no
/// record — and passed through, it failed the `::uuid` cast as a 500 with the driver's text.
fn invalid_id<T>(id: &str, path: &str) -> Option<(axum::http::StatusCode, axum::Json<ApiResponse<T>>)> {
    if uuid::Uuid::parse_str(id).is_ok() {
        return None;
    }
    let violation = crate::violation::Violation::new(
        path,
        "invalid_id",
        format!("`{id}` is not a valid record id"),
    );
    Some((
        axum::http::StatusCode::BAD_REQUEST,
        axum::Json(ApiResponse::rejected(format!("invalid id: `{id}` is not a valid record id"), vec![violation])),
    ))
}

/// [`invalid_id`] over a batch: every id that is not a UUID, each on `<prefix>/<index>` (or
/// `<index>/<suffix>` when `suffix` is given).
fn invalid_ids<'a, T>(
    ids: impl IntoIterator<Item = &'a str>,
    path_of: impl Fn(usize) -> String,
) -> Option<(axum::http::StatusCode, axum::Json<ApiResponse<T>>)> {
    let violations: Vec<crate::violation::Violation> = ids
        .into_iter()
        .enumerate()
        .filter(|(_, id)| uuid::Uuid::parse_str(id).is_err())
        .map(|(i, id)| {
            crate::violation::Violation::new(path_of(i), "invalid_id", format!("`{id}` is not a valid record id"))
        })
        .collect();
    if violations.is_empty() {
        return None;
    }
    Some((
        axum::http::StatusCode::BAD_REQUEST,
        axum::Json(ApiResponse::rejected("invalid id in the batch", violations)),
    ))
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

impl ListQueryParams {
    /// The page request every list route serves: page 0 reads as page 1, and the page size
    /// is held to `1..=`[`MAX_PER_PAGE`], as the repositories hold it, so the `meta` a
    /// response reports is the page actually read.
    pub fn normalized(mut self) -> Self {
        let page = backbone_orm::repository::PaginationParams::new(self.page, self.limit);
        self.page = page.page;
        self.limit = page.per_page;
        self
    }
}

/// Reserved response-shaping query keys (carved out of the filter grammar).
const RESERVED_QUERY_KEYS: [&str; 3] = ["fields", "include", "with"];

/// Parse the reserved `fields` key (comma-separated) into trimmed field names.
/// Absent/empty → empty vec, meaning "no projection — return every field".
// ─── Per-record history ───────────────────────────────────────────────────────

/// One recorded change to a record.
///
/// `changed` is the capture layer's diff shape, carried through verbatim:
/// `{field: {"from": old, "to": new}}` for an update, and the full image as
/// `{field: {"to": v}}` / `{field: {"from": v}}` for an insert / delete. The
/// trail is DIFF-ONLY, so an update entry names only the fields that actually
/// changed — a consumer wanting the record's state at a point in time must
/// re-anchor on the nearest full image and replay forward, which is what the
/// insert/delete images exist for.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub occurred_at: String,
    /// `insert` | `update` | `delete`, or a verb-emitted action.
    pub action: String,
    pub actor: String,
    pub changed: serde_json::Value,
    pub reason: Option<String>,
    pub correlation_id: Option<String>,
}

/// Supplies a record's change history to the generic CRUD router.
///
/// This exists so `backbone-core` never learns that an audit module exists.
/// The composing service builds a provider and installs it as an axum
/// `Extension`; a composition without one simply has no history to serve, and
/// says so rather than pretending.
#[async_trait::async_trait]
pub trait HistoryProvider: Send + Sync {
    /// History for one row of `table` (schema-qualified, e.g. `sapiens.users`).
    ///
    /// `Ok(None)` means **this table is not audited** — deliberately distinct
    /// from `Ok(Some(vec![]))`, which means it is audited and genuinely never
    /// changed. Collapsing the two would answer "nothing ever happened" to a
    /// question that was never actually asked, which is the same defect as a
    /// filtered count that quietly returns the whole table.
    async fn history(
        &self,
        table: &str,
        id: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Option<Vec<HistoryEntry>>, String>;
}

/// Query keys the aggregate endpoint consumes itself.
///
/// They name columns and reductions, so they are grammar rather than
/// predicates — left in the filter map they would be read as filters on
/// columns called `sum` or `group_by`.
const AGGREGATE_QUERY_KEYS: [&str; 7] =
    ["group_by", "sum", "avg", "min", "max", "group_limit", "group_label"];

/// Read an aggregate request off the query string.
///
/// Column names are NOT validated here; they are resolved against the entity's
/// declared columns down in the repository, which is the only layer that knows
/// them. Nothing between here and there splices a caller's string into SQL.
fn aggregate_spec(params: &ListQueryParams) -> backbone_orm::repository::AggregateSpec {
    use backbone_orm::repository::{AggregateFn, AggregateSpec};

    let mut reductions = Vec::new();
    for (key, func) in [
        ("sum", AggregateFn::Sum),
        ("avg", AggregateFn::Avg),
        ("min", AggregateFn::Min),
        ("max", AggregateFn::Max),
    ] {
        if let Some(raw) = params.filters.get(key) {
            for field in raw.split(',').map(str::trim).filter(|f| !f.is_empty()) {
                reductions.push((func, field.to_string()));
            }
        }
    }

    AggregateSpec {
        group_by: params
            .filters
            .get("group_by")
            .map(|g| g.trim().to_string())
            .filter(|g| !g.is_empty()),
        reductions,
        group_limit: params
            .filters
            .get("group_limit")
            .and_then(|l| l.trim().parse::<usize>().ok())
            .unwrap_or(0),
        label_field: params
            .filters
            .get("group_label")
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty()),
        label_relation: None,
    }
}

/// The secret field (`@sensitive`, `@hashed`) a list-style request names as a column,
/// if any: as a filter (`token_hash=…`, `token_hash[startwith]=…`), a sort (`orderby`,
/// `sort`, `sort_by`, `orderby[…]`), a search column (`searchFields`), or an aggregate's
/// group or reduction. Each answers a question about the secret's value — a prefix match
/// recovers a digest one character at a time, and `min=` returns one outright — so the
/// request is refused rather than the column quietly dropped.
fn named_secret(params: &ListQueryParams, secret_fields: &[&str]) -> Option<String> {
    if secret_fields.is_empty() {
        return None;
    }
    let secrets: Vec<String> = secret_fields.iter().map(|f| camel_to_snake_case(f)).collect();
    const COLUMN_LISTS: [&str; 10] = [
        "orderby", "sort", "searchfields", "search_fields", "group_by", "sum", "avg", "min", "max", "group_label",
    ];
    let mut names: Vec<String> = Vec::new();
    for (key, value) in &params.filters {
        match key.find('[') {
            Some(open) => {
                names.push(key[..open].to_string());
                names.push(key[open + 1..].trim_end_matches(']').to_string());
            }
            None => names.push(key.clone()),
        }
        if COLUMN_LISTS.contains(&key.to_ascii_lowercase().as_str()) {
            names.extend(value.split(',').map(|v| v.trim().trim_start_matches('-').to_string()));
        }
    }
    names.extend(params.sort_by.iter().cloned());
    names
        .into_iter()
        .find(|n| secrets.contains(&camel_to_snake_case(n.trim())))
}

/// The answer to a request that names a secret field (see [`named_secret`]).
fn secret_named_message(field: &str) -> String {
    format!("'{field}' is a secret field: it cannot be filtered, sorted, searched or aggregated")
}

/// Reduce a list-style query into the filters the repository should actually see.
///
/// `fields`/`include`/`with` shape the response, not the row set, so they are
/// dropped before they can be mistaken for column predicates, as is every
/// `__`-prefixed key (the framework's internal namespace); `search` and
/// `status` arrive as typed fields and are folded back in under the names the
/// repository expects.
///
/// `/count` answers a question about the same rows `/` returns, so both must
/// normalize identically. They did not — the count route never read the query
/// at all — which is why this lives in one function instead of two copies.
fn repository_filters(params: &ListQueryParams) -> HashMap<String, String> {
    let mut filters = params.filters.clone();
    for key in RESERVED_QUERY_KEYS {
        filters.remove(key);
    }
    // `__`-prefixed keys are the framework's internal namespace (the ORM parser ignores them
    // too): no client key there may reach the repository, whatever the parser does with it.
    filters.retain(|key, _| !backbone_orm::is_internal_key(key));
    if let Some(search) = params.search.clone() {
        filters.insert("search".to_string(), search);
    }
    if let Some(status) = params.status.clone() {
        filters.insert("status".to_string(), status);
    }
    // The typed `sort_by`/`sort_order` pair is the documented sort; it rides into the
    // filter grammar as `orderby=<field>:<dir>`, so the same field sanitizer and allow-list
    // the filters take validate it. A sort the filter grammar already names wins.
    let sorted = filters.keys().any(|k| {
        let base = k.split('[').next().unwrap_or(k);
        base.eq_ignore_ascii_case("orderby") || base.eq_ignore_ascii_case("sort")
    });
    if let Some(field) = params.sort_by.as_deref().map(str::trim).filter(|f| !f.is_empty()) {
        if !sorted {
            let dir = match params.sort_order.as_deref().map(str::trim) {
                Some(d) if d.eq_ignore_ascii_case("desc") => "desc",
                _ => "asc",
            };
            filters.insert("orderby".to_string(), format!("{field}:{dir}"));
        }
    }
    filters
}

/// The filters an aggregate should apply: the same rows `/` would list, with
/// the aggregate's own grammar removed.
fn aggregate_filters(params: &ListQueryParams) -> HashMap<String, String> {
    let mut filters = repository_filters(params);
    for key in AGGREGATE_QUERY_KEYS {
        filters.remove(key);
    }
    filters
}

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

/// Per-request access scope, injected by the application's auth middleware as an axum
/// `Extension`. `Platform` sees everything (a superadmin / root caller); `Company(id)` is
/// scoped to one tenant.
///
/// It governs two boundaries, and they must agree:
/// - **row visibility** — the SQL fence (`EntityRepoMeta::company_field`), which decides
///   which rows exist for this caller at all;
/// - **field visibility** — `@private` fields, gated on the row's `@owner` matching.
///
/// # Deriving it
///
/// There must be exactly ONE answer to "who is the tenant" per request. Build this
/// **from** the authenticated principal — `backbone_auth::company::CompanyContext`, whose
/// `company_id` comes from a signed claim — never populate it independently:
///
/// ```rust,ignore
/// let scope = AccessScope::Company(tenant_ctx.company_id);
/// ```
///
/// Two independently-populated identities can disagree under partial wiring, and the
/// failure is a silent cross-tenant read. `Uuid` rather than `String` so the fence and
/// the writer cannot drift on formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessScope {
    /// Full visibility — platform/root caller. No company fence is applied.
    Platform,
    /// Scoped to a single company (legal entity); only this company's rows are visible.
    Company(uuid::Uuid),
}

impl AccessScope {
    /// The company to fence queries by, or `None` for a platform caller.
    ///
    /// Feed this to the ORM's scoped read paths. Note `None` here means "platform, no
    /// fence" — it is NOT the same as an absent `AccessScope`, which means the request was
    /// never scoped and must be refused for a company-scoped entity.
    pub fn company(&self) -> Option<uuid::Uuid> {
        match self {
            AccessScope::Platform => None,
            AccessScope::Company(id) => Some(*id),
        }
    }
}

/// Enforce field-level security on an already-serialized response object: strip the
/// entity's `@private` fields unless the caller may see them. Visibility rule —
/// `Platform` → all; `Company(id)` → only when the row's `@owner` field equals `id`;
/// absent scope → treated as non-owner (fail-closed). Runs BEFORE sparse projection
/// so the security ceiling always beats a `?fields=` request.
/// Strip the fields that never leave the server (`@sensitive`: password and token
/// digests, bearer tokens, credentials), whoever asks. Unlike private fields there is
/// no reader they are kept for: a platform caller or the row's owner does not get them
/// either.
fn strip_secrets(mut value: serde_json::Value, secret_fields: &[&str]) -> serde_json::Value {
    if let serde_json::Value::Object(map) = &mut value {
        for f in secret_fields {
            map.remove(*f);
        }
    }
    value
}

/// The shape every generic route serves one entity in: its secrets stripped for
/// everyone, then its private fields for anyone but the owner or a platform caller.
/// Write responses pass no scope, so they keep private fields from every caller.
fn secure<E: backbone_orm::EntityRepoMeta>(
    value: serde_json::Value,
    scope: Option<&AccessScope>,
) -> serde_json::Value {
    apply_field_security(
        strip_secrets(value, E::secret_fields()),
        scope,
        E::private_fields(),
        E::owner_field(),
    )
}

/// A response body for a hand-written or generated route outside the generic handler (a
/// state transition, a verb) that returns an entity's response DTO: the entity's secrets
/// stripped, and its private fields kept from every caller, as the generic write responses do.
pub fn without_secrets<E: backbone_orm::EntityRepoMeta, R: Serialize>(response: R) -> serde_json::Value {
    secure::<E>(to_response_value(response), None)
}

/// A related row an `?include=` expands, as the response carries it: camelCase keys,
/// and the related model's secrets stripped. The row is read raw (`row_to_json`), so
/// without this a session's `?include=user` would carry the user's password hash.
fn related_row<E: backbone_orm::EntityRepoMeta>(
    relation: &str,
    table: &str,
    obj: serde_json::Value,
) -> serde_json::Value {
    let row = strip_secrets(camelize_keys(obj), E::relation_secret_fields(relation));
    // A cross-module relation names only a table: strip what that table's entity registered.
    strip_secrets(row, &backbone_orm::secret_registry::secret_fields_of(table))
}

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
        Some(AccessScope::Company(id)) => owner_field
            .and_then(|f| value.get(f))
            .and_then(|v| v.as_str())
            // Parse rather than compare strings: the row's owner key is serialized JSON and
            // the scope is a Uuid, so a formatting difference (case, braces) would silently
            // read as "not the owner" and strip fields from their rightful owner.
            .and_then(|owner| uuid::Uuid::parse_str(owner).ok())
            .is_some_and(|owner| owner == *id),
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

/// Parse the reserved `include` (or `with`) key — comma-separated relation names.
fn include_relations(query: &HashMap<String, String>) -> Vec<String> {
    query
        .get("include")
        .or_else(|| query.get("with"))
        .map(|s| {
            s.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn snake_to_camel(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper = false;
    for c in s.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// Shallow-camelCase an object's top-level keys so an expanded relation (fetched
/// as a raw `row_to_json`) reads consistently with the camelCase response.
fn camelize_keys(v: serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(m) => serde_json::Value::Object(
            m.into_iter().map(|(k, val)| (snake_to_camel(&k), val)).collect(),
        ),
        other => other,
    }
}

/// Expand requested `?include=<rel>` relations into each row's JSON as a sibling
/// object keyed by the relation name. Batched: one fetch per relation, keyed by
/// the rows' foreign-key values. Only relations declared by the entity (via
/// `EntityRepoMeta::relations()`) are honored; unknown names are ignored. Runs
/// after field-security and before sparse projection.
///
/// v1 limitation: the expanded object is the raw related row (camelCased keys),
/// NOT run through the target's response DTO or `@private` field-security — fine
/// while no includable target has private fields. Revisit if that changes.
async fn expand_includes<S, E, C, U>(
    service: &S,
    rows: &mut [serde_json::Value],
    includes: &[String],
) where
    S: CrudService<E, C, U>,
    E: backbone_orm::EntityRepoMeta + Send + Sync + 'static,
    C: Send + Sync + 'static,
    U: Send + Sync + 'static,
{
    if includes.is_empty() || rows.is_empty() {
        return;
    }
    for (rel_name, table, fk_field) in E::relations() {
        if !includes.iter().any(|i| i == rel_name) {
            continue;
        }
        let mut ids: Vec<String> = rows
            .iter()
            .filter_map(|r| r.get(fk_field).and_then(|v| v.as_str()).map(str::to_string))
            .collect();
        ids.sort();
        ids.dedup();
        if ids.is_empty() {
            continue;
        }
        let related = service.fetch_related_json(table, &ids).await;
        let mut by_id: HashMap<String, serde_json::Value> = HashMap::new();
        for obj in related {
            if let Some(id) = obj.get("id").and_then(|v| v.as_str()).map(str::to_string) {
                by_id.insert(id, related_row::<E>(rel_name, table, obj));
            }
        }
        for r in rows.iter_mut() {
            let related_obj = r
                .get(fk_field)
                .and_then(|v| v.as_str())
                .and_then(|id| by_id.get(id).cloned())
                .unwrap_or(serde_json::Value::Null);
            if let serde_json::Value::Object(m) = r {
                m.insert((*rel_name).to_string(), related_obj);
            }
        }
    }
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
        // Aggregate field rejections: the caller named a column this entity
        // does not have, or asked to sum something that is not a number. Both
        // are faults in the request, so they must not read as server errors.
        || m.contains("not a column of this entity")
        || m.contains("needs a numeric column")
}

/// A list request refused before it reached the repository: 400's `invalid_request` code
/// beside the sentence.
fn bad_list_request(message: String) -> PaginatedApiResponse<serde_json::Value> {
    let violation = crate::violation::Violation::new("", "invalid_request", message.clone());
    PaginatedApiResponse::rejected(message, vec![violation])
}

/// Pagination response metadata
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PaginationResponse {
    pub total: u64,
    pub page: u32,
    pub limit: u32,
    pub total_pages: u32,
    /// Keyset paging: pass back as `after=` to fetch the page that follows.
    /// Present only when the repository walked a deterministic order and
    /// more rows follow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Keyset paging: pass back as `before=` to fetch the page that precedes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev_cursor: Option<String>,
    /// Whether another page follows (known without a count on a cursor walk).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
}

impl PaginationResponse {
    pub fn new(total: u64, page: u32, limit: u32) -> Self {
        let total_pages = if limit == 0 { 0 } else { ((total as f64) / (limit as f64)).ceil() as u32 };
        Self { total, page, limit, total_pages, next_cursor: None, prev_cursor: None, has_more: None }
    }

    /// From the repository's pagination info — the cursor fields ride along
    /// when the repository produced them (a cursor walk or a filtered list
    /// with a deterministic order).
    pub fn from_info(info: &backbone_orm::repository::PaginationInfo) -> Self {
        Self {
            total: info.total,
            page: info.page,
            limit: info.per_page,
            total_pages: info.total_pages,
            next_cursor: info.next_cursor.clone(),
            prev_cursor: info.prev_cursor.clone(),
            has_more: info.has_more,
        }
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
    /// The coded reasons for a refused list request, beside `error`; absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub violations: Option<Vec<crate::violation::Violation>>,
}

impl<T> PaginatedApiResponse<T> {
    /// A successful response carrying the repository's pagination info —
    /// the cursor fields surface only when the repository produced them.
    pub fn ok_with_info(data: Vec<T>, info: &backbone_orm::repository::PaginationInfo) -> Self {
        Self {
            success: true,
            data,
            meta: PaginationResponse::from_info(info),
            error: None,
            violations: None,
        }
    }

    /// Create a successful paginated response
    pub fn ok(data: Vec<T>, total: u64, page: u32, limit: u32) -> Self {
        Self {
            success: true,
            data,
            meta: PaginationResponse::new(total, page, limit),
            error: None,
            violations: None,
        }
    }

    /// Create from a PaginatedResponse
    pub fn from_paginated(resp: PaginatedResponse<T>) -> Self {
        Self {
            success: true,
            data: resp.data,
            meta: resp.meta,
            error: None,
            violations: None,
        }
    }

    /// Create an error response
    pub fn error(error: impl Into<String>) -> Self {
        Self {
            success: false,
            data: Vec::new(),
            meta: PaginationResponse::new(0, 0, 0),
            error: Some(error.into()),
            violations: None,
        }
    }

    /// A refused list request: the sentence and its coded reasons.
    pub fn rejected(error: impl Into<String>, violations: Vec<crate::violation::Violation>) -> Self {
        Self {
            violations: (!violations.is_empty()).then_some(violations),
            ..Self::error(error)
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

/// The answer of `POST {resource}/bulk/preview`: what a bulk PATCH would do.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BulkPreview {
    /// Rows the patch would change, each with its patched fields before and after.
    pub will_change: Vec<PreviewChange>,
    /// Rows that already hold the patched values.
    pub unchanged: Vec<String>,
    /// Rows the write would refuse, with why.
    pub refused: Vec<PreviewRefusal>,
    /// Every row named, in one of the three lists.
    pub total: usize,
}

/// A row a bulk PATCH would change.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PreviewChange {
    /// The row's id.
    pub id: String,
    /// Each patched field whose value would move.
    pub changes: Vec<FieldChange>,
}

/// One field's value before and after a previewed write.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FieldChange {
    /// The field, as the response names it.
    pub field: String,
    /// Its value now.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub from: serde_json::Value,
    /// Its value after the write.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub to: serde_json::Value,
}

/// A row a bulk PATCH would refuse.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PreviewRefusal {
    /// The row's id.
    pub id: String,
    /// The first rule it breaks (`refused` when the error names none).
    pub code: String,
    /// The sentence the write would refuse it with.
    pub message: String,
}

/// One dry-run row as the response shows it: `(before, after)`, or `(code, sentence)`.
type PreviewOutcome = Result<(serde_json::Value, serde_json::Value), (String, String)>;

/// Sort a bulk dry run's rows into the three lists of [`BulkPreview`]. Each row is
/// its record before and after as the response shows it (secrets already
/// stripped), or the `(code, sentence)` it would be refused with; `patched` names
/// the fields each id's patch set (snake_case; read under their camelCase name
/// first), and only those are compared, so a value the
/// update hook moves on its own (a timestamp) is not reported as the change.
fn preview_answer(
    rows: Vec<(String, PreviewOutcome)>,
    patched: &HashMap<String, Vec<String>>,
) -> BulkPreview {
    let mut out = BulkPreview::default();
    for (id, outcome) in rows {
        match outcome {
            Ok((before, after)) => {
                let mut keys = patched.get(&id).cloned().unwrap_or_default();
                keys.sort();
                let changes: Vec<FieldChange> = keys
                    .into_iter()
                    .filter_map(|field| {
                        let read = |v: &serde_json::Value| {
                            v.get(snake_to_camel(&field))
                                .or_else(|| v.get(&field))
                                .cloned()
                                .unwrap_or(serde_json::Value::Null)
                        };
                        let (from, to) = (read(&before), read(&after));
                        (from != to).then_some(FieldChange { field, from, to })
                    })
                    .collect();
                if changes.is_empty() {
                    out.unchanged.push(id);
                } else {
                    out.will_change.push(PreviewChange { id, changes });
                }
            }
            Err((code, message)) => out.refused.push(PreviewRefusal { id, code, message }),
        }
    }
    out.total = out.will_change.len() + out.unchanged.len() + out.refused.len();
    out
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

    /// The named rules a refused write broke, when the error carries them, so
    /// the response can list them beside the sentence. `None` by default.
    fn violations_of(err: &Self::Error) -> Option<Vec<crate::violation::Violation>> {
        let _ = err;
        None
    }

    /// The status a failed call answers with, decided by the error's kind rather than by
    /// searching its message. `None` (the default) leaves each route its own fallback: 422 when
    /// [`violations_of`](Self::violations_of) names rules, else the route's usual status.
    /// `GenericCrudService` answers by variant (see [`service_error_status`]).
    fn status_of(err: &Self::Error) -> Option<axum::http::StatusCode> {
        let _ = err;
        None
    }

    /// Whether this service has a real upsert. `true` by default, so a hand-written service
    /// that implements one keeps its route; `GenericCrudService` answers `false`, and the
    /// generic `POST {resource}/upsert` route then answers 501 rather than creating a duplicate.
    fn supports_upsert() -> bool {
        true
    }

    /// Entity name for error messages (e.g., "User", "Role")
    fn entity_name() -> &'static str;

    /// Hydrate `?include=` relations: fetch rows from `table` by id list, as JSON.
    /// Default: no expansion. `GenericCrudService` delegates to its repository.
    async fn fetch_related_json(
        &self,
        _table: &str,
        _ids: &[String],
    ) -> Vec<serde_json::Value> {
        Vec::new()
    }

    /// 1. List entities with pagination and filters
    async fn list(&self, page: u32, limit: u32, filters: HashMap<String, String>) -> Result<(Vec<Entity>, u64), Self::Error>;

    /// `list`, carrying the pagination info (cursor positions included) so
    /// the HTTP layer can surface keyset paging. Default: the tuple form —
    /// generated services override this with the repository's cursors.
    async fn list_with_info(
        &self,
        page: u32,
        limit: u32,
        filters: HashMap<String, String>,
    ) -> Result<(Vec<Entity>, backbone_orm::repository::PaginationInfo), Self::Error> {
        let (rows, total) = self.list(page, limit, filters).await?;
        Ok((
            rows,
            backbone_orm::repository::PaginationInfo::new(page, limit, total),
        ))
    }

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

    /// The schema-qualified table this service reads, when it has one.
    ///
    /// Used to key a record's history. Derived from the mount path instead,
    /// this would silently return an empty history whenever a route segment
    /// and a Postgres schema disagreed — so it is read from the repository,
    /// which is the same string the capture trigger writes.
    fn table_name(&self) -> Option<&str> {
        None
    }

    /// Group and reduce the rows `list` would return, under the same filters.
    ///
    /// The default refuses rather than inventing an answer — see the repository
    /// trait for why a zero-filled default would be worse than an error.
    async fn aggregate(
        &self,
        spec: &backbone_orm::repository::AggregateSpec,
        filters: HashMap<String, String>,
    ) -> Result<backbone_orm::repository::AggregateResult, Self::Error>;

    /// Count active entities MATCHING the caller's filters.
    ///
    /// The generated client has always sent filters to `/count`; the handler
    /// never read them, so a filtered count silently answered with the whole
    /// table. Nothing caught it because the wrong number is a plausible one.
    ///
    /// The default delegates to `list`, whose total is already built from the
    /// same where-clause as its data query — so the filtered count is correct
    /// by construction rather than by a second implementation of the same
    /// filter parsing, which is exactly how the two drifted apart. It asks for
    /// a single row because the rows are discarded; an implementor that can
    /// count without fetching may override.
    async fn count_active_filtered(
        &self,
        filters: HashMap<String, String>,
    ) -> Result<u64, Self::Error> {
        self.list(1, 1, filters).await.map(|(_, total)| total)
    }

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

    /// 23. What [`bulk_partial_update`](Self::bulk_partial_update) would do, with
    /// nothing written: each row as it is and as it would become, or why the
    /// write would refuse it. `Ok(None)` (the default) means this service has no
    /// dry run, and the route says so instead of guessing.
    async fn preview_bulk_partial_update(
        &self,
        items: Vec<(String, HashMap<String, serde_json::Value>)>,
    ) -> Result<Option<Vec<BulkPreviewRow<Entity, Self::Error>>>, Self::Error> {
        let _ = items;
        Ok(None)
    }
}

/// One row of a bulk partial update's dry run: the record as it is and as the
/// patch would leave it, or the error the write would refuse it with.
pub struct BulkPreviewRow<Entity, Err> {
    /// The id the patch named.
    pub id: String,
    /// `(before, after)`, or why the row would be refused.
    pub outcome: Result<(Entity, Entity), Err>,
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
    ///
    /// These routes check no permission: whatever guards the mount is the only barrier. Generated
    /// modules call [`routes_guarded`](Self::routes_guarded) instead, and `schema doctor` flags a
    /// hand-written call of this one.
    pub fn routes(service: Arc<S>, base_path: &str) -> axum::Router<()>
    where
        S: Clone,
    {
        Self::read_routes(service.clone(), base_path)
            .merge(Self::write_routes(service, base_path))
    }

    /// [`routes`](Self::routes) with every route guarded by its permission. This is what the
    /// generated `create_<entity>_routes` calls.
    pub fn routes_guarded(
        service: Arc<S>,
        base_path: &str,
        permissions: crate::permission::CrudPermissions,
    ) -> axum::Router<()>
    where
        S: Clone,
    {
        Self::read_routes_guarded(service.clone(), base_path, permissions.clone())
            .merge(Self::write_routes_guarded(service, base_path, permissions))
    }

    /// Wrap `route` with the permission check `gate` selects, when permissions are in force.
    fn gate(
        perms: &Option<crate::permission::CrudPermissions>,
        gate: crate::permission::Gate,
        route: axum::routing::MethodRouter<()>,
    ) -> axum::routing::MethodRouter<()> {
        match perms {
            // A public gate is left unwrapped: no `AuthContext` is read, so an unauthenticated
            // request is served where the service mounted the route outside its auth layer.
            Some(p) if p.is_public(gate) => route,
            Some(p) => crate::permission::guarded_needs(route, p.needs(gate)),
            None => route,
        }
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
        Self::read_routes_with(service, base_path, None)
    }

    /// [`read_routes`](Self::read_routes), each route refusing a caller who lacks the permission
    /// `permissions` names for it (`<entity>:read` unless the schema says otherwise).
    ///
    /// A request with no `AuthContext` is `401`, a caller whose context lacks the permission `403`
    /// with the violation `permission_required`. Nothing is allowed by default: with no grant
    /// resolver installed the permissions are the ones already on the `AuthContext`.
    pub fn read_routes_guarded(
        service: Arc<S>,
        base_path: &str,
        permissions: crate::permission::CrudPermissions,
    ) -> axum::Router<()>
    where
        S: Clone,
    {
        Self::read_routes_with(service, base_path, Some(permissions))
    }

    fn read_routes_with(
        service: Arc<S>,
        base_path: &str,
        perms: Option<crate::permission::CrudPermissions>,
    ) -> axum::Router<()>
    where
        S: Clone,
    {
        use crate::permission::Gate;
        // Building a generic surface for this entity records its secrets under its table, so
        // a cross-module `?include=` of the table strips them (see `secret_registry`).
        if let Some(table) = service.table_name() {
            backbone_orm::secret_registry::register(table, E::secret_fields());
        }
        use axum::{
            extract::{Path, Query},
            routing::get,
            Extension, Router,
        };

        let handler = Arc::new(Self::new(service));

        Router::new()
            // GET /collection - List
            .route(base_path, Self::gate(&perms, Gate::Read, get({
                let h = handler.clone();
                move |query: Query<ListQueryParams>, access: Option<Extension<AccessScope>>| async move {
                    Self::list_handler(h, query, access).await
                }
            })))
            // GET /collection/trash - List deleted
            .route(&format!("{}/trash", base_path), Self::gate(&perms, Gate::Read, get({
                let h = handler.clone();
                move |query: Query<ListQueryParams>, access: Option<Extension<AccessScope>>| async move {
                    Self::list_deleted_handler(h, query, access).await
                }
            })))
            // GET /collection/:id - Get by ID
            .route(&format!("{}/:id", base_path), Self::gate(&perms, Gate::Read, get({
                let h = handler.clone();
                move |path: Path<String>, query: Query<ListQueryParams>, access: Option<Extension<AccessScope>>| async move {
                    Self::get_handler(h, path, query, access).await
                }
            })))
            // GET /collection/:id/deleted - Get deleted by ID
            .route(&format!("{}/:id/deleted", base_path), Self::gate(&perms, Gate::Read, get({
                let h = handler.clone();
                move |path: Path<String>, query: Query<ListQueryParams>, access: Option<Extension<AccessScope>>| async move {
                    Self::get_deleted_handler(h, path, query, access).await
                }
            })))
            // GET /collection/count - Count active entities
            // GET /collection/:id/history - This record's recorded changes
            .route(&format!("{}/:id/history", base_path), Self::gate(&perms, Gate::Read, get({
                let h = handler.clone();
                move |path: axum::extract::Path<String>,
                      query: axum::extract::Query<ListQueryParams>,
                      provider: Option<axum::Extension<std::sync::Arc<dyn HistoryProvider>>>| async move {
                    Self::history_handler(h, path, query, provider).await
                }
            })))
            // GET /collection/aggregate - Group and reduce
            .route(&format!("{}/aggregate", base_path), Self::gate(&perms, Gate::Read, get({
                let h = handler.clone();
                move |query: axum::extract::Query<ListQueryParams>| async move {
                    Self::aggregate_handler(h, query).await
                }
            })))
            .route(&format!("{}/count", base_path), Self::gate(&perms, Gate::Read, get({
                let h = handler.clone();
                move |query: axum::extract::Query<ListQueryParams>| async move {
                    Self::count_active_handler(h, query).await
                }
            })))
            // GET /collection/trash/count - Count deleted entities
            .route(&format!("{}/trash/count", base_path), Self::gate(&perms, Gate::Read, get({
                let h = handler.clone();
                move || async move {
                    Self::count_deleted_handler(h).await
                }
            })))
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
        Self::write_routes_with(service, base_path, None)
    }

    /// [`write_routes`](Self::write_routes), each route refusing a caller who lacks the permission
    /// `permissions` names for it: `create` for a create, bulk create, `update` for a put, patch
    /// and bulk update, `create` and `update` for an upsert, `delete`, `restore`, and `purge` for
    /// a hard delete or an emptied trash. See [`read_routes_guarded`](Self::read_routes_guarded).
    pub fn write_routes_guarded(
        service: Arc<S>,
        base_path: &str,
        permissions: crate::permission::CrudPermissions,
    ) -> axum::Router<()>
    where
        S: Clone,
    {
        Self::write_routes_with(service, base_path, Some(permissions))
    }

    fn write_routes_with(
        service: Arc<S>,
        base_path: &str,
        perms: Option<crate::permission::CrudPermissions>,
    ) -> axum::Router<()>
    where
        S: Clone,
    {
        use crate::permission::Gate;
        // Building a generic surface for this entity records its secrets under its table, so
        // a cross-module `?include=` of the table strips them (see `secret_registry`).
        if let Some(table) = service.table_name() {
            backbone_orm::secret_registry::register(table, E::secret_fields());
        }
        use axum::{
            extract::Path,
            routing::{delete, patch, post, put},
            Router,
        };

        let handler = Arc::new(Self::new(service));

        Router::new()
            // POST /collection - Create
            .route(base_path, Self::gate(&perms, Gate::Create, post({
                let h = handler.clone();
                move |body: StrictJsonOrForm<C>| async move {
                    Self::create_handler(h, body).await
                }
            })))
            // POST /collection/bulk - Bulk create
            .route(&format!("{}/bulk", base_path), Self::gate(&perms, Gate::Create, post({
                let h = handler.clone();
                move |body: StrictJsonOrForm<Vec<C>>| async move {
                    Self::bulk_create_handler(h, body).await
                }
            })))
            // POST /collection/upsert - Upsert
            .route(&format!("{}/upsert", base_path), Self::gate(&perms, Gate::CreateUpdate, post({
                let h = handler.clone();
                move |body: StrictJsonOrForm<C>| async move {
                    Self::upsert_handler(h, body).await
                }
            })))
            // POST /collection/delete/bulk - Bulk soft delete by ids
            .route(&format!("{}/delete/bulk", base_path), Self::gate(&perms, Gate::Delete, post({
                let h = handler.clone();
                move |body: JsonOrForm<BatchIdsRequest>| async move {
                    Self::bulk_delete_handler(h, body).await
                }
            })))
            // POST /collection/restore/bulk - Bulk restore by ids
            .route(&format!("{}/restore/bulk", base_path), Self::gate(&perms, Gate::Restore, post({
                let h = handler.clone();
                move |body: JsonOrForm<BatchIdsRequest>| async move {
                    Self::bulk_restore_handler(h, body).await
                }
            })))
            // POST /collection/restore/all - Restore all soft-deleted
            .route(&format!("{}/restore/all", base_path), Self::gate(&perms, Gate::Restore, post({
                let h = handler.clone();
                move || async move {
                    Self::restore_all_handler(h).await
                }
            })))
            // DELETE /collection/trash/bulk - Bulk hard delete by ids
            // (registered alongside /trash/:id; matchit prioritises the static segment)
            .route(&format!("{}/trash/bulk", base_path), Self::gate(&perms, Gate::Purge, delete({
                let h = handler.clone();
                move |body: JsonOrForm<BatchIdsRequest>| async move {
                    Self::bulk_permanent_delete_handler(h, body).await
                }
            })))
            // PUT /collection/bulk - Bulk full update
            .route(&format!("{}/bulk", base_path), Self::gate(&perms, Gate::Update, put({
                let h = handler.clone();
                move |body: JsonOrForm<Vec<serde_json::Map<String, serde_json::Value>>>| async move {
                    Self::bulk_update_handler(h, body).await
                }
            })))
            // PATCH /collection/bulk - Bulk partial update (shared or per-id)
            .route(&format!("{}/bulk", base_path), Self::gate(&perms, Gate::Update, patch({
                let h = handler.clone();
                move |body: JsonOrForm<BulkPatchRequest>| async move {
                    Self::bulk_patch_handler(h, body).await
                }
            })))
            // POST /collection/bulk/preview - What the bulk partial update would do, nothing written
            .route(&format!("{}/bulk/preview", base_path), Self::gate(&perms, Gate::Update, post({
                let h = handler.clone();
                move |body: JsonOrForm<BulkPatchRequest>| async move {
                    Self::bulk_patch_preview_handler(h, body).await
                }
            })))
            // DELETE /collection/empty - Empty trash
            .route(&format!("{}/empty", base_path), Self::gate(&perms, Gate::Purge, delete({
                let h = handler.clone();
                move || async move {
                    Self::empty_trash_handler(h).await
                }
            })))
            // DELETE /collection/trash/:id - Permanent delete from trash
            .route(&format!("{}/trash/:id", base_path), Self::gate(&perms, Gate::Purge, delete({
                let h = handler.clone();
                move |path: Path<String>| async move {
                    Self::permanent_delete_handler(h, path).await
                }
            })))
            // PUT /collection/:id - Full update
            .route(&format!("{}/:id", base_path), Self::gate(&perms, Gate::Update, put({
                let h = handler.clone();
                move |path: Path<String>, body: StrictJsonOrForm<U>| async move {
                    Self::update_handler(h, path, body).await
                }
            })))
            // PATCH /collection/:id - Partial update
            .route(&format!("{}/:id", base_path), Self::gate(&perms, Gate::Update, patch({
                let h = handler.clone();
                move |path: Path<String>, body: JsonOrForm<HashMap<String, serde_json::Value>>| async move {
                    Self::partial_update_handler(h, path, body).await
                }
            })))
            // DELETE /collection/:id - Soft delete
            .route(&format!("{}/:id", base_path), Self::gate(&perms, Gate::Delete, delete({
                let h = handler.clone();
                move |path: Path<String>| async move {
                    Self::delete_handler(h, path).await
                }
            })))
            // POST /collection/:id/restore - Restore
            .route(&format!("{}/:id/restore", base_path), Self::gate(&perms, Gate::Restore, post({
                let h = handler.clone();
                move |path: Path<String>| async move {
                    Self::restore_handler(h, path).await
                }
            })))
            // Every write stages its events under the request's correlation id.
            .layer(axum::middleware::from_fn(correlation_middleware))
    }

    // ============================================================
    // Handler Implementations
    // ============================================================

    /// The response for a failed service call: the status the service gives the error's kind
    /// ([`CrudService::status_of`]), else 422 when it names broken rules, else `fallback`. A
    /// server fault answers with a generic sentence; its detail is logged.
    fn failure<T>(err: &S::Error, fallback: axum::http::StatusCode) -> (axum::http::StatusCode, axum::Json<ApiResponse<T>>) {
        let violations = S::violations_of(err);
        let status = S::status_of(err).unwrap_or(if violations.is_some() {
            axum::http::StatusCode::UNPROCESSABLE_ENTITY
        } else {
            fallback
        });
        error_at(status, violations, err)
    }

    /// The response for a failed list-shaped call: a bad filter or sort key is the caller's
    /// fault (400); anything else goes through [`Self::failure`].
    fn list_failure(err: &S::Error) -> (axum::http::StatusCode, axum::Json<PaginatedApiResponse<serde_json::Value>>) {
        use axum::{http::StatusCode, Json};
        let msg = err.to_string();
        if is_bad_query_error(&msg) {
            // The detail is the database's sentence (SQL, column and table names): logged,
            // never sent.
            tracing::debug!(target: "backbone::http", error = %msg, "list query refused");
            let message = "Invalid query parameter or filter";
            return (
                StatusCode::BAD_REQUEST,
                Json(PaginatedApiResponse::rejected(
                    message,
                    vec![crate::violation::Violation::new("", "invalid_query", message)],
                )),
            );
        }
        let (status, Json(body)) = Self::failure::<()>(err, StatusCode::INTERNAL_SERVER_ERROR);
        let error = body.error.unwrap_or_default();
        (status, Json(PaginatedApiResponse::rejected(error, body.violations.unwrap_or_default())))
    }

    async fn list_handler(
        handler: Arc<Self>,
        axum::extract::Query(params): axum::extract::Query<ListQueryParams>,
        access: Option<axum::Extension<AccessScope>>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        let params = params.normalized();
        if let Some(err) = pagination_depth_error(params.page, params.limit) {
            return (StatusCode::BAD_REQUEST, Json(bad_list_request(err)));
        }
        if let Some(field) = named_secret(&params, E::secret_fields()) {
            return (
                StatusCode::BAD_REQUEST,
                Json(bad_list_request(secret_named_message(&field))),
            );
        }

        // Sparse fieldset (`?fields=a,b,c`) is response-shaping, not a filter — read it,
        // then drop the reserved keys so they're never passed to the repository.
        let fields = sparse_fields(&params.filters);
        let includes = include_relations(&params.filters);
        let scope = access.map(|axum::Extension(s)| s);

        let filters = repository_filters(&params);

        match handler
            .service
            .list_with_info(params.page, params.limit, filters)
            .await
        {
            Ok((entities, info)) => {
                // Security ceiling first, then relation expansion (batched across all
                // rows), then sparse projection.
                let mut rows: Vec<serde_json::Value> = entities
                    .into_iter()
                    .map(|e| {
                        secure::<E>(to_response_value(R::from(e)), scope.as_ref())
                    })
                    .collect();
                expand_includes::<S, E, C, U>(&*handler.service, &mut rows, &includes).await;
                let items: Vec<serde_json::Value> =
                    rows.into_iter().map(|r| project_sparse(r, &fields)).collect();
                let response = PaginatedApiResponse::ok_with_info(items, &info);
                (StatusCode::OK, Json(response))
            }
            Err(e) => Self::list_failure(&e),
        }
    }

    async fn create_handler(
        handler: Arc<Self>,
        StrictJsonOrForm(dto): StrictJsonOrForm<C>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        match handler.service.create(dto).await {
            Ok(entity) => {
                let response = secure::<E>(to_response_value(R::from(entity)), None);
                (StatusCode::CREATED, Json(ApiResponse::ok(response)))
            }
            Err(e) => Self::failure(&e, StatusCode::BAD_REQUEST),
        }
    }

    async fn get_handler(
        handler: Arc<Self>,
        axum::extract::Path(id): axum::extract::Path<String>,
        axum::extract::Query(params): axum::extract::Query<ListQueryParams>,
        access: Option<axum::Extension<AccessScope>>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        if let Some(refused) = invalid_id::<serde_json::Value>(&id, "id") {
            return refused;
        }

        let fields = sparse_fields(&params.filters);
        let includes = include_relations(&params.filters);
        let scope = access.map(|axum::Extension(s)| s);

        match handler.service.get_by_id(&id).await {
            Ok(Some(entity)) => {
                let secured = secure::<E>(to_response_value(R::from(entity)), scope.as_ref());
                let mut rows = [secured];
                expand_includes::<S, E, C, U>(&*handler.service, &mut rows, &includes).await;
                let [secured] = rows;
                let value = project_sparse(secured, &fields);
                (StatusCode::OK, Json(ApiResponse::ok(value)))
            }
            Ok(None) => {
                (StatusCode::NOT_FOUND, Json(ApiResponse::<serde_json::Value>::not_found(S::entity_name(), &id)))
            }
            Err(e) => {
                Self::failure(&e, StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }

    async fn update_handler(
        handler: Arc<Self>,
        axum::extract::Path(id): axum::extract::Path<String>,
        StrictJsonOrForm(dto): StrictJsonOrForm<U>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        if let Some(refused) = invalid_id::<serde_json::Value>(&id, "id") {
            return refused;
        }

        match handler.service.update(&id, dto).await {
            Ok(Some(entity)) => {
                let response = secure::<E>(to_response_value(R::from(entity)), None);
                (StatusCode::OK, Json(ApiResponse::ok(response)))
            }
            Ok(None) => {
                (StatusCode::NOT_FOUND, Json(ApiResponse::<serde_json::Value>::not_found(S::entity_name(), &id)))
            }
            Err(e) => {
                Self::failure(&e, StatusCode::BAD_REQUEST)
            }
        }
    }

    async fn partial_update_handler(
        handler: Arc<Self>,
        axum::extract::Path(id): axum::extract::Path<String>,
        JsonOrForm(fields): JsonOrForm<HashMap<String, serde_json::Value>>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        if let Some(refused) = invalid_id::<serde_json::Value>(&id, "id") {
            return refused;
        }

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
                let response = secure::<E>(to_response_value(R::from(entity)), None);
                (StatusCode::OK, Json(ApiResponse::ok(response)))
            }
            Ok(None) => {
                (StatusCode::NOT_FOUND, Json(ApiResponse::<serde_json::Value>::not_found(S::entity_name(), &id)))
            }
            Err(e) => {
                Self::failure(&e, StatusCode::BAD_REQUEST)
            }
        }
    }

    async fn delete_handler(
        handler: Arc<Self>,
        axum::extract::Path(id): axum::extract::Path<String>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        if let Some(refused) = invalid_id::<()>(&id, "id") {
            return refused;
        }

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
                Self::failure(&e, StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }

    async fn bulk_create_handler(
        handler: Arc<Self>,
        StrictJsonOrForm(items): StrictJsonOrForm<Vec<C>>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        if let Some(err) = batch_size_error(items.len()) {
            return (StatusCode::BAD_REQUEST, Json(ApiResponse::<BulkResponse<serde_json::Value>>::error(err)));
        }
        match handler.service.bulk_create(items).await {
            Ok(entities) => {
                let result_items: Vec<serde_json::Value> = entities
                    .into_iter()
                    .map(|e| secure::<E>(to_response_value(R::from(e)), None))
                    .collect();
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
                Self::failure(&e, StatusCode::BAD_REQUEST)
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
        if let Some(refused) = invalid_ids::<serde_json::Value>(req.ids.iter().map(String::as_str), |i| format!("ids/{i}")) {
            return refused;
        }
        match handler.service.bulk_soft_delete(req.ids).await {
            Ok(count) => (StatusCode::OK, Json(ApiResponse::success_with_message(
                serde_json::json!({ "soft_deleted": count }),
                format!("Soft-deleted {count} item(s)"),
            ))),
            Err(e) => Self::failure(&e, StatusCode::BAD_REQUEST),
        }
    }

    async fn bulk_restore_handler(
        handler: Arc<Self>,
        JsonOrForm(req): JsonOrForm<BatchIdsRequest>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        if let Some(err) = batch_size_error(req.ids.len()) {
            return (StatusCode::BAD_REQUEST, Json(ApiResponse::<BulkResponse<serde_json::Value>>::error(err)));
        }
        if let Some(refused) = invalid_ids::<BulkResponse<serde_json::Value>>(req.ids.iter().map(String::as_str), |i| format!("ids/{i}")) {
            return refused;
        }
        match handler.service.bulk_restore(req.ids).await {
            Ok(entities) => {
                let items: Vec<serde_json::Value> = entities
                    .into_iter()
                    .map(|e| secure::<E>(to_response_value(R::from(e)), None))
                    .collect();
                let total = items.len();
                (StatusCode::OK, Json(ApiResponse::ok(BulkResponse { items, total, failed: 0, errors: vec![] })))
            }
            Err(e) => Self::failure(&e, StatusCode::BAD_REQUEST),
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
            Err(e) => Self::failure(&e, StatusCode::INTERNAL_SERVER_ERROR),
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
        if let Some(refused) = invalid_ids::<serde_json::Value>(req.ids.iter().map(String::as_str), |i| format!("ids/{i}")) {
            return refused;
        }
        match handler.service.bulk_permanent_delete(req.ids).await {
            Ok(count) => (StatusCode::OK, Json(ApiResponse::success_with_message(
                serde_json::json!({ "permanently_deleted": count }),
                format!("Permanently deleted {count} item(s)"),
            ))),
            Err(e) => Self::failure(&e, StatusCode::BAD_REQUEST),
        }
    }

    /// `PUT {resource}/bulk`: an array of `{ "id": .., <update fields> }`. Each item's fields
    /// are read as the update DTO with the same unknown-key check a single PUT applies; the
    /// `id` is taken out first, since the DTO itself has none.
    async fn bulk_update_handler(
        handler: Arc<Self>,
        JsonOrForm(raw): JsonOrForm<Vec<serde_json::Map<String, serde_json::Value>>>,
    ) -> axum::response::Response {
        use axum::{http::StatusCode, response::IntoResponse, Json};

        if let Some(err) = batch_size_error(raw.len()) {
            return (StatusCode::BAD_REQUEST, Json(ApiResponse::<BulkResponse<serde_json::Value>>::error(err)))
                .into_response();
        }
        let mut items: Vec<(String, U)> = Vec::with_capacity(raw.len());
        for (index, mut fields) in raw.into_iter().enumerate() {
            let id = match fields.remove("id") {
                Some(serde_json::Value::String(id)) => id,
                _ => {
                    let violation = crate::violation::Violation::new(
                        crate::violation::batch_item_path(index, "id"),
                        "required",
                        "id is required",
                    );
                    return (
                        StatusCode::UNPROCESSABLE_ENTITY,
                        Json(ApiResponse::<()>::rejected(format!("item {index}: id is required"), vec![violation])),
                    )
                        .into_response();
                }
            };
            if let Some(refused) = invalid_id::<()>(&id, &crate::violation::batch_item_path(index, "id")) {
                return refused.into_response();
            }
            let bytes = match serde_json::to_vec(&fields) {
                Ok(bytes) => bytes,
                Err(e) => return error_at::<()>(StatusCode::INTERNAL_SERVER_ERROR, None, &e).into_response(),
            };
            match crate::extractors::decode_json_or_refusal::<U>(&bytes, true) {
                Ok(data) => items.push((id, data)),
                Err(refusal) => return refusal.at_item(index).into_response(),
            }
        }
        match handler.service.bulk_update(items).await {
            Ok(entities) => {
                let items: Vec<serde_json::Value> = entities
                    .into_iter()
                    .map(|e| secure::<E>(to_response_value(R::from(e)), None))
                    .collect();
                let total = items.len();
                (StatusCode::OK, Json(ApiResponse::ok(BulkResponse { items, total, failed: 0, errors: vec![] })))
                    .into_response()
            }
            Err(e) => Self::failure::<()>(&e, StatusCode::BAD_REQUEST).into_response(),
        }
    }

    async fn bulk_patch_handler(
        handler: Arc<Self>,
        JsonOrForm(req): JsonOrForm<BulkPatchRequest>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        let items = req.into_items();
        if let Some(err) = batch_size_error(items.len()) {
            return (StatusCode::BAD_REQUEST, Json(ApiResponse::<BulkResponse<serde_json::Value>>::error(err)));
        }
        if let Some(refused) = invalid_ids::<BulkResponse<serde_json::Value>>(items.iter().map(|(id, _)| id.as_str()), |i| format!("{i}/id")) {
            return refused;
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
                let items: Vec<serde_json::Value> = entities
                    .into_iter()
                    .map(|e| secure::<E>(to_response_value(R::from(e)), None))
                    .collect();
                let total = items.len();
                (StatusCode::OK, Json(ApiResponse::ok(BulkResponse { items, total, failed: 0, errors: vec![] })))
            }
            Err(e) => Self::failure(&e, StatusCode::BAD_REQUEST),
        }
    }

    /// `POST {resource}/bulk/preview`: the bulk PATCH body, answered with what it
    /// would do and nothing written. Each row lands in one of three lists: it
    /// would change (each patched field before and after), it already holds the
    /// values, or the write would refuse it (the code and sentence it would give).
    /// The values pass the same secret and field-security strip as any response.
    async fn bulk_patch_preview_handler(
        handler: Arc<Self>,
        JsonOrForm(req): JsonOrForm<BulkPatchRequest>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        let items = req.into_items();
        if let Some(err) = batch_size_error(items.len()) {
            return (StatusCode::BAD_REQUEST, Json(ApiResponse::<BulkPreview>::error(err)));
        }
        if let Some(refused) = invalid_ids::<BulkPreview>(items.iter().map(|(id, _)| id.as_str()), |i| format!("{i}/id")) {
            return refused;
        }
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
        let patched: HashMap<String, Vec<String>> = items
            .iter()
            .map(|(id, f)| (id.clone(), f.keys().cloned().collect()))
            .collect();
        match handler.service.preview_bulk_partial_update(items).await {
            Ok(Some(rows)) => {
                let rows = rows
                    .into_iter()
                    .map(|row| {
                        let outcome = match row.outcome {
                            Ok((before, after)) => Ok((
                                secure::<E>(to_response_value(R::from(before)), None),
                                secure::<E>(to_response_value(R::from(after)), None),
                            )),
                            Err(e) => Err((
                                S::violations_of(&e)
                                    .and_then(|v| v.into_iter().next().map(|v| v.code))
                                    .unwrap_or_else(|| "refused".to_string()),
                                e.to_string(),
                            )),
                        };
                        (row.id, outcome)
                    })
                    .collect();
                (StatusCode::OK, Json(ApiResponse::ok(preview_answer(rows, &patched))))
            }
            Ok(None) => (
                StatusCode::NOT_IMPLEMENTED,
                Json(ApiResponse::error("this resource has no preview of a bulk change".to_string())),
            ),
            Err(e) => Self::failure(&e, StatusCode::BAD_REQUEST),
        }
    }

    async fn upsert_handler(
        handler: Arc<Self>,
        StrictJsonOrForm(dto): StrictJsonOrForm<C>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        // A create body names no record to match, so a service without a real upsert could
        // only create — a duplicate on every retry. Say so instead.
        if !S::supports_upsert() {
            return (
                StatusCode::NOT_IMPLEMENTED,
                Json(ApiResponse::<serde_json::Value>::error(crate::service::UPSERT_UNSUPPORTED)),
            );
        }
        match handler.service.upsert(dto).await {
            Ok(entity) => {
                let response = secure::<E>(to_response_value(R::from(entity)), None);
                (StatusCode::OK, Json(ApiResponse::ok(response)))
            }
            Err(e) => {
                Self::failure(&e, StatusCode::BAD_REQUEST)
            }
        }
    }

    async fn list_deleted_handler(
        handler: Arc<Self>,
        axum::extract::Query(params): axum::extract::Query<ListQueryParams>,
        access: Option<axum::Extension<AccessScope>>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        let params = params.normalized();
        if let Some(err) = pagination_depth_error(params.page, params.limit) {
            return (StatusCode::BAD_REQUEST, Json(bad_list_request(err)));
        }
        if let Some(field) = named_secret(&params, E::secret_fields()) {
            return (
                StatusCode::BAD_REQUEST,
                Json(bad_list_request(secret_named_message(&field))),
            );
        }

        let fields = sparse_fields(&params.filters);
        let includes = include_relations(&params.filters);
        let scope = access.map(|axum::Extension(s)| s);

        let filters = repository_filters(&params);
        match handler.service.list_deleted_filtered(params.page, params.limit, filters).await {
            Ok((entities, total)) => {
                // Mirror `list_handler`: field-security ceiling, then batched
                // relation expansion (`?include=`), then sparse projection — so the
                // trash view hydrates relations exactly like the active list does.
                let mut rows: Vec<serde_json::Value> = entities
                    .into_iter()
                    .map(|e| {
                        secure::<E>(to_response_value(R::from(e)), scope.as_ref())
                    })
                    .collect();
                expand_includes::<S, E, C, U>(&*handler.service, &mut rows, &includes).await;
                let items: Vec<serde_json::Value> =
                    rows.into_iter().map(|r| project_sparse(r, &fields)).collect();
                let response = PaginatedApiResponse::ok(items, total, params.page, params.limit);
                (StatusCode::OK, Json(response))
            }
            Err(e) => Self::list_failure(&e),
        }
    }

    async fn restore_handler(
        handler: Arc<Self>,
        axum::extract::Path(id): axum::extract::Path<String>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        if let Some(refused) = invalid_id::<serde_json::Value>(&id, "id") {
            return refused;
        }

        match handler.service.restore(&id).await {
            Ok(Some(entity)) => {
                let response = secure::<E>(to_response_value(R::from(entity)), None);
                (StatusCode::OK, Json(ApiResponse::success_with_message(response, "Entity restored successfully")))
            }
            Ok(None) => {
                (StatusCode::NOT_FOUND, Json(ApiResponse::<serde_json::Value>::not_found(S::entity_name(), &id)))
            }
            Err(e) => {
                Self::failure(&e, StatusCode::BAD_REQUEST)
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
                Self::failure(&e, StatusCode::INTERNAL_SERVER_ERROR)
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

        if let Some(refused) = invalid_id::<serde_json::Value>(&id, "id") {
            return refused;
        }

        let fields = sparse_fields(&params.filters);
        let scope = access.map(|axum::Extension(s)| s);

        match handler.service.get_deleted_by_id(&id).await {
            Ok(Some(entity)) => {
                let secured = secure::<E>(to_response_value(R::from(entity)), scope.as_ref());
                let value = project_sparse(secured, &fields);
                (StatusCode::OK, Json(ApiResponse::ok(value)))
            }
            Ok(None) => {
                (StatusCode::NOT_FOUND, Json(ApiResponse::<serde_json::Value>::not_found(&format!("Deleted {}", S::entity_name()), &id)))
            }
            Err(e) => {
                Self::failure(&e, StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }

    /// 13. DELETE /collection/trash/:id - Permanently delete from trash
    async fn permanent_delete_handler(
        handler: Arc<Self>,
        axum::extract::Path(id): axum::extract::Path<String>,
    ) -> axum::response::Response {
        use axum::{http::StatusCode, Json, response::IntoResponse};

        if let Some(refused) = invalid_id::<R>(&id, "id") {
            return refused.into_response();
        }

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
                        (StatusCode::NOT_FOUND, Json(ApiResponse::<R>::not_found(S::entity_name(), &id)))
                            .into_response()
                    }
                    Err(e) => {
                        Self::failure::<R>(&e, StatusCode::INTERNAL_SERVER_ERROR).into_response()
                    }
                }
            }
            Ok(None) => {
                (StatusCode::NOT_FOUND, Json(ApiResponse::<R>::not_found(
                    &format!("{} in trash", S::entity_name()), &id
                ))).into_response()
            }
            Err(e) => {
                Self::failure::<R>(&e, StatusCode::INTERNAL_SERVER_ERROR).into_response()
            }
        }
    }

    /// 15. GET /collection/count - Count active entities
    async fn count_active_handler(
        handler: Arc<Self>,
        axum::extract::Query(params): axum::extract::Query<ListQueryParams>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        // An unfiltered request still answers the whole-table count, which is
        // what every existing caller expects; a filtered one now answers the
        // question it actually asked.
        if let Some(field) = named_secret(&params, E::secret_fields()) {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<serde_json::Value>::error(secret_named_message(&field))),
            );
        }
        match handler.service.count_active_filtered(repository_filters(&params)).await {
            Ok(count) => {
                (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({ "count": count }))))
            }
            Err(e) => {
                // Now that the filters are read, a malformed one is the
                // caller's fault — the same classification the list path uses.
                if is_bad_query_error(&e.to_string()) {
                    (StatusCode::BAD_REQUEST, Json(ApiResponse::<serde_json::Value>::error(e.to_string())))
                } else {
                    Self::failure(&e, StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        }
    }

    /// GET /collection/aggregate - Group and reduce
    ///
    /// Answers with one entry per distinct group plus the overall total, so a
    /// caller can draw a chart and its headline figure from a single reply.
    async fn aggregate_handler(
        handler: Arc<Self>,
        axum::extract::Query(params): axum::extract::Query<ListQueryParams>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        if let Some(field) = named_secret(&params, E::secret_fields()) {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<serde_json::Value>::error(secret_named_message(&field))),
            );
        }
        let spec = aggregate_spec(&params);
        match handler.service.aggregate(&spec, aggregate_filters(&params)).await {
            Ok(result) => {
                let render = |g: &backbone_orm::repository::AggregateGroup| {
                    // Flat `"sum:amount"` keys become nested `sum: { amount }`,
                    // so a caller reads `groups[i].sum.amount` without having to
                    // reassemble a compound key.
                    let mut out = serde_json::Map::new();
                    out.insert("key".into(), match &g.key {
                        Some(k) => serde_json::Value::String(k.clone()),
                        None => serde_json::Value::Null,
                    });
                    if g.label.is_some() {
                        out.insert(
                            "label".into(),
                            serde_json::Value::String(g.label.clone().unwrap()),
                        );
                    }
                    out.insert("count".into(), serde_json::json!(g.count));
                    for (compound, value) in &g.values {
                        let Some((func, field)) = compound.split_once(':') else { continue };
                        let slot = out
                            .entry(func.to_string())
                            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
                        if let Some(obj) = slot.as_object_mut() {
                            obj.insert(field.to_string(), match value {
                                // Strings, not numbers: Postgres `numeric` carries
                                // more precision than a JSON double, and money
                                // columns are exactly where rounding would show.
                                Some(v) => serde_json::Value::String(v.clone()),
                                None => serde_json::Value::Null,
                            });
                        }
                    }
                    serde_json::Value::Object(out)
                };

                let body = serde_json::json!({
                    "groups": result.groups.iter().map(render).collect::<Vec<_>>(),
                    "total": render(&result.total),
                    // Says plainly that groups were dropped, so a partial chart
                    // is never mistaken for a complete one.
                    "truncated": result.truncated,
                });
                (StatusCode::OK, Json(ApiResponse::ok(body)))
            }
            Err(e) => {
                if is_bad_query_error(&e.to_string()) {
                    (StatusCode::BAD_REQUEST, Json(ApiResponse::<serde_json::Value>::error(e.to_string())))
                } else {
                    Self::failure(&e, StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        }
    }

    /// GET /collection/:id/history - what has been recorded about this record
    ///
    /// Three outcomes, kept deliberately distinct, because collapsing any two
    /// of them would render as "nothing ever happened to this record":
    ///
    /// - no provider installed -> 501, this service serves no history at all
    /// - provider says the table is not audited -> 200, `audited: false`,
    ///   `entries: null`
    /// - audited -> 200, `audited: true`, `entries: [...]` (possibly empty,
    ///   which here genuinely means nothing has changed)
    async fn history_handler(
        handler: Arc<Self>,
        axum::extract::Path(id): axum::extract::Path<String>,
        axum::extract::Query(params): axum::extract::Query<ListQueryParams>,
        provider: Option<axum::Extension<std::sync::Arc<dyn HistoryProvider>>>,
    ) -> impl axum::response::IntoResponse {
        use axum::{http::StatusCode, Json};

        if let Some(refused) = invalid_id::<serde_json::Value>(&id, "id") {
            return refused;
        }

        let Some(axum::Extension(provider)) = provider else {
            return (
                StatusCode::NOT_IMPLEMENTED,
                Json(ApiResponse::<serde_json::Value>::error(
                    "history is not configured for this service".to_string(),
                )),
            );
        };

        // Keyed on the table the repository actually reads — the same string the
        // capture trigger writes as `subject_type`. A service that cannot name
        // its table has no history rather than a wrong one.
        let Some(table) = handler.service.table_name().map(|t| t.to_string()) else {
            return (
                StatusCode::NOT_IMPLEMENTED,
                Json(ApiResponse::<serde_json::Value>::error(
                    "this entity cannot name its table, so its history cannot be keyed".to_string(),
                )),
            );
        };

        let limit = params.limit.clamp(1, 200);
        let offset = params.page.saturating_sub(1) * limit;

        match provider.history(&table, &id, limit, offset).await {
            Ok(Some(entries)) => (
                StatusCode::OK,
                Json(ApiResponse::ok(serde_json::json!({
                    "audited": true,
                    "entries": entries,
                }))),
            ),
            Ok(None) => (
                StatusCode::OK,
                Json(ApiResponse::ok(serde_json::json!({
                    // Not a mistake and not an empty timeline: this table
                    // records nothing, so there is nothing to have missed.
                    "audited": false,
                    "entries": serde_json::Value::Null,
                }))),
            ),
            Err(e) => error_at(StatusCode::INTERNAL_SERVER_ERROR, None, &e),
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
                Self::failure(&e, StatusCode::INTERNAL_SERVER_ERROR)
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
    fn a_bulk_preview_sorts_rows_by_what_the_patch_would_do() {
        use serde_json::json;
        let mut patched: HashMap<String, Vec<String>> = ["a", "b", "c"]
            .iter()
            .map(|id| (id.to_string(), vec!["category".to_string()]))
            .collect();
        patched.insert("d".into(), vec!["category_id".into()]);
        let out = preview_answer(
            vec![
                (
                    "a".into(),
                    Ok((
                        json!({"category": "meal", "updated_at": "1"}),
                        json!({"category": "travel", "updated_at": "2"}),
                    )),
                ),
                ("b".into(), Ok((json!({"category": "travel"}), json!({"category": "travel"})))),
                (
                    "d".into(),
                    Ok((json!({"categoryId": "x"}), json!({"categoryId": "y"}))),
                ),
                ("c".into(), Err(("locked".into(), "a posted claim is locked".into()))),
            ],
            &patched,
        );
        assert_eq!(out.total, 4);
        assert_eq!(out.will_change.len(), 2);
        assert_eq!(out.will_change[1].changes[0].field, "category_id", "a camelCase response is read");
        let ch = &out.will_change[0].changes;
        assert_eq!(ch.len(), 1, "only the patched field is reported, not the timestamp");
        assert_eq!((ch[0].from.clone(), ch[0].to.clone()), (json!("meal"), json!("travel")));
        assert_eq!(out.unchanged, vec!["b".to_string()]);
        assert_eq!(out.refused[0].code, "locked");
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

    /// The row's owner. A real tenant id is a Uuid — the scope carries one so the fence and
    /// field security cannot drift on formatting.
    fn owner_a() -> uuid::Uuid {
        uuid::Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap()
    }
    fn owner_b() -> uuid::Uuid {
        uuid::Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap()
    }

    fn row() -> serde_json::Value {
        serde_json::json!({ "id": "1", "providerId": owner_a().to_string(), "name": "n", "hppPerUnit": 5 })
    }
    const PRIV: &[&str] = &["hppPerUnit"];

    /// An entity with a secret, a private field and a relation to a model with a secret.
    struct Secretive;
    impl backbone_orm::EntityRepoMeta for Secretive {
        fn column_types() -> HashMap<String, String> {
            HashMap::new()
        }
        fn search_fields() -> &'static [&'static str] {
            &[]
        }
        fn secret_fields() -> &'static [&'static str] {
            &["tokenHash"]
        }
        fn private_fields() -> &'static [&'static str] {
            PRIV
        }
        fn owner_field() -> Option<&'static str> {
            Some("providerId")
        }
        fn relation_secret_fields(relation: &str) -> &'static [&'static str] {
            match relation {
                "user" => &["passwordHash"],
                _ => &[],
            }
        }
    }

    fn secret_row() -> serde_json::Value {
        let mut r = row();
        r["tokenHash"] = serde_json::json!("digest");
        r
    }

    #[test]
    fn a_secret_is_served_to_no_caller_not_even_platform_or_the_owner() {
        for scope in [None, Some(AccessScope::Platform), Some(AccessScope::Company(owner_a()))] {
            let out = secure::<Secretive>(secret_row(), scope.as_ref());
            assert!(out.get("tokenHash").is_none(), "{scope:?} was served the secret");
            assert!(out.get("name").is_some());
        }
        let platform = secure::<Secretive>(secret_row(), Some(&AccessScope::Platform));
        assert!(platform.get("hppPerUnit").is_some(), "private fields keep their owner/platform rule");
    }

    #[test]
    fn the_typed_sort_pair_reaches_the_repository_as_the_filter_sort() {
        let params = |pairs: &[(&str, &str)], by: Option<&str>, order: Option<&str>| ListQueryParams {
            filters: pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            sort_by: by.map(str::to_string),
            sort_order: order.map(str::to_string),
            ..Default::default()
        };
        let f = repository_filters(&params(&[], Some("priority"), Some("DESC")));
        assert_eq!(f.get("orderby").map(String::as_str), Some("priority:desc"));
        let f = repository_filters(&params(&[], Some("createdAt"), None));
        assert_eq!(f.get("orderby").map(String::as_str), Some("createdAt:asc"));
        // A sort the filter grammar names wins over the typed pair.
        let f = repository_filters(&params(&[("orderby[name]", "asc")], Some("priority"), Some("desc")));
        assert!(!f.contains_key("orderby"), "{f:?}");
        // It goes through the same sanitizer as any filter sort: an injection is dropped there.
        let parsed = backbone_orm::filter::parse_filters(
            &repository_filters(&params(&[], Some("name; drop table x"), None)),
            &HashMap::new(),
            None,
        )
        .unwrap();
        assert!(parsed.sorts.is_empty(), "{:?}", parsed.sorts);
    }

    #[test]
    fn list_params_normalize_page_zero_and_an_oversized_limit() {
        let p = ListQueryParams { page: 0, limit: 1000, ..Default::default() }.normalized();
        assert_eq!((p.page, p.limit), (1, MAX_PER_PAGE));
        let p = ListQueryParams { page: 3, limit: 0, ..Default::default() }.normalized();
        assert_eq!((p.page, p.limit), (3, 1));
    }

    #[test]
    fn a_request_that_names_a_secret_as_a_column_is_caught_in_every_grammar() {
        let ask = |pairs: &[(&str, &str)], sort_by: Option<&str>| {
            let params = ListQueryParams {
                filters: pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
                sort_by: sort_by.map(str::to_string),
                ..Default::default()
            };
            named_secret(&params, <Secretive as backbone_orm::EntityRepoMeta>::secret_fields())
        };
        for pairs in [
            vec![("token_hash", "abc")],
            vec![("tokenHash[startwith]", "a")],
            vec![("orderby", "name,-token_hash")],
            vec![("orderby[token_hash]", "asc")],
            vec![("searchFields", "name,tokenHash")],
            vec![("min", "token_hash")],
            vec![("group_by", "tokenHash")],
        ] {
            assert_eq!(ask(&pairs, None).as_deref().map(camel_to_snake_case), Some("token_hash".into()), "{pairs:?}");
        }
        assert!(ask(&[], Some("tokenHash")).is_some(), "sort_by");
        assert!(ask(&[("name[contain]", "token_hash"), ("orderby", "name")], None).is_none(), "a value is not a column");
    }

    #[test]
    fn a_write_response_keeps_secrets_and_private_fields_from_every_caller() {
        let out = secure::<Secretive>(secret_row(), None);
        assert!(out.get("tokenHash").is_none() && out.get("hppPerUnit").is_none());
    }

    #[test]
    fn a_routed_response_dto_loses_the_entitys_secrets() {
        let out = without_secrets::<Secretive, _>(secret_row());
        assert!(out.get("tokenHash").is_none() && out.get("hppPerUnit").is_none());
        assert!(out.get("name").is_some());
    }

    #[test]
    fn an_included_row_loses_the_related_models_secrets() {
        let raw = serde_json::json!({ "id": "u1", "email": "a@b.c", "password_hash": "$argon2id$..." });
        let out = related_row::<Secretive>("user", "users", raw.clone());
        assert!(out.get("passwordHash").is_none(), "{out}");
        assert_eq!(out["email"], "a@b.c");
        assert!(related_row::<Secretive>("other", "others", raw.clone()).get("passwordHash").is_some(), "only the named relation");
        // A cross-module relation: the table's entity registered its secrets.
        backbone_orm::secret_registry::register("employee_probe.staff", &["passwordHash"]);
        assert!(related_row::<Secretive>("staff", "employee_probe.staff", raw).get("passwordHash").is_none(), "registered table");
    }

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
            Some(&AccessScope::Company(owner_a())),
            PRIV,
            Some("providerId"),
        );
        assert!(out.get("hppPerUnit").is_some());
    }

    #[test]
    fn security_other_tenant_stripped() {
        let out = apply_field_security(
            row(),
            Some(&AccessScope::Company(owner_b())),
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

    // ── Relation expansion (?include=) helpers ──

    #[test]
    fn include_relations_parses_include_and_with() {
        let mut q = HashMap::new();
        q.insert("include".to_string(), "provider, outlet ".to_string());
        assert_eq!(include_relations(&q), vec!["provider", "outlet"]);
        let mut q2 = HashMap::new();
        q2.insert("with".to_string(), "category".to_string());
        assert_eq!(include_relations(&q2), vec!["category"]);
        assert!(include_relations(&HashMap::new()).is_empty());
    }

    #[test]
    fn snake_to_camel_converts() {
        assert_eq!(snake_to_camel("provider_id"), "providerId");
        assert_eq!(snake_to_camel("business_name"), "businessName");
        assert_eq!(snake_to_camel("id"), "id");
    }

    #[test]
    fn camelize_keys_top_level_only() {
        let v = serde_json::json!({ "provider_id": "1", "meta_data": { "created_at": "x" } });
        let out = camelize_keys(v);
        assert!(out.get("providerId").is_some());
        // nested keys are left untouched (per-struct camelCase, shallow).
        assert!(out.get("metaData").unwrap().get("created_at").is_some());
    }

    #[test]
    fn security_null_owner_only_platform_sees() {
        let v = serde_json::json!({ "id": "1", "providerId": null, "hppPerUnit": 5 });
        let tenant = apply_field_security(v.clone(), Some(&AccessScope::Company(owner_a())), PRIV, Some("providerId"));
        assert!(tenant.get("hppPerUnit").is_none());
        let plat = apply_field_security(v, Some(&AccessScope::Platform), PRIV, Some("providerId"));
        assert!(plat.get("hppPerUnit").is_some());
    }

    #[test]
    fn a_write_refused_for_named_reasons_answers_422_with_its_violations() {
        let v = vec![crate::violation::Violation::new("status", "field_not_writable", "`status` changes only through its verbs")];
        let (status, axum::Json(body)) =
            write_error::<()>(Some(v), &"validation failed: `status` changes only through its verbs", axum::http::StatusCode::BAD_REQUEST);
        assert_eq!(status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["error"], "validation failed: `status` changes only through its verbs");
        assert_eq!(json["violations"][0]["path"], "status");
        assert_eq!(json["violations"][0]["code"], "field_not_writable");

        // Any other client failure keeps its status and sentence, coded by its status.
        let (status, axum::Json(body)) = write_error::<()>(None, &"boom", axum::http::StatusCode::BAD_REQUEST);
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["error"], "boom");
        assert_eq!(json["violations"][0]["code"], "invalid_request");
        assert_eq!(json["violations"][0]["path"], "");
        // A server fault stays uncoded and generic.
        let (_, axum::Json(body)) = write_error::<()>(None, &"boom", axum::http::StatusCode::INTERNAL_SERVER_ERROR);
        assert!(serde_json::to_value(&body).unwrap().get("violations").is_none());
    }
}

#[cfg(test)]
mod internal_key_tests {
    use super::*;

    fn params_with(filters: &[(&str, &str)]) -> ListQueryParams {
        ListQueryParams {
            page: 1,
            limit: 20,
            sort_by: None,
            sort_order: None,
            search: None,
            status: None,
            filters: filters
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    #[test]
    fn internal_keys_never_reach_the_repository() {
        let params = params_with(&[
            ("__base_condition", "EXISTS(SELECT 1)"),
            ("__base_condition[eq]", "1=1"),
            ("__anything", "x"),
            ("name", "acme"),
        ]);
        let filters = repository_filters(&params);
        assert_eq!(filters.len(), 1, "{filters:?}");
        assert_eq!(filters.get("name").map(String::as_str), Some("acme"));
        let filters = aggregate_filters(&params);
        assert!(filters.keys().all(|k| !k.starts_with("__")), "{filters:?}");
    }
}

#[cfg(test)]
mod service_error_response_tests {
    use super::*;
    use crate::service::ServiceError;

    #[test]
    fn a_refused_field_is_422_with_its_violation_never_500() {
        let err = ServiceError::Violations(vec![crate::violation::Violation::new(
            "state",
            "field_not_writable",
            "state cannot be changed by a generic write",
        )]);
        let (status, body) = service_error_response::<()>(&err);
        assert_eq!(status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body.0.violations.as_ref().map(Vec::len), Some(1));
    }

    #[test]
    fn other_errors_keep_their_own_status() {
        use axum::http::StatusCode;
        let status = |e: ServiceError| service_error_response::<()>(&e).0;
        assert_eq!(
            status(ServiceError::Validation("bad".into())),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(status(ServiceError::NotFound), StatusCode::NOT_FOUND);
        assert_eq!(
            status(ServiceError::AlreadyExists("x".into())),
            StatusCode::CONFLICT
        );
        assert_eq!(
            status(ServiceError::Internal("boom".into())),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
