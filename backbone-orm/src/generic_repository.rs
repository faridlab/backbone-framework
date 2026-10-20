//! `GenericCrudRepository<T, DeleteMode>` — one implementation for all entities.
//!
//! The generated repository file previously duplicated ~20 identical method bodies
//! for every entity.  This module provides those implementations once so that
//! the generator can emit a thin newtype wrapper instead of hundreds of lines of
//! copy-paste.
//!
//! # Delete mode markers
//!
//! | Marker | Behaviour |
//! |--------|-----------|
//! | [`SoftDelete`] | `deleted_at` stored in `metadata` JSONB; active rows filter `IS NULL` |
//! | [`HardDelete`] | Rows are permanently deleted; no trash concept |
//!
//! # Generated output (after this change)
//!
//! ```rust,ignore
//! // ~20 lines total instead of ~460-560
//! pub struct UserRepository(
//!     backbone_orm::GenericCrudRepository<User, backbone_orm::SoftDelete>
//! );
//! impl std::ops::Deref for UserRepository {
//!     type Target = backbone_orm::GenericCrudRepository<User, backbone_orm::SoftDelete>;
//!     fn deref(&self) -> &Self::Target { &self.0 }
//! }
//! impl UserRepository {
//!     pub fn new(pool: sqlx::PgPool) -> Self {
//!         Self(backbone_orm::GenericCrudRepository::new(pool, "users"))
//!     }
//!     // entity-specific methods only:
//!     pub async fn find_by_email(&self, email: &str) -> anyhow::Result<Option<User>> { … }
//!     pub async fn partial_update(…) { … }
//!     pub async fn list_paginated_filtered(…) { … }
//! }
//! backbone_core::impl_crud_repository!(UserRepository, User, soft_delete);
//! ```

use std::collections::HashMap;
use std::marker::PhantomData;

use anyhow::Result;
use serde::Serialize;
use sqlx::postgres::PgRow;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::repository::{
    DatabaseOperations, PaginatedResult, PaginationInfo, PaginationParams, PostgresRepository,
};

// ─── EntityRepoMeta trait ─────────────────────────────────────────────────────

/// Metadata trait that entities implement so `GenericCrudRepository` can provide
/// `list_paginated_filtered` and `list_deleted_filtered` without per-entity boilerplate.
///
/// Generated once per entity by the `rust` generator.
///
/// | Method | Purpose |
/// |--------|---------|
/// | `column_types` | PostgreSQL cast hints (e.g. `{"id": "uuid"}`) |
/// | `search_fields` | Text columns searched with `ILIKE` |
pub trait EntityRepoMeta {
    /// PostgreSQL type hints for filter/sort (e.g. `{"id": "uuid"}`).
    fn column_types() -> HashMap<String, String>;
    /// Text columns to include in full-text / ILIKE search.
    fn search_fields() -> &'static [&'static str];

    // ── Field-level security (response shaping) ──
    // Names below are RESPONSE JSON keys (camelCase), not DB columns — they are
    // matched against the serialized response object. Defaults = no restriction.

    /// Response fields visible only to the row owner and platform/root callers
    /// (`@private`). Stripped for everyone else.
    fn private_fields() -> &'static [&'static str] {
        &[]
    }

    /// Response fields (camelCase) that never leave the server (`@sensitive`): no
    /// caller is served them, a platform caller or the row's owner included.
    fn secret_fields() -> &'static [&'static str] {
        &[]
    }

    /// The `@sensitive` fields (camelCase response keys) of the model a to-one
    /// relation points at, stripped from the related row an `?include=` expands.
    /// Keyed by the relation name of [`relations`](Self::relations). Default: none.
    fn relation_secret_fields(_relation: &str) -> &'static [&'static str] {
        &[]
    }

    /// Fields (camelCase response keys) a caller may see and set only while holding a
    /// permission, as `(field, permission)`. The serialization seam redacts the value to `null`
    /// (the key stays, so response shapes are stable) for a caller whose resolved grants do not
    /// hold the permission, list-shaped requests may not filter, sort, search or aggregate over
    /// one without it, and a write that sets one is refused naming the permission. `*:*` (and
    /// `field_resource:*` / `*:action` wildcards) hold everything, as any other permission.
    /// Default: no field is gated. The schema generator fills it from `requires:` on fields.
    fn field_permissions() -> &'static [(&'static str, &'static str)] {
        &[]
    }

    /// The response field (camelCase) holding the owner/company id (`@owner`),
    /// compared against the caller's access scope to decide private-field
    /// visibility. `None` = the entity has no owner concept.
    fn owner_field() -> Option<&'static str> {
        None
    }

    // ── Row-level company isolation ──

    /// The DB column that scopes rows to a company (snake_case, e.g. `company_id`).
    /// `None` = the entity is global (reference data) and is not fenced.
    ///
    /// The generator emits this **structurally** — from the presence of the company
    /// column, a fact it cannot forget — and an entity opts OUT with `@global`.
    /// The polarity is deliberate: `owner_field()` above is opt-IN via `@owner`, and
    /// in a year of generated entities not one model ever declared it, so the whole
    /// field-security path silently never ran. Security that must be remembered, in a
    /// file nobody edits for security reasons, defaults to open and stays open. Here
    /// silence means fenced, and forgetting is the safe direction.
    ///
    /// This is a *row-visibility fence*, the same category as `SoftDelete`'s
    /// unconditional `deleted_at IS NULL` — not domain logic.
    fn company_field() -> Option<&'static str> {
        None
    }

    /// To-one relations expandable via `?include=<name>`, as
    /// `(relation_name, target_table, local_fk_field)` — all response keys
    /// (relation_name/local_fk in camelCase; target_table is the DB table).
    /// Default: none.
    fn relations() -> &'static [(&'static str, &'static str, &'static str)] {
        &[]
    }
}

// ─── Delete-mode markers ──────────────────────────────────────────────────────

// ─── Row-level company fence ───────────────────────────────────────────────────

/// The caller could not be scoped to a company for an entity that requires one.
///
/// Deliberately an error and not an empty result: an empty list is a silent lie that
/// looks identical to "no data" and gets debugged for days, whereas this names the
/// wiring bug. Callers should surface it as 403.
#[derive(Debug, thiserror::Error)]
#[error("entity is company-scoped ({column}) but the request carries no company scope")]
pub struct MissingCompanyScope {
    /// The column that would have fenced the query.
    pub column: &'static str,
}

/// Build the SQL fence for `T`, or fail closed.
///
/// - entity is global (`company_field() == None`) → no fence, reference data reads freely
/// - entity is fenced but the request has no company → **`Err`**, never an unfenced query
/// - both present → `col = '<uuid>'`, ANDed into the WHERE by the caller
///
/// The literal is safe to interpolate: `column` is generator-emitted (never client
/// input) and `Uuid`'s `Display` can only produce hex and dashes, so neither side can
/// carry a quote. A base condition is code-authored SQL with no parameter slot, the
/// same seam the soft-delete guard uses.
pub fn company_fence<T: EntityRepoMeta>(company: Option<Uuid>) -> Result<Option<String>, MissingCompanyScope> {
    match (T::company_field(), company) {
        (None, _) => Ok(None),
        (Some(column), None) => Err(MissingCompanyScope { column }),
        (Some(column), Some(id)) => Ok(Some(format!("{column} = '{id}'"))),
    }
}

/// Remove any caller-supplied filter on the company column.
///
/// The HTTP filter map is `#[serde(flatten)]`, so `?company_id=<victim>` arrives as an
/// ordinary filter and the builder honours it. The fence is ANDed, so a client cannot
/// widen past its own company — but leaving the key in lets a caller AND a *second*,
/// contradictory company predicate and turn every list into an empty-vs-nonempty oracle
/// for "does company X have rows matching Y". Strip it: a client may narrow **within**
/// its company, never speak about the company at all.
///
/// Handles the snake_case column, its camelCase response key, and both operator forms
/// (`company_id[eq]=…`).
pub fn strip_client_company_filters<T: EntityRepoMeta>(filters: &mut HashMap<String, String>) {
    let Some(column) = T::company_field() else {
        return;
    };
    let camel = snake_to_camel(column);
    filters.retain(|key, _| {
        let base = key.split('[').next().unwrap_or(key);
        !base.eq_ignore_ascii_case(column) && !base.eq_ignore_ascii_case(&camel)
    });
}

/// AND two optional SQL conditions into the single base-condition argument the filtered
/// queries take, so the soft-delete guard and the company fence travel as one string.
pub fn and_conditions(a: Option<&str>, b: Option<String>) -> Option<String> {
    match (a, b) {
        (None, None) => None,
        (Some(a), None) => Some(a.to_string()),
        (None, Some(b)) => Some(b),
        (Some(a), Some(b)) => Some(format!("{a} AND {b}")),
    }
}

fn snake_to_camel(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper = false;
    for c in s.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            out.push(c.to_ascii_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// The not-deleted predicate an update of a soft-delete entity carries, against the alias `t`.
const ACTIVE_ROW_GUARD: &str = " AND t.metadata->>'deleted_at' IS NULL";

/// Marker: entity uses JSONB-based soft delete (`metadata->>'deleted_at'`).
///
/// Active rows satisfy `metadata->>'deleted_at' IS NULL`.
pub struct SoftDelete;

/// Marker: entity uses hard deletes only — no soft delete / trash concept.
pub struct HardDelete;

// ─── GenericCrudRepository ────────────────────────────────────────────────────

/// Generic PostgreSQL CRUD repository parameterised by entity type `T` and
/// delete strategy `D` ([`SoftDelete`] or [`HardDelete`]).
///
/// All standard CRUD methods are implemented here once.  Generated repository
/// structs wrap this type via newtype + `Deref` and only add entity-specific
/// methods (`find_by_*`, `exists_by_*`, `partial_update`, filtered listings).
pub struct GenericCrudRepository<T, D = SoftDelete>
where
    T: for<'r> FromRow<'r, PgRow> + Send + Unpin,
{
    inner: PostgresRepository<T>,
    _mode: PhantomData<D>,
}

// ─── Common (both modes) ──────────────────────────────────────────────────────

impl<T, D> GenericCrudRepository<T, D>
where
    T: for<'r> FromRow<'r, PgRow> + Send + Unpin,
{
    pub fn new(pool: PgPool, table_name: &str) -> Self {
        Self {
            inner: PostgresRepository::new(pool, table_name),
            _mode: PhantomData,
        }
    }

    pub fn pool(&self) -> &PgPool {
        self.inner.pool()
    }

    pub fn table_name(&self) -> &str {
        self.inner.table_name()
    }

    /// Insert a new entity row and return the created row.
    pub async fn create(&self, entity: &T) -> Result<T>
    where
        T: Serialize + Send + Sync,
    {
        self.inner.create(entity).await
    }

    /// Insert multiple entity rows inside a single transaction: all of them, or none.
    ///
    /// The first row the database refuses rolls the whole batch back, and the error is a
    /// [`BatchItemError`] naming that row's position in `entities`. The transaction runs on the
    /// request's own connection when a request scope holds one, so it carries the same fence
    /// (company, org units, audit attribution) as a single create.
    pub async fn bulk_create(&self, entities: &[T]) -> Result<Vec<T>>
    where
        T: Serialize + Send + Sync,
    {
        if entities.is_empty() {
            return Ok(Vec::new());
        }
        let table = self.table_name();
        let unit = crate::write_tx::begin_unit(self.pool()).await?;
        let rows = async {
            let mut conn = unit.conn().await;
            insert_each(&mut conn, table, entities).await
        }
        .await;
        unit.finish(rows).await
    }

    /// Update many rows from `(before, after)` pairs inside one transaction, writing for each
    /// only the columns that differ — the batch twin of a single changed-columns update — and
    /// only while `guard` holds (see [`crate::repository::PostgresRepository::update_guarded`]).
    ///
    /// A row that no longer matches rolls the whole batch back with a [`BatchItemError`]
    /// naming its position.
    async fn bulk_update_changed_guarded(&self, pairs: &[(T, T)], guard: &str) -> Result<Vec<T>>
    where
        T: Serialize + Send + Sync,
    {
        if pairs.is_empty() {
            return Ok(Vec::new());
        }
        let table = self.table_name();
        let unit = crate::write_tx::begin_unit(self.pool()).await?;
        let rows = async {
            let mut conn = unit.conn().await;
            update_each_changed(&mut conn, table, guard, pairs).await
        }
        .await;
        unit.finish(rows).await
    }

    // ── Generic unique-field lookups ──────────────────────────────────────────
    //
    // These are shared across both delete modes. The SQL is the same for both;
    // the soft-delete guard is added separately by the per-mode wrappers below.

    /// Internal: query by a text field with a caller-supplied extra condition.
    async fn find_by_text_field_with_cond(
        &self,
        field: &str,
        value: &str,
        extra: &str,
    ) -> Result<Option<T>> {
        let query = format!(
            "SELECT * FROM {} WHERE {} = $1{}",
            self.table_name(), field, extra
        );
        let result = crate::company_scope::fetch_optional_scoped(
            self.pool(),
            sqlx::query_as::<_, T>(&query).bind(value),
        )
        .await?;
        Ok(result)
    }

    /// Internal: existence check by a text field with a caller-supplied extra condition.
    async fn exists_by_text_field_with_cond(
        &self,
        field: &str,
        value: &str,
        extra: &str,
    ) -> Result<bool> {
        let query = format!(
            "SELECT 1 FROM {} WHERE {} = $1{} LIMIT 1",
            self.table_name(), field, extra
        );
        let result = crate::company_scope::fetch_optional_scalar_scoped(
            self.pool(),
            sqlx::query_scalar::<_, i32>(&query).bind(value),
        )
        .await?;
        Ok(result.is_some())
    }

    /// Internal: query by a UUID field with a caller-supplied extra condition.
    async fn find_by_uuid_field_with_cond(
        &self,
        field: &str,
        value: Uuid,
        extra: &str,
    ) -> Result<Option<T>> {
        let query = format!(
            "SELECT * FROM {} WHERE {} = $1{}",
            self.table_name(), field, extra
        );
        let result = crate::company_scope::fetch_optional_scoped(
            self.pool(),
            sqlx::query_as::<_, T>(&query).bind(value),
        )
        .await?;
        Ok(result)
    }

    /// Internal: existence check by a UUID field with a caller-supplied extra condition.
    async fn exists_by_uuid_field_with_cond(
        &self,
        field: &str,
        value: Uuid,
        extra: &str,
    ) -> Result<bool> {
        let query = format!(
            "SELECT 1 FROM {} WHERE {} = $1{} LIMIT 1",
            self.table_name(), field, extra
        );
        let result = crate::company_scope::fetch_optional_scalar_scoped(
            self.pool(),
            sqlx::query_scalar::<_, i32>(&query).bind(value),
        )
        .await?;
        Ok(result.is_some())
    }

    /// Execute a filtered / paginated query against this entity's table.
    ///
    /// `base_condition` — when `Some`, raw SQL ANDed as its own parenthesised group ahead of
    /// the caller-supplied filters, which can therefore never widen past it (not even with
    /// an `[or]` filter). It must be code-authored; the filters map cannot carry one, since
    /// the parser ignores every `__`-prefixed key.
    ///
    /// NOTE: prefer the `*_scoped` entry points for anything a client can reach —
    /// they compose the company fence into this condition. This raw form applies no
    /// fence and is for internal/job callers that have already established scope.
    pub async fn run_filtered_query(
        &self,
        pagination: PaginationParams,
        base_condition: Option<&str>,
        filters: &HashMap<String, String>,
        column_types: &HashMap<String, String>,
        search_fields: &[&str],
    ) -> Result<PaginatedResult<T>>
    where
        T: Send + Sync,
    {
        self.inner
            .list_with_filters_and_base(
                pagination,
                base_condition,
                filters,
                column_types,
                search_fields,
            )
            .await
    }

    /// The aggregate twin of [`Self::run_filtered_query`].
    ///
    /// Takes the same `base_condition` so an aggregate is computed over exactly
    /// the row set the matching list would return — a total that counted
    /// soft-deleted rows, or another tenant's, would be wrong in a way no
    /// caller could see.
    pub async fn run_aggregate_query(
        &self,
        spec: &crate::repository::AggregateSpec,
        base_condition: Option<&str>,
        filters: &HashMap<String, String>,
        column_types: &HashMap<String, String>,
        search_fields: &[&str],
    ) -> Result<crate::repository::AggregateResult>
    where
        T: crate::EntityRepoMeta + Send + Sync,
    {
        // Resolve the group column's relation HERE (the only layer that
        // knows the entity's metadata): a group on a relation FK gets its
        // target table passed down so the inner query can carry the label.
        let mut spec = spec.clone();
        if let (Some(group), Some(label)) = (&spec.group_by, &spec.label_field) {
            let _ = label;
            let camel = snake_to_camel(group);
            if let Some((_, table, _)) = T::relations().iter().find(|(_, _, fk)| *fk == camel) {
                let base_fk = camel_to_snake(&camel);
                spec.label_relation = Some((table.to_string(), base_fk));
            }
        }
        self.inner
            .aggregate_with_filters_and_base(
                &spec,
                base_condition,
                filters,
                column_types,
                search_fields,
            )
            .await
    }
}

// ─── SoftDelete mode ──────────────────────────────────────────────────────────

impl<T> GenericCrudRepository<T, SoftDelete>
where
    T: for<'r> FromRow<'r, PgRow> + Send + Unpin,
{
    // ── Unique-field lookups (active records only) ────────────────────────────

    /// Find an active entity by a unique text field.
    ///
    /// Filters `metadata->>'deleted_at' IS NULL` automatically.
    pub async fn find_by_text_field(&self, field: &str, value: &str) -> Result<Option<T>> {
        self.find_by_text_field_with_cond(field, value, " AND metadata->>'deleted_at' IS NULL").await
    }

    /// Check existence by a unique text field (active records only).
    pub async fn exists_by_text_field(&self, field: &str, value: &str) -> Result<bool> {
        self.exists_by_text_field_with_cond(field, value, " AND metadata->>'deleted_at' IS NULL").await
    }

    /// Find an active entity by a unique UUID field.
    pub async fn find_by_uuid_field(&self, field: &str, value: Uuid) -> Result<Option<T>> {
        self.find_by_uuid_field_with_cond(field, value, " AND metadata->>'deleted_at' IS NULL").await
    }

    /// Check existence by a unique UUID field (active records only).
    pub async fn exists_by_uuid_field(&self, field: &str, value: Uuid) -> Result<bool> {
        self.exists_by_uuid_field_with_cond(field, value, " AND metadata->>'deleted_at' IS NULL").await
    }

    // ── Filtered pagination (requires EntityRepoMeta on T) ────────────────────

    /// Paginate active entities with filter and search support.
    ///
    /// Requires `T: EntityRepoMeta` for column type hints and search fields.
    pub async fn list_paginated_filtered(
        &self,
        pagination: PaginationParams,
        filters: Option<&HashMap<String, String>>,
    ) -> Result<PaginatedResult<T>>
    where
        T: EntityRepoMeta + Send + Sync,
    {
        let filters_map = filters.cloned().unwrap_or_default();
        let column_types = T::column_types();
        let search_fields_owned: Vec<&'static str> = T::search_fields().iter().copied().collect();
        self.run_filtered_query(
            pagination,
            Some("metadata->>'deleted_at' IS NULL"),
            &filters_map,
            &column_types,
            &search_fields_owned,
        ).await
    }

    /// Group and reduce active entities, skipping the soft-deleted.
    pub async fn aggregate_filtered(
        &self,
        spec: &crate::repository::AggregateSpec,
        filters: Option<&HashMap<String, String>>,
    ) -> Result<crate::repository::AggregateResult>
    where
        T: EntityRepoMeta + Send + Sync,
    {
        let filters_map = filters.cloned().unwrap_or_default();
        let column_types = T::column_types();
        let search_fields_owned: Vec<&'static str> = T::search_fields().iter().copied().collect();
        self.run_aggregate_query(spec, Some("metadata->>'deleted_at' IS NULL"), &filters_map, &column_types, &search_fields_owned).await
    }

    /// Paginate active entities, fenced to `company`.
    ///
    /// The company-scoped counterpart of [`Self::list_paginated_filtered`] and the one any
    /// client-reachable read should use. Fails closed: a fenced entity with no company
    /// returns [`MissingCompanyScope`] rather than every company's rows.
    ///
    /// The fence is ANDed into the same base condition the soft-delete guard uses, so
    /// it lands in SQL — which is what keeps `total` honest. A post-filter in the handler
    /// would fetch `limit` rows and return the survivors, leaving callers unable to tell
    /// "end of data" from "filtered", and could not fix `COUNT` at all.
    pub async fn list_paginated_filtered_scoped(
        &self,
        pagination: PaginationParams,
        filters: Option<&HashMap<String, String>>,
        company: Option<Uuid>,
    ) -> Result<PaginatedResult<T>>
    where
        T: EntityRepoMeta + Send + Sync,
    {
        let fence = company_fence::<T>(company)?;
        let mut filters_map = filters.cloned().unwrap_or_default();
        strip_client_company_filters::<T>(&mut filters_map);
        let column_types = T::column_types();
        let search_fields_owned: Vec<&'static str> = T::search_fields().iter().copied().collect();
        self.run_filtered_query(
            pagination,
            and_conditions(Some("metadata->>'deleted_at' IS NULL"), fence).as_deref(),
            &filters_map,
            &column_types,
            &search_fields_owned,
        )
        .await
    }

    /// Paginate soft-deleted entities, fenced to `company`.
    ///
    /// `/trash` is the worse leak of the two: it serves rows whose owners were told the
    /// data is gone. Same fence, same fail-closed contract.
    pub async fn list_deleted_filtered_scoped(
        &self,
        pagination: PaginationParams,
        filters: Option<&HashMap<String, String>>,
        company: Option<Uuid>,
    ) -> Result<PaginatedResult<T>>
    where
        T: EntityRepoMeta + Send + Sync,
    {
        let fence = company_fence::<T>(company)?;
        let mut filters_map = filters.cloned().unwrap_or_default();
        strip_client_company_filters::<T>(&mut filters_map);
        let column_types = T::column_types();
        let search_fields: Vec<&'static str> = T::search_fields().to_vec();
        self.run_filtered_query(
            pagination,
            and_conditions(Some("metadata->>'deleted_at' IS NOT NULL"), fence).as_deref(),
            &filters_map,
            &column_types,
            &search_fields,
        )
        .await
    }

    /// Paginate soft-deleted entities with filter and search support — the trash twin of
    /// [`Self::list_paginated_filtered`].
    pub async fn list_deleted_filtered(
        &self,
        pagination: PaginationParams,
        filters: Option<&HashMap<String, String>>,
    ) -> Result<PaginatedResult<T>>
    where
        T: EntityRepoMeta + Send + Sync,
    {
        let filters_map = filters.cloned().unwrap_or_default();
        let column_types = T::column_types();
        let search_fields: Vec<&'static str> = T::search_fields().to_vec();
        self.run_filtered_query(
            pagination,
            Some("metadata->>'deleted_at' IS NOT NULL"),
            &filters_map,
            &column_types,
            &search_fields,
        ).await
    }

    /// Find an active (non-deleted) entity by primary key.
    pub async fn find_by_id(&self, id: &str) -> Result<Option<T>> {
        let query = format!(
            "SELECT * FROM {} WHERE id = $1::uuid AND metadata->>'deleted_at' IS NULL",
            self.table_name()
        );
        let result = crate::company_scope::fetch_optional_scoped(
            self.pool(),
            sqlx::query_as::<_, T>(&query).bind(id),
        )
        .await?;
        Ok(result)
    }

    /// Return all active (non-deleted) entities.
    pub async fn find_all(&self) -> Result<Vec<T>> {
        let query = format!(
            "SELECT * FROM {} WHERE metadata->>'deleted_at' IS NULL",
            self.table_name()
        );
        let results = crate::company_scope::fetch_all_scoped(
            self.pool(),
            sqlx::query_as::<_, T>(&query),
        )
        .await?;
        Ok(results)
    }

    /// Full update of an active record; `Ok(None)` when it is missing or soft-deleted.
    ///
    /// The not-deleted predicate is part of the UPDATE itself, so a write racing a soft
    /// delete can never revive the row.
    pub async fn update(&self, id: &str, entity: &T) -> Result<Option<T>>
    where
        T: Serialize + Send + Sync,
    {
        self.inner
            .update_guarded(id, entity, None, ACTIVE_ROW_GUARD)
            .await
    }

    /// Update an active record from `before` (the row as read) to `after`, writing only the
    /// columns whose value differs between the two; `Ok(None)` when the row is missing or was
    /// soft-deleted since it was read.
    ///
    /// The path for a read-modify-write such as a PATCH: a column the caller did not change is
    /// left as it now stands in the database, so a concurrent write to it (a verb moving a
    /// status) is not overwritten with the stale value read before it.
    pub async fn update_changed(&self, id: &str, before: &T, after: &T) -> Result<Option<T>>
    where
        T: Serialize + Send + Sync,
    {
        let changed = crate::repository::changed_columns(before, after)?;
        self.inner
            .update_guarded(id, after, Some(&changed), ACTIVE_ROW_GUARD)
            .await
    }

    /// Soft-delete an entity (sets `metadata.deleted_at`).
    pub async fn delete(&self, id: &str) -> Result<bool> {
        self.soft_delete(id).await
    }

    /// Count active (non-deleted) entities.
    pub async fn count(&self) -> Result<u64> {
        self.count_active().await
    }

    /// Return `true` if an active entity with the given ID exists.
    pub async fn exists(&self, id: &str) -> Result<bool> {
        let query = format!(
            "SELECT 1 FROM {} WHERE id = $1::uuid AND metadata->>'deleted_at' IS NULL LIMIT 1",
            self.table_name()
        );
        let result = crate::company_scope::fetch_optional_scalar_scoped(
            self.pool(),
            sqlx::query_scalar::<_, i32>(&query).bind(id),
        )
        .await?;
        Ok(result.is_some())
    }

    /// Paginate active entities, ordered by `id` ascending — the same key order every
    /// filtered list ends on, so a page never repeats or skips a row between two fetches.
    pub async fn list_paginated(&self, pagination: PaginationParams) -> Result<PaginatedResult<T>> {
        let offset = pagination.offset();
        let limit = pagination.limit();
        let query = format!(
            "SELECT * FROM {} WHERE metadata->>'deleted_at' IS NULL \
             ORDER BY id LIMIT $1 OFFSET $2",
            self.table_name()
        );
        let data = crate::company_scope::fetch_all_scoped(
            self.pool(),
            sqlx::query_as::<_, T>(&query).bind(limit as i64).bind(offset as i64),
        )
        .await?;
        let total = self.count_active().await?;
        Ok(PaginatedResult {
            data,
            pagination: PaginationInfo::new(pagination.page, pagination.per_page, total),
        })
    }

    // ── Soft-delete helpers ───────────────────────────────────────────────────

    /// Set `metadata.deleted_at` to NOW() (soft delete).
    pub async fn soft_delete(&self, id: &str) -> Result<bool> {
        let query = format!(
            "UPDATE {} SET metadata = jsonb_set(\
               COALESCE(metadata, '{{}}'), \
               '{{deleted_at}}', \
               to_jsonb(NOW())\
             ) WHERE id = $1::uuid AND (metadata->>'deleted_at') IS NULL",
            self.table_name()
        );
        let result = crate::company_scope::execute_scoped(
            self.pool(),
            sqlx::query(&query).bind(id),
        )
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Remove `deleted_at` from metadata, restoring the entity.
    pub async fn restore(&self, id: &str) -> Result<Option<T>> {
        let query = format!(
            "UPDATE {} SET metadata = metadata - 'deleted_at' \
             WHERE id = $1::uuid AND (metadata->>'deleted_at') IS NOT NULL \
             RETURNING *",
            self.table_name()
        );
        let result = crate::company_scope::fetch_optional_scoped(
            self.pool(),
            sqlx::query_as::<_, T>(&query).bind(id),
        )
        .await?;
        Ok(result)
    }

    /// Paginate soft-deleted entities (trash view), most recently deleted first, `id` ascending
    /// among rows deleted at the same instant so the order is total.
    pub async fn list_deleted(&self, pagination: PaginationParams) -> Result<PaginatedResult<T>> {
        let offset = pagination.offset();
        let limit = pagination.limit();
        let query = format!(
            "SELECT * FROM {} WHERE (metadata->>'deleted_at') IS NOT NULL \
             ORDER BY (metadata->>'deleted_at') DESC, id LIMIT $1 OFFSET $2",
            self.table_name()
        );
        let data = crate::company_scope::fetch_all_scoped(
            self.pool(),
            sqlx::query_as::<_, T>(&query).bind(limit as i64).bind(offset as i64),
        )
        .await?;
        let count_query = format!(
            "SELECT COUNT(*) FROM {} WHERE (metadata->>'deleted_at') IS NOT NULL",
            self.table_name()
        );
        let total = crate::company_scope::fetch_one_scalar_scoped(
            self.pool(),
            sqlx::query_scalar::<_, i64>(&count_query),
        )
        .await? as u64;
        Ok(PaginatedResult {
            data,
            pagination: PaginationInfo::new(pagination.page, pagination.per_page, total),
        })
    }

    /// Permanently delete all soft-deleted rows (empty trash).
    pub async fn empty_trash(&self) -> Result<u64> {
        let query = format!(
            "DELETE FROM {} WHERE (metadata->>'deleted_at') IS NOT NULL",
            self.table_name()
        );
        let result = crate::company_scope::execute_scoped(self.pool(), sqlx::query(&query)).await?;
        Ok(result.rows_affected())
    }

    /// Find a soft-deleted entity by primary key.
    pub async fn find_deleted_by_id(&self, id: &str) -> Result<Option<T>> {
        let query = format!(
            "SELECT * FROM {} WHERE id = $1::uuid AND (metadata->>'deleted_at') IS NOT NULL",
            self.table_name()
        );
        let result = crate::company_scope::fetch_optional_scoped(
            self.pool(),
            sqlx::query_as::<_, T>(&query).bind(id),
        )
        .await?;
        Ok(result)
    }

    /// Permanently delete a soft-deleted entity by primary key.
    pub async fn permanent_delete(&self, id: &str) -> Result<bool> {
        let query = format!(
            "DELETE FROM {} WHERE id = $1::uuid AND (metadata->>'deleted_at') IS NOT NULL",
            self.table_name()
        );
        let result = crate::company_scope::execute_scoped(
            self.pool(),
            sqlx::query(&query).bind(id),
        )
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Count active (non-deleted) entities.
    pub async fn count_active(&self) -> Result<u64> {
        let query = format!(
            "SELECT COUNT(*) FROM {} WHERE (metadata->>'deleted_at') IS NULL",
            self.table_name()
        );
        let count = crate::company_scope::fetch_one_scalar_scoped(
            self.pool(),
            sqlx::query_scalar::<_, i64>(&query),
        )
        .await? as u64;
        Ok(count)
    }

    /// Count soft-deleted entities.
    pub async fn count_deleted(&self) -> Result<u64> {
        let query = format!(
            "SELECT COUNT(*) FROM {} WHERE (metadata->>'deleted_at') IS NOT NULL",
            self.table_name()
        );
        let count = crate::company_scope::fetch_one_scalar_scoped(
            self.pool(),
            sqlx::query_scalar::<_, i64>(&query),
        )
        .await? as u64;
        Ok(count)
    }

    // ── Atomic batch operations ───────────────────────────────────────────────
    //
    // Each runs inside a single transaction. For id-list operations the affected
    // row count must equal the number of ids requested, otherwise the whole batch
    // is rolled back — all-or-nothing semantics where a missing / already-in-the-
    // target-state id (or a duplicate id) fails the entire request.

    /// Soft-delete many active rows atomically.
    pub async fn bulk_soft_delete(&self, ids: &[String]) -> Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }
        let placeholders = id_in_placeholders(ids.len());
        let query = format!(
            "UPDATE {} SET metadata = jsonb_set(\
               COALESCE(metadata, '{{}}'), '{{deleted_at}}', to_jsonb(NOW())\
             ) WHERE id IN ({placeholders}) AND (metadata->>'deleted_at') IS NULL",
            self.table_name()
        );
        let unit = crate::write_tx::begin_unit(self.pool()).await?;
        let outcome: Result<u64> = async {
            let mut conn = unit.conn().await;
            let mut q = sqlx::query(&query);
            for id in ids {
                q = q.bind(id);
            }
            let affected = q.execute(&mut *conn).await?.rows_affected();
            if affected != ids.len() as u64 {
                // Finishing with an error rolls the unit back.
                return Err(anyhow::anyhow!(
                    "bulk_soft_delete: {} of {} ids were not active/deletable; rolled back",
                    ids.len() as u64 - affected,
                    ids.len()
                ));
            }
            Ok(affected)
        }
        .await;
        unit.finish(outcome).await
    }

    /// Restore many soft-deleted rows atomically, returning the restored rows.
    pub async fn bulk_restore(&self, ids: &[String]) -> Result<Vec<T>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = id_in_placeholders(ids.len());
        let query = format!(
            "UPDATE {} SET metadata = metadata - 'deleted_at' \
             WHERE id IN ({placeholders}) AND (metadata->>'deleted_at') IS NOT NULL \
             RETURNING *",
            self.table_name()
        );
        let unit = crate::write_tx::begin_unit(self.pool()).await?;
        let outcome: Result<Vec<T>> = async {
            let mut conn = unit.conn().await;
            let mut q = sqlx::query_as::<_, T>(&query);
            for id in ids {
                q = q.bind(id);
            }
            let rows = q.fetch_all(&mut *conn).await?;
            if rows.len() != ids.len() {
                return Err(anyhow::anyhow!(
                    "bulk_restore: {} of {} ids were not in trash; rolled back",
                    ids.len() - rows.len(),
                    ids.len()
                ));
            }
            Ok(rows)
        }
        .await;
        unit.finish(outcome).await
    }

    /// Permanently delete many soft-deleted rows atomically.
    pub async fn bulk_permanent_delete(&self, ids: &[String]) -> Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }
        let placeholders = id_in_placeholders(ids.len());
        let query = format!(
            "DELETE FROM {} WHERE id IN ({placeholders}) AND (metadata->>'deleted_at') IS NOT NULL",
            self.table_name()
        );
        let unit = crate::write_tx::begin_unit(self.pool()).await?;
        let outcome: Result<u64> = async {
            let mut conn = unit.conn().await;
            let mut q = sqlx::query(&query);
            for id in ids {
                q = q.bind(id);
            }
            let affected = q.execute(&mut *conn).await?.rows_affected();
            if affected != ids.len() as u64 {
                return Err(anyhow::anyhow!(
                    "bulk_permanent_delete: {} of {} ids were not in trash; rolled back",
                    ids.len() as u64 - affected,
                    ids.len()
                ));
            }
            Ok(affected)
        }
        .await;
        unit.finish(outcome).await
    }

    /// Restore every soft-deleted row, returning the restored rows. A single
    /// `UPDATE ... RETURNING` is atomic, and the returned rows let the service
    /// layer emit a `Restored` event per entity.
    pub async fn restore_all(&self) -> Result<Vec<T>> {
        let query = format!(
            "UPDATE {} SET metadata = metadata - 'deleted_at' \
             WHERE (metadata->>'deleted_at') IS NOT NULL \
             RETURNING *",
            self.table_name()
        );
        let rows = crate::company_scope::fetch_all_scoped(
            self.pool(),
            sqlx::query_as::<_, T>(&query),
        )
        .await?;
        Ok(rows)
    }

    /// Update many active rows atomically. Every entity must reference an
    /// existing active (non-soft-deleted) row or the whole batch is rolled back.
    pub async fn bulk_update(&self, entities: &[T]) -> Result<Vec<T>>
    where
        T: Serialize + Send + Sync,
    {
        bulk_update_rows(
            self.pool(),
            self.table_name(),
            ACTIVE_ROW_GUARD,
            entities,
        )
        .await
    }

    /// Update many active rows from `(before, after)` pairs atomically, writing only the
    /// columns each update changed; a row missing or soft-deleted since it was read rolls the
    /// whole batch back.
    pub async fn bulk_update_changed(&self, pairs: &[(T, T)]) -> Result<Vec<T>>
    where
        T: Serialize + Send + Sync,
    {
        self.bulk_update_changed_guarded(pairs, ACTIVE_ROW_GUARD).await
    }
}

/// Build `"$1::uuid, $2::uuid, …"` for an `id IN (…)` clause of `n` bound ids.
fn id_in_placeholders(n: usize) -> String {
    (1..=n)
        .map(|i| format!("${i}::uuid"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// One item of a batch write failed, so the whole batch was rolled back.
///
/// `index` is the item's position in the batch as the caller passed it; `error` is why it
/// failed (a database refusal keeps its `sqlx::Error` inside, so a caller can still read the
/// constraint it broke).
#[derive(Debug)]
pub struct BatchItemError {
    /// Position of the failed item in the batch.
    pub index: usize,
    /// Why it failed.
    pub error: anyhow::Error,
}

impl std::fmt::Display for BatchItemError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "item {} of the batch: {}; rolled back", self.index, self.error)
    }
}

impl std::error::Error for BatchItemError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.error.as_ref())
    }
}

/// Insert each entity on `conn` (an open transaction), stopping at the first failure.
async fn insert_each<T>(conn: &mut sqlx::PgConnection, table: &str, entities: &[T]) -> Result<Vec<T>>
where
    T: for<'r> FromRow<'r, PgRow> + Serialize + Send + Sync + Unpin,
{
    let mut out = Vec::with_capacity(entities.len());
    for (index, entity) in entities.iter().enumerate() {
        let (query, json) = crate::repository::insert_statement(table, entity)
            .map_err(|error| BatchItemError { index, error })?;
        let statement = match json {
            Some(json) => sqlx::query_as::<_, T>(&query).bind(json),
            None => sqlx::query_as::<_, T>(&query),
        };
        let row = statement.fetch_one(&mut *conn).await.map_err(|e| BatchItemError {
            index,
            error: crate::repository::explain_unknown_column(e, table),
        })?;
        out.push(row);
    }
    Ok(out)
}

/// Apply each `(before, after)` pair on `conn` (an open transaction) as a changed-columns
/// update under `guard`, stopping at the first row that fails or no longer matches.
async fn update_each_changed<T>(
    conn: &mut sqlx::PgConnection,
    table: &str,
    guard: &str,
    pairs: &[(T, T)],
) -> Result<Vec<T>>
where
    T: for<'r> FromRow<'r, PgRow> + Serialize + Send + Sync + Unpin,
{
    use crate::repository::UpdateStatement;
    let mut out = Vec::with_capacity(pairs.len());
    for (index, (before, after)) in pairs.iter().enumerate() {
        let item = |error: anyhow::Error| BatchItemError { index, error };
        let id = match serde_json::to_value(after).map_err(|e| item(e.into()))? {
            serde_json::Value::Object(obj) => match obj.get("id") {
                Some(serde_json::Value::String(id)) => id.clone(),
                _ => return Err(item(anyhow::anyhow!("entity missing string 'id' field")).into()),
            },
            _ => return Err(item(anyhow::anyhow!("entity must serialize to a JSON object")).into()),
        };
        let changed = crate::repository::changed_columns(before, after).map_err(item)?;
        let updated = match crate::repository::update_statement(table, after, Some(&changed), guard)
            .map_err(item)?
        {
            UpdateStatement::Unchanged(query) => {
                sqlx::query_as::<_, T>(&query).bind(&id).fetch_optional(&mut *conn).await
            }
            UpdateStatement::Update { query, json } => {
                sqlx::query_as::<_, T>(&query)
                    .bind(json)
                    .bind(&id)
                    .fetch_optional(&mut *conn)
                    .await
            }
        }
        .map_err(|e| item(e.into()))?;
        match updated {
            Some(row) => out.push(row),
            None => {
                return Err(item(anyhow::anyhow!("id '{id}' not found or already deleted")).into())
            }
        }
    }
    Ok(out)
}

/// Serialize an entity and return `(id, json_string, quoted_column_list)` for the
/// `jsonb_populate_record` update query — shared by the per-mode `bulk_update`s.
fn build_update_parts<T: Serialize>(entity: &T) -> Result<(String, String, String)> {
    let json_value = serde_json::to_value(entity)?;
    let json_obj = match json_value {
        serde_json::Value::Object(obj) => obj,
        _ => return Err(anyhow::anyhow!("entity must serialize to a JSON object")),
    };
    let id = json_obj
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("entity missing string 'id' field"))?
        .to_string();
    let column_names = json_obj
        .keys()
        .filter(|k| *k != "id")
        .map(|k| format!("\"{k}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let json_str = serde_json::to_string(&json_obj)?;
    Ok((id, json_str, column_names))
}

// ─── HardDelete mode ─────────────────────────────────────────────────────────

impl<T> GenericCrudRepository<T, HardDelete>
where
    T: for<'r> FromRow<'r, PgRow> + Send + Sync + Unpin + Serialize,
{
    // ── Unique-field lookups (no soft-delete guard) ───────────────────────────

    /// Find an entity by a unique text field (no soft-delete guard).
    pub async fn find_by_text_field(&self, field: &str, value: &str) -> Result<Option<T>> {
        self.find_by_text_field_with_cond(field, value, "").await
    }

    /// Check existence by a unique text field.
    pub async fn exists_by_text_field(&self, field: &str, value: &str) -> Result<bool> {
        self.exists_by_text_field_with_cond(field, value, "").await
    }

    /// Find an entity by a unique UUID field.
    pub async fn find_by_uuid_field(&self, field: &str, value: Uuid) -> Result<Option<T>> {
        self.find_by_uuid_field_with_cond(field, value, "").await
    }

    /// Check existence by a unique UUID field.
    pub async fn exists_by_uuid_field(&self, field: &str, value: Uuid) -> Result<bool> {
        self.exists_by_uuid_field_with_cond(field, value, "").await
    }

    // ── Filtered pagination ───────────────────────────────────────────────────

    /// Paginate entities with filter and search support.
    pub async fn list_paginated_filtered(
        &self,
        pagination: PaginationParams,
        filters: Option<&HashMap<String, String>>,
    ) -> Result<PaginatedResult<T>>
    where
        T: EntityRepoMeta + Send + Sync,
    {
        let filters_map = filters.cloned().unwrap_or_default();
        let column_types = T::column_types();
        let search_fields_owned: Vec<&'static str> = T::search_fields().iter().copied().collect();
        self.run_filtered_query(pagination, None, &filters_map, &column_types, &search_fields_owned).await
    }

    /// Group and reduce entities. This mode has no trash to exclude.
    pub async fn aggregate_filtered(
        &self,
        spec: &crate::repository::AggregateSpec,
        filters: Option<&HashMap<String, String>>,
    ) -> Result<crate::repository::AggregateResult>
    where
        T: EntityRepoMeta + Send + Sync,
    {
        let filters_map = filters.cloned().unwrap_or_default();
        let column_types = T::column_types();
        let search_fields_owned: Vec<&'static str> = T::search_fields().iter().copied().collect();
        self.run_aggregate_query(spec, None, &filters_map, &column_types, &search_fields_owned).await
    }

    /// Find an entity by primary key.
    pub async fn find_by_id(&self, id: &str) -> Result<Option<T>> {
        self.inner.find_by_id(id).await
    }

    /// Return all entities.
    pub async fn find_all(&self) -> Result<Vec<T>> {
        let query = format!("SELECT * FROM {}", self.table_name());
        let results = crate::company_scope::fetch_all_scoped(
            self.pool(),
            sqlx::query_as::<_, T>(&query),
        )
        .await?;
        Ok(results)
    }

    /// Full update.
    pub async fn update(&self, id: &str, entity: &T) -> Result<Option<T>> {
        self.inner.update(id, entity).await
    }

    /// Update a record from `before` (the row as read) to `after`, writing only the columns
    /// whose value differs; `Ok(None)` when the row no longer exists. See the soft-delete
    /// twin for why a read-modify-write must not write back the columns it did not change.
    pub async fn update_changed(&self, id: &str, before: &T, after: &T) -> Result<Option<T>> {
        let changed = crate::repository::changed_columns(before, after)?;
        self.inner
            .update_guarded(id, after, Some(&changed), "")
            .await
    }

    /// Permanently delete an entity by primary key.
    pub async fn delete(&self, id: &str) -> Result<bool> {
        self.inner.delete(id).await
    }

    /// Count all entities.
    pub async fn count(&self) -> Result<u64> {
        let query = format!("SELECT COUNT(*) FROM {}", self.table_name());
        let count = crate::company_scope::fetch_one_scalar_scoped(
            self.pool(),
            sqlx::query_scalar::<_, i64>(&query),
        )
        .await? as u64;
        Ok(count)
    }

    /// Return `true` if an entity with the given ID exists.
    pub async fn exists(&self, id: &str) -> Result<bool> {
        let query = format!(
            "SELECT 1 FROM {} WHERE id = $1::uuid LIMIT 1",
            self.table_name()
        );
        let result = crate::company_scope::fetch_optional_scalar_scoped(
            self.pool(),
            sqlx::query_scalar::<_, i32>(&query).bind(id),
        )
        .await?;
        Ok(result.is_some())
    }

    /// Paginate entities, ordered by `id` ascending — the same key order every filtered list
    /// ends on, so a page never repeats or skips a row between two fetches.
    pub async fn list_paginated(&self, pagination: PaginationParams) -> Result<PaginatedResult<T>> {
        let offset = pagination.offset();
        let limit = pagination.limit();
        let query = format!(
            "SELECT * FROM {} ORDER BY id LIMIT $1 OFFSET $2",
            self.table_name()
        );
        let data = crate::company_scope::fetch_all_scoped(
            self.pool(),
            sqlx::query_as::<_, T>(&query).bind(limit as i64).bind(offset as i64),
        )
        .await?;
        let total = self.count().await?;
        Ok(PaginatedResult {
            data,
            pagination: PaginationInfo::new(pagination.page, pagination.per_page, total),
        })
    }

    // ── Atomic batch operations ───────────────────────────────────────────────

    /// Hard-delete many rows atomically. Rolls back unless every id matched.
    pub async fn bulk_delete(&self, ids: &[String]) -> Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }
        let placeholders = id_in_placeholders(ids.len());
        let query = format!(
            "DELETE FROM {} WHERE id IN ({placeholders})",
            self.table_name()
        );
        let unit = crate::write_tx::begin_unit(self.pool()).await?;
        let outcome: Result<u64> = async {
            let mut conn = unit.conn().await;
            let mut q = sqlx::query(&query);
            for id in ids {
                q = q.bind(id);
            }
            let affected = q.execute(&mut *conn).await?.rows_affected();
            if affected != ids.len() as u64 {
                return Err(anyhow::anyhow!(
                    "bulk_delete: {} of {} ids not found; rolled back",
                    ids.len() as u64 - affected,
                    ids.len()
                ));
            }
            Ok(affected)
        }
        .await;
        unit.finish(outcome).await
    }

    /// Update many rows atomically. No soft-delete guard in this mode.
    pub async fn bulk_update(&self, entities: &[T]) -> Result<Vec<T>> {
        bulk_update_rows(self.pool(), self.table_name(), "", entities).await
    }

    /// Update many rows from `(before, after)` pairs atomically, writing only the columns each
    /// update changed; a row deleted since it was read rolls the whole batch back.
    pub async fn bulk_update_changed(&self, pairs: &[(T, T)]) -> Result<Vec<T>> {
        self.bulk_update_changed_guarded(pairs, "").await
    }
}

/// Qualify a relation target table with the calling entity's schema.
///
/// `EntityRepoMeta::relations()` emits bare table names (e.g. `countries`), but
/// module tables live in the module's own schema (`geo.countries`) and the
/// runtime app role's search_path does not include module schemas — an
/// unqualified reference fails with "relation does not exist". The calling
/// entity's table name is schema-qualified, so its prefix is the schema a
/// relation declared inside one module shares with its target. Targets that
/// already carry a schema, and callers that do not, pass through unchanged.
pub fn qualify_relation_table(caller_table: &str, target_table: &str) -> String {
    if target_table.contains('.') || !caller_table.contains('.') {
        target_table.to_string()
    } else {
        let schema = caller_table.split('.').next().unwrap_or(caller_table);
        format!("{schema}.{target_table}")
    }
}

/// True when the error is Postgres `undefined_table` (SQLSTATE 42P01).
///
/// Walks the anyhow chain because the scoped-fetch error is converted with `?`
/// on its way up — the sqlx error rides as the (only) source.
fn is_undefined_table(err: &anyhow::Error) -> bool {
    err.chain()
        .filter_map(|cause| cause.downcast_ref::<sqlx::Error>())
        .any(|sqlx_err| {
            sqlx_err
                .as_database_error()
                .map(|db| db.code().as_deref() == Some("42P01"))
                .unwrap_or(false)
        })
}

/// Fetch rows from an arbitrary table by id list, as JSON — hydrates `?include=`
/// relations in the generic CRUD handler without a typed repository for the
/// target entity. `table` is a generator-emitted collection name (from
/// `EntityRepoMeta::relations()`), NEVER client input, so the interpolation is
/// not an injection vector. `caller_table` is the hydrated entity's own
/// (schema-qualified) table name, used to qualify a bare `table` — see
/// [`qualify_relation_table`]. One batched `WHERE id = ANY(...)` per relation.
/// Returns `row_to_json` objects keyed by raw (snake_case) column names.
///
/// Resolution is two-step for bare targets: the caller's schema first, then —
/// only on `undefined_table` — the name as written, which resolves through the
/// role's search_path (platform tables such as `users` or `roles` live in
/// `public` while their declaring entities live in a module schema). Any other
/// error, or a target that fails both spellings, is returned to the caller.
pub async fn fetch_by_ids_as_json(
    pool: &PgPool,
    caller_table: &str,
    table: &str,
    ids: &[String],
) -> Result<Vec<serde_json::Value>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let qualified = qualify_relation_table(caller_table, table);
    match fetch_rows_as_json(pool, &qualified, ids).await {
        Ok(rows) => Ok(rows),
        Err(err) if qualified != table && is_undefined_table(&err) => {
            fetch_rows_as_json(pool, table, ids).await
        }
        Err(err) => Err(err),
    }
}

/// Whether the row of `table` whose `column` holds `value` is visible on the caller's
/// scope: one `SELECT EXISTS` run as every read is (on the request's connection when one
/// is bound, else under the task's company scope), so a row row-level security hides
/// reads as absent. Postgres checks a foreign key without row-level security; this is
/// the check that does not.
///
/// `value` is a JSON scalar, cast to the column's own type by `jsonb_populate_record`, so
/// the lookup keeps the column's index whatever its type. `table` and `column` are
/// generator-emitted (`PersistentEntity::references`), never client input, and are refused
/// unless they are plain identifiers. A bare `table` resolves as
/// [`fetch_by_ids_as_json`]'s does: the caller's schema first, then the search path.
/// `Ok(None)` when the table does not exist in either spelling (its module is not
/// installed, and the database has no key to it either).
pub async fn reference_visible(
    pool: &PgPool,
    caller_table: &str,
    table: &str,
    column: &str,
    value: &serde_json::Value,
) -> Result<Option<bool>> {
    let plain = |s: &str, dot: bool| {
        !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || (dot && c == '.'))
    };
    if !plain(table, true) || !plain(column, false) {
        anyhow::bail!("reference target `{table}`.`{column}` is not a plain identifier");
    }
    let qualified = qualify_relation_table(caller_table, table);
    match row_visible(pool, &qualified, column, value).await {
        Ok(seen) => Ok(Some(seen)),
        Err(err) if is_undefined_table(&err) => {
            if qualified == table {
                return Ok(None);
            }
            match row_visible(pool, table, column, value).await {
                Ok(seen) => Ok(Some(seen)),
                Err(err) if is_undefined_table(&err) => Ok(None),
                Err(err) => Err(err),
            }
        }
        Err(err) => Err(err),
    }
}

async fn row_visible(
    pool: &PgPool,
    table: &str,
    column: &str,
    value: &serde_json::Value,
) -> Result<bool> {
    let query = format!(
        "SELECT EXISTS (SELECT 1 FROM {table} t \
         WHERE t.{column} = (jsonb_populate_record(NULL::{table}, $1::jsonb)).{column})"
    );
    let probe = serde_json::json!({ column: value });
    // A missing table fails this statement and the caller tries another spelling. Inside a write
    // transaction that failure would abort it, so the lookup runs as a savepoint of it.
    Ok(crate::write_tx::within_savepoint(crate::company_scope::fetch_one_scalar_scoped(
        pool,
        sqlx::query_scalar::<_, bool>(&query).bind(probe),
    ))
    .await?)
}

/// The scoped fetch behind [`fetch_by_ids_as_json`] — one relation, one table
/// spelling. Scoped because an `?include=` hydration of a company-fenced
/// relation must obey the same fence, so a cross-company related row is never
/// leaked through the expansion.
async fn fetch_rows_as_json(
    pool: &PgPool,
    table: &str,
    ids: &[String],
) -> Result<Vec<serde_json::Value>> {
    let query = format!("SELECT row_to_json(t) AS j FROM {table} t WHERE t.id = ANY($1::uuid[])");
    let rows: Vec<(serde_json::Value,)> =
        crate::company_scope::fetch_all_scoped(pool, sqlx::query_as(&query).bind(ids)).await?;
    Ok(rows.into_iter().map(|(j,)| j).collect())
}

/// Shared transactional bulk-update used by both delete modes. Each entity is
/// updated by id inside one transaction; a missing (or, with `active_guard`,
/// soft-deleted) row rolls the whole batch back. `active_guard` is an extra SQL
/// predicate ANDed into the `WHERE` clause (`""` for none).
async fn bulk_update_rows<T>(
    pool: &PgPool,
    table: &str,
    active_guard: &str,
    entities: &[T],
) -> Result<Vec<T>>
where
    T: for<'r> FromRow<'r, PgRow> + Send + Sync + Unpin + Serialize,
{
    if entities.is_empty() {
        return Ok(Vec::new());
    }
    let unit = crate::write_tx::begin_unit(pool).await?;
    let outcome: Result<Vec<T>> = async {
        let mut conn = unit.conn().await;
        let mut out = Vec::with_capacity(entities.len());
        for entity in entities {
            let (id, json_str, column_names) = build_update_parts(entity)?;
            let query = format!(
                "WITH new_row AS (\
                    SELECT (jsonb_populate_record(NULL::{table}, $1::jsonb)).*\
                 ) UPDATE {table} AS t \
                 SET ({columns}) = (SELECT {columns} FROM new_row) \
                 WHERE t.id = $2::uuid{guard} \
                 RETURNING t.*",
                table = table,
                columns = column_names,
                guard = active_guard,
            );
            let updated = sqlx::query_as::<_, T>(&query)
                .bind(&json_str)
                .bind(&id)
                .fetch_optional(&mut *conn)
                .await?;
            match updated {
                Some(e) => out.push(e),
                None => {
                    return Err(anyhow::anyhow!(
                        "bulk_update: id '{id}' not found or already deleted; rolled back"
                    ));
                }
            }
        }
        Ok(out)
    }
    .await;
    unit.finish(outcome).await
}

/// camelCase → snake_case (`projectId` → `project_id`), the SQL vocabulary.
fn camel_to_snake(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for ch in s.chars() {
        if ch.is_uppercase() {
            out.push('_');
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}
