//! Repository implementations for PostgreSQL with comprehensive CRUD operations

use async_trait::async_trait;
use sqlx::{PgPool, FromRow, postgres::PgRow, Postgres};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use chrono::NaiveDateTime;
use std::collections::{HashMap, HashSet};

use crate::qualify_relation_table;
use crate::filter::{parse_filters as parse_query_filter};
use crate::filter::SortDirection as FilterSortDirection;
use sqlx::Row as _;

/// Generic entity trait that all repository entities must implement
pub trait Entity {
    /// Get the entity's ID
    fn id(&self) -> Option<&str>;

    /// Get the table name for this entity
    fn table_name() -> &'static str where Self: Sized;

    /// Check if entity is soft deleted
    fn is_deleted(&self) -> bool { false }

    /// Get creation timestamp
    fn created_at(&self) -> Option<NaiveDateTime> { None }

    /// Get update timestamp
    fn updated_at(&self) -> Option<NaiveDateTime> { None }
}

/// Pagination parameters
#[derive(Debug, Clone, Default)]
pub struct PaginationParams {
    pub page: u32,
    pub per_page: u32,
}

impl PaginationParams {
    pub fn new(page: u32, per_page: u32) -> Self {
        Self {
            page: page.max(1),
            per_page: per_page.clamp(1, 100), // Limit to 1-100 per page
        }
    }

    pub fn offset(&self) -> u32 {
        (self.page - 1) * self.per_page
    }

    pub fn limit(&self) -> u32 {
        self.per_page
    }
}

/// Sorting parameters
#[derive(Debug, Clone, Default)]
pub struct SortParams {
    pub field: String,
    pub direction: SortDirection,
}

#[derive(Debug, Clone, Default)]
pub enum SortDirection {
    #[default]
    Asc,
    Desc,
}

/// Filter parameters
#[derive(Debug, Clone, Default)]
pub struct FilterParams {
    pub conditions: HashMap<String, FilterCondition>,
}

#[derive(Debug, Clone)]
pub enum FilterCondition {
    Equals(String),
    NotEquals(String),
    GreaterThan(String),
    LessThan(String),
    Like(String),
    In(Vec<String>),
    IsNull,
    IsNotNull,
}

/// Paginated result wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResult<T> {
    pub data: Vec<T>,
    pub pagination: PaginationInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationInfo {
    pub page: u32,
    pub per_page: u32,
    pub total: u64,
    pub total_pages: u32,
    /// Keyset paging: the position of this page's last row, for the next
    /// page's `after=`. None when the page is empty or no more rows follow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Keyset paging: the position of this page's first row, for the
    /// previous page's `before=`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_cursor: Option<String>,
    /// Whether another page follows (fetched with limit+1, so it is known
    /// without a count).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
    /// How `total` came to be: "exact" (counted), "estimate" (the planner's
    /// row estimate — `estimate=1` asked for it), or "none" (a cursor walk
    /// without an estimate; the exact figure is the separate count call).
    #[serde(default = "default_count_mode")]
    pub count_mode: String,
}

fn default_count_mode() -> String {
    "exact".to_string()
}

impl PaginationInfo {
    pub fn new(page: u32, per_page: u32, total: u64) -> Self {
        let total_pages = ((total as f64) / (per_page as f64)).ceil() as u32;
        Self {
            page,
            per_page,
            total,
            total_pages,
            next_cursor: None,
            prev_cursor: None,
            has_more: None,
            count_mode: default_count_mode(),
        }
    }
}

/// Database operations trait - requires Serialize for write operations
#[async_trait]
pub trait DatabaseOperations<T: for<'a> FromRow<'a, PgRow> + Send + Unpin> {
    /// Create a new entity
    async fn create(&self, entity: &T) -> anyhow::Result<T>;

    /// Find entity by ID
    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<T>>;

    /// Find all entities
    async fn find_all(&self) -> anyhow::Result<Vec<T>>;

    /// Update an existing entity
    async fn update(&self, id: &str, entity: &T) -> anyhow::Result<Option<T>>;

    /// Delete an entity
    async fn delete(&self, id: &str) -> anyhow::Result<bool>;

    /// Count all entities
    async fn count(&self) -> anyhow::Result<u64>;

    /// Check if entity exists
    async fn exists(&self, id: &str) -> anyhow::Result<bool>;

    /// Execute custom query
    async fn execute_query(&self, query: &str) -> anyhow::Result<u64>;
}

/// PostgreSQL repository implementation with JSON-based dynamic queries
pub struct PostgresRepository<T: for<'a> FromRow<'a, PgRow> + Send + Unpin> {
    pool: PgPool,
    table_name: String,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: for<'a> FromRow<'a, PgRow> + Send + Unpin> PostgresRepository<T> {
    pub fn new(pool: PgPool, table_name: &str) -> Self {
        Self {
            pool,
            table_name: table_name.to_string(),
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub fn table_name(&self) -> &str {
        &self.table_name
    }

    /// List entities with pagination and advanced filtering
    ///
    /// This method provides comprehensive filtering capabilities similar to Laravel's Filter Query String.
    ///
    /// # Supported Filter Operators
    ///
    /// - `field[eq]=value` - Equal
    /// - `field[notEq]=value` - Not equal
    /// - `field[gt]=value` - Greater than
    /// - `field[gte]=value` - Greater than or equal
    /// - `field[lt]=value` - Less than
    /// - `field[lte]=value` - Less than or equal
    /// - `field[like]=value` - LIKE (case-sensitive)
    /// - `field[ilike]=value` - ILIKE (case-insensitive)
    /// - `field[notlike]=value` - NOT LIKE
    /// - `field[contain]=value` - Contains (%value%)
    /// - `field[notcontain]=value` - Does not contain
    /// - `field[startwith]=value` - Starts with (value%)
    /// - `field[endwith]=value` - Ends with (%value)
    /// - `field[in]=val1,val2` - IN array
    /// - `field[notin]=val1,val2` - NOT IN array
    /// - `field[between]=val1,val2` - BETWEEN
    /// - `field[notbetween]=val1,val2` - NOT BETWEEN
    /// - `field[isnull]` - IS NULL
    /// - `field[isnotnull]` - IS NOT NULL
    ///
    /// # Special Parameters
    ///
    /// - `search=value&searchFields=field1,field2` - Search in multiple fields
    /// - `orderby=field` or `orderby[field]=asc` - Sort results
    /// - `limit=10` - Limit results
    /// - `page=1` - Page number
    ///
    /// # Column Type Casting
    ///
    /// The `column_types` HashMap maps field names to their PostgreSQL types for proper casting.
    /// For example, `{"status": "user_status"}` will cast the status parameter to `user_status` enum type.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut filters = HashMap::new();
    /// filters.insert("username[contain]".to_string(), "john".to_string());
    /// filters.insert("age[gt]".to_string(), "18".to_string());
    ///
    /// let mut column_types = HashMap::new();
    /// column_types.insert("status".to_string(), "user_status".to_string());
    ///
    /// let result = repo.list_with_filters(
    ///     PaginationParams::new(1, 10),
    ///     &filters,
    ///     &column_types,
    ///     &["username", "email"]  // search fields
    /// ).await?;
    /// ```
    pub async fn list_with_filters(
        &self,
        pagination: PaginationParams,
        filters: &HashMap<String, String>,
        column_types: &HashMap<String, String>,
        search_fields: &[&str],
    ) -> anyhow::Result<PaginatedResult<T>>
    where
        T: Send + Sync,
    {
        // Parse filters from HashMap (no field allow-list by default for backward compatibility)
        let mut query_filter = parse_query_filter(filters, column_types, None)?;

        // Set up search fields if provided
        if !search_fields.is_empty() {
            query_filter.search_fields = search_fields.iter().map(|s| s.to_string()).collect();
        }

        self.execute_list(pagination, query_filter).await
    }

    /// List entities with pagination, filtering, and field whitelist enforcement
    ///
    /// Similar to `list_with_filters` but accepts an optional set of allowed field names.
    /// When provided, only filter conditions on whitelisted fields are applied;
    /// conditions on unknown fields are silently dropped.
    ///
    /// This prevents clients from filtering on internal or sensitive columns
    /// (e.g., `password_hash`, `internal_notes`).
    ///
    /// # Arguments
    ///
    /// * `pagination` - Page and limit parameters
    /// * `filters` - HTTP query parameters (e.g., `field[operator]=value`)
    /// * `column_types` - PostgreSQL type mappings for enum casting
    /// * `search_fields` - Fields to search when `search` parameter is present
    /// * `allowed_fields` - Optional whitelist of field names; `None` allows all fields
    ///
    /// # Example
    ///
    /// ```ignore
    /// let allowed: HashSet<String> = ["username", "email", "status"]
    ///     .iter().map(|s| s.to_string()).collect();
    ///
    /// let result = repo.list_with_filters_whitelisted(
    ///     PaginationParams::new(1, 10),
    ///     &filters,
    ///     &column_types,
    ///     &["username", "email"],
    ///     Some(&allowed),
    /// ).await?;
    /// ```
    pub async fn list_with_filters_whitelisted(
        &self,
        pagination: PaginationParams,
        filters: &HashMap<String, String>,
        column_types: &HashMap<String, String>,
        search_fields: &[&str],
        allowed_fields: Option<&HashSet<String>>,
    ) -> anyhow::Result<PaginatedResult<T>>
    where
        T: Send + Sync,
    {
        // Parse filters with optional field whitelist
        let mut query_filter = parse_query_filter(filters, column_types, allowed_fields)?;

        // Set up search fields if provided
        if !search_fields.is_empty() {
            query_filter.search_fields = search_fields.iter().map(|s| s.to_string()).collect();
        }

        self.execute_list(pagination, query_filter).await
    }

    /// The shared list execution: filters, order, paging, and the total.
    ///
    /// Three shapes, chosen by the request:
    ///
    /// * **page mode** (today's behaviour, unchanged): exact COUNT, then
    ///   `LIMIT l OFFSET o` in the requested order;
    /// * **cursor mode** (`after=`/`before=`): the keyset predicate replaces
    ///   the offset, the order always ends on the `id` tiebreaker, and one
    ///   extra row is fetched so `has_more` is known without a count — the
    ///   page costs the same at any depth, which is the point;
    /// * **estimate** (`estimate=1`, either mode): the exact COUNT (a scan
    ///   of the whole filtered set) is replaced by the planner's row
    ///   estimate; the exact figure stays the separate count call.
    #[allow(clippy::type_complexity)]
    async fn execute_list(
        &self,
        pagination: PaginationParams,
        mut query_filter: crate::QueryFilter,
    ) -> anyhow::Result<PaginatedResult<T>> {
        let limit = pagination.limit() as i64;
        let backwards =
            query_filter.cursor_before.is_some() && query_filter.cursor_after.is_none();
        let cursor_walk = query_filter.cursor_after.is_some() || backwards;

        let (mut where_clause, mut filter_params) = query_filter.build_where_clause();
        let order_clause;
        // The deterministic order a cursor walks in (cursor mode only).
        let mut boundary_sorts: Vec<(String, FilterSortDirection)> = Vec::new();
        // Cast suffixes for the sort columns, for the keyset binds.
        let mut boundary_casts: Vec<Option<String>> = Vec::new();

        // The deterministic order: a cursor walks one, and a page-mode list
        // that carries a sort gets the same treatment so it can HAND OUT a
        // cursor to start a keyset walk from. Appending the id tiebreaker
        // only reorders rows that tied — ties had no order to preserve.
        let mut sorts: Vec<(String, FilterSortDirection)> = query_filter
            .sorts
            .iter()
            .map(|s| (s.field.clone(), s.direction.clone()))
            .collect();
        if cursor_walk || !sorts.is_empty() {
            if sorts.is_empty() {
                sorts.push(("id".into(), FilterSortDirection::Asc));
            } else if sorts.last().map(|(f, _)| f != "id").unwrap_or(true) {
                sorts.push(("id".into(), FilterSortDirection::Asc));
            }
        }

        // Casts whenever there is a deterministic order to key: cursor mode
        // walks on them, page mode encodes the handed-out cursor from them.
        if !sorts.is_empty() {
            boundary_casts = self.sort_column_casts(&sorts).await?;
        }

        if cursor_walk {
            let opaque = if backwards {
                query_filter.cursor_before.clone().unwrap()
            } else {
                query_filter.cursor_after.clone().unwrap()
            };
            let payload = crate::filter::cursor::decode_cursor(&opaque, &sorts)
                .map_err(|e| anyhow::anyhow!("cursor refused: {e}"))?;
            let mut idx = filter_params.len() + 1;
            let (keyset_sql, keyset_params) = crate::filter::cursor::build_keyset_predicate(
                &payload,
                &mut idx,
                &boundary_casts,
                backwards,
            );
            if where_clause.is_empty() {
                where_clause = format!(" WHERE {}", keyset_sql);
            } else {
                where_clause = format!("{} AND ({})", where_clause, keyset_sql);
            }
            filter_params.extend(keyset_params);
            let parts: Vec<String> = sorts
                .iter()
                .map(|(f, d)| {
                    let dir = if (*d == FilterSortDirection::Desc) != backwards {
                        "DESC"
                    } else {
                        "ASC"
                    };
                    format!("{} {}", f, dir)
                })
                .collect();
            order_clause = format!(" ORDER BY {}", parts.join(", "));
        } else {
            // Page mode with a sort: the caller's order, made deterministic
            // by the same id tiebreaker so the cursor it hands out is real.
            if sorts.is_empty() {
                order_clause = query_filter.build_order_by_clause();
            } else {
                let parts: Vec<String> = sorts
                    .iter()
                    .map(|(f, d)| {
                        let dir =
                            if *d == FilterSortDirection::Desc { "DESC" } else { "ASC" };
                        format!("{} {}", f, dir)
                    })
                    .collect();
                order_clause = format!(" ORDER BY {}", parts.join(", "));
            }
        }
        boundary_sorts = sorts;

        // The total: exact in page mode (today's behaviour), the planner's
        // estimate when asked, nothing on a cursor walk that did not ask.
        let (total, count_mode) = if query_filter.estimate_total {
            (
                self.estimate_filtered_rows(&where_clause, &filter_params).await?,
                "estimate",
            )
        } else if cursor_walk {
            (0u64, "none")
        } else {
            let count_query = format!("SELECT COUNT(*) FROM {}{}", self.table_name, where_clause);
            let mut count_query_builder = sqlx::query_scalar::<_, i64>(&count_query);
            for param in &filter_params {
                count_query_builder = count_query_builder.bind(param);
            }
            (
                crate::company_scope::fetch_one_scalar_scoped(&self.pool, count_query_builder)
                    .await? as u64,
                "exact",
            )
        };

        // The page: limit+1 rows so has_more is known without a count. The
        // extra row is truncated away before returning.
        let fetch = limit + 1;
        let data_query = if cursor_walk {
            format!(
                "SELECT * FROM {}{}{} LIMIT {}",
                self.table_name, where_clause, order_clause, fetch
            )
        } else {
            format!(
                "SELECT * FROM {}{}{} LIMIT {} OFFSET {}",
                self.table_name,
                where_clause,
                order_clause,
                fetch,
                pagination.offset()
            )
        };

        let mut pagination_info = PaginationInfo::new(pagination.page, pagination.per_page, total);
        pagination_info.count_mode = count_mode.to_string();

        // One fetch path for both modes: untyped rows, decoded per-entity
        // through FromRow exactly as query_as would (decimals keep their
        // scale), with the boundary values read for the cursors off the same
        // rows — no second query, no JSON round-trip.
        let mut rows_query = sqlx::query(&data_query);
        for param in &filter_params {
            rows_query = rows_query.bind(param);
        }
        let rows: Vec<PgRow> =
            crate::company_scope::fetch_all_rows_scoped(&self.pool, rows_query).await?;
        let has_more = rows.len() as i64 > limit;
        let mut page: Vec<PgRow> = rows.into_iter().take(limit as usize).collect();
        if backwards {
            page.reverse();
        }
        let data: anyhow::Result<Vec<T>> = page
            .iter()
            .map(|row| T::from_row(row).map_err(|e| anyhow::anyhow!("decode row: {e}")))
            .collect();
        let data = data?;

        // Cursors from the boundary rows' own values, whenever the order is
        // deterministic. A NULL in a sort column cannot key a position, so
        // that side's cursor is omitted.
        let deterministic = !boundary_sorts.is_empty();
        let next_cursor = if (has_more || backwards) && deterministic {
            page.last().and_then(|r| {
                let casts = if boundary_casts.is_empty() {
                    // Page mode never resolved casts; the encode does not
                    // need them, only the walk does.
                    &boundary_casts
                } else {
                    &boundary_casts
                };
                self.row_cursor(r, &boundary_sorts, casts)
            })
        } else {
            None
        };
        let prev_cursor = if !page.is_empty() && deterministic {
            page.first().and_then(|r| self.row_cursor(r, &boundary_sorts, &boundary_casts))
        } else {
            None
        };

        pagination_info.has_more = Some(has_more);
        pagination_info.next_cursor = next_cursor;
        pagination_info.prev_cursor = prev_cursor;

        Ok(PaginatedResult {
            data,
            pagination: pagination_info,
        })
    }

    /// The planner's row estimate for a filtered read, from EXPLAIN. The
    /// exact figure stays the separate count endpoint; this exists because
    /// counting a hot filtered set is a scan of all of it.
    async fn estimate_filtered_rows(
        &self,
        where_clause: &str,
        filter_params: &[String],
    ) -> anyhow::Result<u64> {
        let explain = format!("EXPLAIN (FORMAT JSON) SELECT 1 FROM {}{}", self.table_name, where_clause);
        let mut builder = sqlx::query_scalar::<_, serde_json::Value>(&explain);
        for param in filter_params {
            builder = builder.bind(param);
        }
        let plan: serde_json::Value =
            crate::company_scope::fetch_one_scalar_scoped(&self.pool, builder).await?;
        let rows = plan
            .as_array()
            .and_then(|a| a.first())
            .and_then(|top| top.get("Plan"))
            .and_then(|p| p.get("Plan Rows"))
            .and_then(|r| r.as_i64())
            .unwrap_or(0);
        Ok(rows.max(0) as u64)
    }

    /// The SQL cast suffix for each sort column, from the table's real
    /// columns (never a cached hint list — the aggregate lesson). The
    /// placeholder is cast to the column's type so the bind compares
    /// against the column without coercing the column itself.
    async fn sort_column_casts(
        &self,
        sorts: &[(String, FilterSortDirection)],
    ) -> anyhow::Result<Vec<Option<String>>> {
        let (schema, table) = match self.table_name.rsplit_once('.') {
            Some((s, t)) => (s.to_string(), t.to_string()),
            None => ("public".to_string(), self.table_name.clone()),
        };
        let mut casts: Vec<Option<String>> = Vec::with_capacity(sorts.len());
        for (field, _) in sorts {
            let row: Option<(String, String)> = sqlx::query_as(
                "SELECT data_type, coalesce(udt_name, '') FROM information_schema.columns \
                  WHERE table_schema = $1 AND table_name = $2 AND column_name = $3",
            )
            .bind(&schema)
            .bind(&table)
            .bind(field)
            .fetch_optional(&self.pool)
            .await?;
            let cast = row.map(|(data_type, udt)| cast_suffix(&data_type, &udt)).flatten();
            casts.push(cast);
        }
        Ok(casts)
    }

    /// One boundary row's cursor: its values for the sort columns (typed
    /// exactly as sqlx decodes them, so a decimal keeps its scale) plus its
    /// id. NULL in any sort column yields None: a null has no position.
    fn row_cursor(
        &self,
        row: &PgRow,
        sorts: &[(String, FilterSortDirection)],
        casts: &[Option<String>],
    ) -> Option<String> {
        let id: uuid::Uuid = row.try_get("id").ok()?;
        let mut values: Vec<serde_json::Value> = Vec::with_capacity(sorts.len());
        for (i, (field, _)) in sorts.iter().enumerate() {
            let field = field.as_str();
            let mut data_type = casts.get(i).and_then(|c| c.as_deref()).unwrap_or("");
            // The ORM appends the id tiebreaker itself and page mode never
            // resolves casts — without this, the uuid id decodes as text,
            // the read fails, and the whole cursor silently vanishes.
            if field == "id" && data_type.is_empty() {
                data_type = "uuid";
            }
            let text: Option<String> = match data_type {
                "numeric" => row
                    .try_get::<Option<sqlx::types::Decimal>, _>(field)
                    .ok()?
                    .map(|d| d.to_string()),
                "uuid" => row
                    .try_get::<Option<uuid::Uuid>, _>(field)
                    .ok()?
                    .map(|u| u.to_string()),
                "timestamptz" => row
                    .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>(field)
                    .ok()?
                    .map(|t| t.to_rfc3339()),
                "integer" | "smallint" => row
                    .try_get::<Option<i32>, _>(field)
                    .ok()?
                    .map(|n| n.to_string()),
                "bigint" => row
                    .try_get::<Option<i64>, _>(field)
                    .ok()?
                    .map(|n| n.to_string()),
                "boolean" => row
                    .try_get::<Option<bool>, _>(field)
                    .ok()?
                    .map(|b| b.to_string()),
                "date" => row
                    .try_get::<Option<chrono::NaiveDate>, _>(field)
                    .ok()?
                    .map(|d| d.to_string()),
                _ => row
                    .try_get::<Option<String>, _>(field)
                    .ok()?
                    .filter(|s| !s.is_empty() || data_type.is_empty()),
            };
            values.push(serde_json::Value::String(text?));
        }
        crate::filter::cursor::encode_cursor(sorts, &values, &id.to_string()).ok()
    }
}

/// The placeholder cast for a column type, or None when a bare text bind
/// compares correctly.
fn cast_suffix(data_type: &str, udt_name: &str) -> Option<String> {
    match data_type {
        "uuid" => Some("uuid".into()),
        "numeric" => Some("numeric".into()),
        "integer" => Some("integer".into()),
        "smallint" => Some("smallint".into()),
        "bigint" => Some("bigint".into()),
        "boolean" => Some("boolean".into()),
        "date" => Some("date".into()),
        "timestamp with time zone" => Some("timestamptz".into()),
        "timestamp without time zone" => Some("timestamp".into()),
        // An enum: bind text, cast to the enum's own name so the comparison
        // runs in the enum's ordering.
        "USER-DEFINED" if !udt_name.is_empty() => Some(udt_name.to_string()),
        _ => None,
    }
}

/// Turn "column ... does not exist" into a sentence that names the cause.
///
/// The insert names the columns the entity serializes. A field that is serialized but is not a
/// column of the table used to vanish quietly, because selecting every column of the row type threw
/// unknown keys away; now it fails, and the bare Postgres error does not say why. Serialized field
/// and table column are meant to be the same set — the update path has always assumed it — so this
/// points at the mismatch rather than leaving someone to guess.
fn explain_unknown_column(error: sqlx::Error, table: &str) -> anyhow::Error {
    let text = error.to_string();
    if text.contains("does not exist") && text.contains("column") {
        return anyhow::Error::new(error).context(format!(
            "insert into {table} named a column that does not exist: the entity serializes a field \
             with no matching column. Every serialized field must be a column of the table (rename \
             it, map it with #[serde(rename)], or skip it with #[serde(skip)])"
        ));
    }
    anyhow::Error::new(error)
}

/// Quote a column name as a SQL identifier.
///
/// Column names here come from serializing the caller's entity, so they are Rust field names in
/// practice — but they are interpolated into DDL/DML, where Postgres has no bind parameter for an
/// identifier. Doubling an embedded quote is the identifier escape, so a name can never end the
/// quoted section early.
fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

#[async_trait]
impl<T> DatabaseOperations<T> for PostgresRepository<T>
where
    T: for<'a> FromRow<'a, PgRow> + Send + Sync + Unpin + Serialize,
{
    async fn create(&self, entity: &T) -> anyhow::Result<T> {
        // Serialize entity to JSON to extract field names and values
        let json_value = serde_json::to_value(entity)?;

        let json_obj = match json_value {
            Value::Object(obj) => obj,
            _ => return Err(anyhow::anyhow!("Entity must serialize to a JSON object")),
        };

        // Build dynamic INSERT query using jsonb_populate_record
        // This approach handles all PostgreSQL types correctly including ENUMs and booleans
        let json_str = serde_json::to_string(&json_obj)?;

        // Name only the columns the payload actually carries.
        //
        // `SELECT (jsonb_populate_record(...)).*` emits EVERY column of the row type, so a column
        // the entity does not know about arrived as an explicit NULL — and an explicit NULL is not
        // an absent value: it overrides the column DEFAULT. That is invisible until a table's
        // correctness depends on a default, which is exactly what composition-installed tenancy
        // does (a scoped table defaults `org_unit_id` from the acting unit), so generic creates
        // over such a table wrote NULL and were refused by the write-path guard.
        //
        // Listing the payload's own keys leaves every other column unmentioned, so its default
        // applies. A key that is present with a JSON null is still written as NULL, which is
        // right: the caller said so. This mirrors the update path below, which has always built
        // its column list from these same keys.
        let insert_columns: Vec<String> = json_obj.keys().map(|k| quote_ident(k)).collect();

        let query = if insert_columns.is_empty() {
            // Nothing supplied at all: let every column take its default rather than emitting
            // `INSERT INTO t () SELECT`, which is not valid SQL.
            format!("INSERT INTO {table} DEFAULT VALUES RETURNING *", table = self.table_name)
        } else {
            let columns = insert_columns.join(", ");
            format!(
                r#"
            INSERT INTO {table} ({columns})
            SELECT {columns} FROM jsonb_populate_record(NULL::{table}, $1::jsonb)
            RETURNING *
            "#,
                table = self.table_name,
                columns = columns
            )
        };

        // The DEFAULT VALUES form takes no bind; every other form binds the payload.
        let statement = if insert_columns.is_empty() {
            sqlx::query_as::<_, T>(&query)
        } else {
            sqlx::query_as::<_, T>(&query).bind(&json_str)
        };
        let result = crate::company_scope::fetch_one_scoped(&self.pool, statement)
            .await
            .map_err(|e| explain_unknown_column(e, &self.table_name))?;

        Ok(result)
    }

    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<T>> {
        // Cast text to UUID for PostgreSQL UUID columns
        let query = format!("SELECT * FROM {} WHERE id = $1::uuid", self.table_name);
        let result = crate::company_scope::fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<Postgres, T>(&query).bind(id),
        )
        .await?;
        Ok(result)
    }

    async fn find_all(&self) -> anyhow::Result<Vec<T>> {
        let query = format!("SELECT * FROM {}", self.table_name);
        let results = crate::company_scope::fetch_all_scoped(
            &self.pool,
            sqlx::query_as::<Postgres, T>(&query),
        )
        .await?;
        Ok(results)
    }

    async fn update(&self, id: &str, entity: &T) -> anyhow::Result<Option<T>> {
        // Serialize entity to JSON
        let json_value = serde_json::to_value(entity)?;

        let json_obj = match json_value {
            Value::Object(obj) => obj,
            _ => return Err(anyhow::anyhow!("Entity must serialize to a JSON object")),
        };

        // Build column list for the update (excluding 'id')
        let update_columns: Vec<&String> = json_obj.keys()
            .filter(|k| *k != "id")
            .collect();

        let column_names = update_columns.iter()
            .map(|k| quote_ident(k))
            .collect::<Vec<_>>()
            .join(", ");

        let json_str = serde_json::to_string(&json_obj)?;

        // Use jsonb_populate_record with CTE to get properly typed values
        let query = format!(
            r#"
            WITH new_row AS (
                SELECT (jsonb_populate_record(NULL::{table}, $1::jsonb)).*
            )
            UPDATE {table} AS t
            SET ({columns}) = (SELECT {columns} FROM new_row)
            WHERE t.id = $2::uuid
            RETURNING t.*
            "#,
            table = self.table_name,
            columns = column_names
        );

        let result = crate::company_scope::fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, T>(&query).bind(&json_str).bind(id),
        )
        .await?;

        Ok(result)
    }

    async fn delete(&self, id: &str) -> anyhow::Result<bool> {
        let query = format!("DELETE FROM {} WHERE id = $1::uuid", self.table_name);
        let result = crate::company_scope::execute_scoped(
            &self.pool,
            sqlx::query(&query).bind(id),
        )
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn count(&self) -> anyhow::Result<u64> {
        let query = format!("SELECT COUNT(*) FROM {}", self.table_name);
        let count = crate::company_scope::fetch_one_scalar_scoped(
            &self.pool,
            sqlx::query_scalar::<_, i64>(&query),
        )
        .await? as u64;
        Ok(count)
    }

    async fn exists(&self, id: &str) -> anyhow::Result<bool> {
        let query = format!("SELECT 1 FROM {} WHERE id = $1::uuid LIMIT 1", self.table_name);
        let result = crate::company_scope::fetch_optional_scalar_scoped(
            &self.pool,
            sqlx::query_scalar::<_, i32>(&query).bind(id),
        )
        .await?;
        Ok(result.is_some())
    }

    async fn execute_query(&self, query: &str) -> anyhow::Result<u64> {
        let result = crate::company_scope::execute_scoped(
            &self.pool,
            sqlx::query(query),
        )
        .await?;
        Ok(result.rows_affected())
    }
}

// ─── Aggregation ──────────────────────────────────────────────────────────────

/// Which reduction to apply to a column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AggregateFn {
    Sum,
    Avg,
    Min,
    Max,
}

impl AggregateFn {
    fn sql(self) -> &'static str {
        match self {
            AggregateFn::Sum => "SUM",
            AggregateFn::Avg => "AVG",
            AggregateFn::Min => "MIN",
            AggregateFn::Max => "MAX",
        }
    }

    fn label(self) -> &'static str {
        match self {
            AggregateFn::Sum => "sum",
            AggregateFn::Avg => "avg",
            AggregateFn::Min => "min",
            AggregateFn::Max => "max",
        }
    }

    /// `SUM`/`AVG` on a text column is a type error, not a zero. `MIN`/`MAX`
    /// order any comparable type, so they carry no such restriction.
    fn requires_numeric(self) -> bool {
        matches!(self, AggregateFn::Sum | AggregateFn::Avg)
    }
}

/// What to group by and what to reduce — the parsed form of the query string.
#[derive(Debug, Clone, Default)]
pub struct AggregateSpec {
    /// Column whose distinct values become groups. `None` asks for one total.
    pub group_by: Option<String>,
    /// `(function, column)` pairs, in the order the caller asked for them.
    pub reductions: Vec<(AggregateFn, String)>,
    /// Most groups to return before reporting the answer as truncated.
    pub group_limit: usize,
    /// Carry a label per group: the column on the group column's RELATED
    /// table (resolved through the entity's relation metadata) to show
    /// instead of a bare uuid key. `None` = keys stay as they are.
    pub label_field: Option<String>,
    /// The resolved relation behind the group column — `(target table, the
    /// BASE table's FK column, snake)`, filled by the generic layer from
    /// the entity's `relations()` metadata. Callers never set this.
    pub label_relation: Option<(String, String)>,
}

/// The default ceiling on distinct groups.
///
/// A `group_by` on a uuid or a timestamp yields one group per row, which is a
/// table scan wearing a chart's clothes. Rather than refusing those columns —
/// a list that would be wrong for some schema sooner or later — the answer is
/// capped and the cap is *reported*, so a caller can tell a complete picture
/// from a partial one instead of quietly drawing the wrong one.
pub const DEFAULT_GROUP_LIMIT: usize = 200;

/// One group's numbers. `key` is the group's value; `None` is a real answer —
/// the rows whose group column is null — and is distinct from "no rows".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateGroup {
    pub key: Option<String>,
    /// The group's display label (the related row's label column), when the
    /// caller asked for one and the group column is a relation FK.
    pub label: Option<String>,
    pub count: u64,
    /// Reduction results keyed `"sum:amount"`, carried as strings.
    ///
    /// Postgres `numeric` holds more precision than an IEEE double, and money
    /// columns are exactly where that bites: a tenant large enough for the
    /// total to matter is a tenant large enough to round it. The string is the
    /// exact value Postgres computed; the caller decides how to parse it.
    /// `None` is SQL NULL — no rows contributed — which is not zero.
    pub values: HashMap<String, Option<String>>,
}

/// Groups plus the overall total, computed together.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateResult {
    pub groups: Vec<AggregateGroup>,
    pub total: AggregateGroup,
    /// True when more distinct groups exist than `group_limit` allowed.
    pub truncated: bool,
}

/// A column name rejected by the allow-list, or a reduction that its type
/// cannot answer.
#[derive(Debug, Clone)]
pub struct AggregateFieldError(pub String);

impl std::fmt::Display for AggregateFieldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AggregateFieldError {}

/// True for the Postgres types `SUM`/`AVG` accept.
fn is_numeric_pg_type(pg_type: &str) -> bool {
    let t = pg_type.trim().to_ascii_lowercase();
    let t = t.split('(').next().unwrap_or(&t).trim();
    matches!(
        t,
        "numeric" | "decimal" | "money"
            | "smallint" | "int2" | "integer" | "int" | "int4" | "bigint" | "int8"
            | "real" | "float4" | "double precision" | "float8"
            | "smallserial" | "serial" | "bigserial"
    )
}

/// Read a table's real columns and their types from the catalog.
///
/// `EntityRepoMeta::column_types()` looks like the natural allow-list and is
/// not one: it carries only the columns the filter parser must CAST — uuids and
/// enums — so every numeric column is absent from it, which is exactly the set
/// `sum` and `avg` exist for. The catalog is the only complete and current
/// answer, and it cannot drift from the table the query will actually run
/// against.
async fn catalog_columns(
    pool: &PgPool,
    qualified_table: &str,
) -> anyhow::Result<HashMap<String, String>> {
    let (schema, table) = match qualified_table.split_once('.') {
        Some((s, t)) => (s.to_string(), t.to_string()),
        None => ("public".to_string(), qualified_table.to_string()),
    };
    let q = sqlx::query_as::<Postgres, (String, String)>(
        "SELECT column_name, data_type FROM information_schema.columns
          WHERE table_schema = $1 AND table_name = $2",
    )
    .bind(schema)
    .bind(table);
    let rows = crate::company_scope::fetch_all_scoped(pool, q).await?;
    Ok(rows.into_iter().collect())
}

/// Resolve a caller-supplied column name against the entity's real columns.
///
/// This is the whole defence for the aggregate path. Unlike a filter *value*,
/// which is bound as a parameter, a `group_by` or `sum` column is spliced into
/// the SQL as an identifier — binding cannot protect it. So the name is never
/// escaped or quoted into safety; it is *replaced* by the matching key already
/// present in the entity's declared column map, and a name with no match is
/// refused. Nothing a caller types can reach the query text.
fn resolve_column<'a>(
    name: &str,
    column_types: &'a HashMap<String, String>,
) -> Result<(&'a str, &'a str), AggregateFieldError> {
    column_types
        .get_key_value(name)
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .ok_or_else(|| {
            AggregateFieldError(format!("unknown field `{name}` — not a column of this entity"))
        })
}

impl<T: for<'a> FromRow<'a, PgRow> + Send + Unpin> PostgresRepository<T> {
    /// Group and reduce rows in one statement, under the same filters, the same
    /// soft-delete convention and the same tenancy fence as the list endpoint.
    ///
    /// Groups and the overall total come back from a single `GROUPING SETS`
    /// query, which is what lets a caller draw a chart and its headline from
    /// one reply, and what keeps the two numbers consistent — a separate total
    /// query could observe a different set of rows.
    pub async fn aggregate_with_filters(
        &self,
        spec: &AggregateSpec,
        filters: &HashMap<String, String>,
        column_types: &HashMap<String, String>,
        search_fields: &[&str],
    ) -> anyhow::Result<AggregateResult> {
        let mut query_filter = parse_query_filter(filters, column_types, None)?;
        if !search_fields.is_empty() {
            query_filter.search_fields = search_fields.iter().map(|s| s.to_string()).collect();
        }
        // Grouping replaces row output entirely: paging and ordering describe a
        // page of rows, and there are none.
        query_filter.limit = None;
        query_filter.offset = None;
        let (where_clause, filter_params) = query_filter.build_where_clause();

        // Every identifier below comes from the catalog, never from the caller.
        let columns = catalog_columns(&self.pool, &self.table_name).await?;
        let mut selects: Vec<String> = Vec::new();
        let mut value_keys: Vec<String> = Vec::new();
        for (func, field) in &spec.reductions {
            let (column, pg_type) = resolve_column(field, &columns)?;
            if func.requires_numeric() && !is_numeric_pg_type(pg_type) {
                return Err(AggregateFieldError(format!(
                    "cannot {} `{}`: its type is {} — {} needs a numeric column",
                    func.label(),
                    column,
                    pg_type,
                    func.label()
                ))
                .into());
            }
            let key = format!("{}:{}", func.label(), column);
            // Cast to text in SQL so the exact value Postgres computed is what
            // crosses the wire — see `AggregateGroup::values`.
            selects.push(format!("{}({})::text AS \"{}\"", func.sql(), column, key));
            value_keys.push(key);
        }

        let group_limit = if spec.group_limit == 0 { DEFAULT_GROUP_LIMIT } else { spec.group_limit };
        let reductions = if selects.is_empty() { String::new() } else { format!(", {}", selects.join(", ")) };

        // The group's label: when the caller names one (group_label) and the
        // group column is a relation FK of THIS entity, LEFT JOIN the
        // related table and carry its label column beside the key — the
        // aggregate's equivalent of `?include=`, which has no row to
        // hydrate otherwise.
        let mut label_select = String::new();
        let mut label_join = String::new();
        if let (Some(field), Some(label_field), Some((rel_table, base_fk))) = (
            &spec.group_by,
            &spec.label_field,
            &spec.label_relation,
        ) {
            let _ = field;
            let qualified = qualify_relation_table(&self.table_name, rel_table);
            let rel_columns = catalog_columns(&self.pool, &qualified).await?;
            let (label_col, _) = resolve_column(label_field, &rel_columns).map_err(|_| {
                AggregateFieldError(format!(
                    "cannot label groups by `{label_field}`: the related table `{qualified}` has no such column"
                ))
            })?;
            label_select = format!(", (label_rel.{label_col})::text AS __group_label");
            label_join = format!(
                " LEFT JOIN {qualified} AS label_rel ON label_rel.id IS NOT DISTINCT FROM {base_fk}"
            );
        }

        let sql = match &spec.group_by {
            Some(field) => {
                let (column, _) = resolve_column(field, &columns)?;
                format!(
                    "SELECT GROUPING({column}) AS __is_total, ({column})::text AS __group_key{label_select}, \
                     COUNT(*) AS __count{reductions} \
                     FROM {table}{label_join}{where_clause} \
                     GROUP BY GROUPING SETS (({column}), ()) \
                     ORDER BY __is_total DESC, __count DESC \
                     LIMIT {limit}",
                    column = column,
                    label_select = label_select,
                    label_join = label_join,
                    reductions = reductions,
                    table = self.table_name,
                    where_clause = where_clause,
                    // One total row, the groups themselves, and one more to
                    // detect that a further group existed.
                    limit = group_limit + 2,
                )
            }
            None => format!(
                "SELECT 1 AS __is_total, NULL::text AS __group_key, COUNT(*) AS __count{reductions} \
                 FROM {table}{where_clause}",
                reductions = reductions,
                table = self.table_name,
                where_clause = where_clause,
            ),
        };

        let mut builder = sqlx::query(&sql);
        for param in &filter_params {
            builder = builder.bind(param);
        }
        let rows = crate::company_scope::fetch_all_rows_scoped(&self.pool, builder).await?;

        let read_group = |row: &PgRow| -> AggregateGroup {
            use sqlx::Row as _;
            let mut values = HashMap::with_capacity(value_keys.len());
            for key in &value_keys {
                values.insert(key.clone(), row.try_get::<Option<String>, _>(key.as_str()).ok().flatten());
            }
            AggregateGroup {
                key: row.try_get::<Option<String>, _>("__group_key").ok().flatten(),
                label: row.try_get::<Option<String>, _>("__group_label").ok().flatten(),
                count: row.try_get::<i64, _>("__count").unwrap_or(0).max(0) as u64,
                values,
            }
        };

        use sqlx::Row as _;
        let mut total: Option<AggregateGroup> = None;
        let mut groups: Vec<AggregateGroup> = Vec::new();
        for row in &rows {
            let is_total = row.try_get::<i32, _>("__is_total").unwrap_or(0) == 1;
            if is_total {
                // Ordered first, so it survives the cap.
                total = Some(read_group(row));
            } else {
                groups.push(read_group(row));
            }
        }

        let truncated = groups.len() > group_limit;
        groups.truncate(group_limit);

        // No rows at all means no total row either: an empty result is a real
        // answer of zero, not a missing one.
        let total = total.unwrap_or_else(|| AggregateGroup {
            key: None,
            label: None,
            count: 0,
            values: value_keys.iter().map(|k| (k.clone(), None)).collect(),
        });

        Ok(AggregateResult { groups, total, truncated })
    }
}

#[cfg(test)]
mod aggregate_field_tests {
    use super::*;

    fn columns() -> HashMap<String, String> {
        [
            ("status", "text"),
            ("total", "numeric"),
            ("qty", "integer"),
            ("notes", "text"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
    }

    #[test]
    fn resolves_only_declared_columns() {
        let cols = columns();
        assert_eq!(resolve_column("total", &cols).unwrap().0, "total");
        assert!(resolve_column("password_hash", &cols).is_err());
    }

    /// The allow-list is the entire defence, because a group/sum column is
    /// spliced into SQL as an identifier and cannot be bound as a parameter.
    #[test]
    fn rejects_injection_attempts_rather_than_escaping_them() {
        let cols = columns();
        for probe in [
            "total) FROM selling.sales_orders; DROP TABLE users --",
            "status\"",
            "1=1",
            "total, (SELECT password FROM users)",
            "",
        ] {
            assert!(
                resolve_column(probe, &cols).is_err(),
                "`{probe}` must be refused, never escaped into the query"
            );
        }
    }

    /// The returned name is the map's own key, not the caller's string, so no
    /// caller-controlled bytes can reach the SQL even on a match.
    #[test]
    fn returns_the_declared_key_not_the_callers_string() {
        let cols = columns();
        let (name, _) = resolve_column("total", &cols).unwrap();
        assert!(std::ptr::eq(name, cols.get_key_value("total").unwrap().0.as_str()));
    }

    #[test]
    fn sum_and_avg_require_a_numeric_type() {
        assert!(AggregateFn::Sum.requires_numeric());
        assert!(AggregateFn::Avg.requires_numeric());
        // Ordering works on any comparable column, so these stay open.
        assert!(!AggregateFn::Min.requires_numeric());
        assert!(!AggregateFn::Max.requires_numeric());
    }

    #[test]
    fn recognises_the_numeric_postgres_types() {
        for t in ["numeric", "NUMERIC(14,2)", "integer", "bigint", "double precision", "money"] {
            assert!(is_numeric_pg_type(t), "{t} should count as numeric");
        }
        for t in ["text", "uuid", "timestamptz", "boolean", "jsonb", "USER-DEFINED"] {
            assert!(!is_numeric_pg_type(t), "{t} must not accept a SUM");
        }
    }
}
