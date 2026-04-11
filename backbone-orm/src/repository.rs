//! Repository implementations for PostgreSQL with comprehensive CRUD operations

use async_trait::async_trait;
use sqlx::{PgPool, FromRow, postgres::PgRow, Postgres};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use chrono::NaiveDateTime;
use std::collections::{HashMap, HashSet};

use crate::filter::{parse_filters as parse_query_filter};

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
}

impl PaginationInfo {
    pub fn new(page: u32, per_page: u32, total: u64) -> Self {
        let total_pages = ((total as f64) / (per_page as f64)).ceil() as u32;
        Self {
            page,
            per_page,
            total,
            total_pages,
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

        // Apply pagination
        let offset = pagination.offset();
        let limit = pagination.limit();
        query_filter.limit = Some(limit);
        query_filter.offset = Some(offset);

        // Build WHERE clause and collect parameters
        let (where_clause, filter_params) = query_filter.build_where_clause();
        let order_clause = query_filter.build_order_by_clause();

        // Count query
        let count_query = format!(
            "SELECT COUNT(*) FROM {}{}",
            self.table_name,
            where_clause
        );

        let mut count_query_builder = sqlx::query_scalar::<_, i64>(&count_query);
        for param in &filter_params {
            count_query_builder = count_query_builder.bind(param);
        }
        let total = count_query_builder.fetch_one(&self.pool).await? as u64;

        // Data query
        let data_query = format!(
            "SELECT * FROM {}{}{} LIMIT {} OFFSET {}",
            self.table_name,
            where_clause,
            order_clause,
            limit,
            offset
        );

        let mut data_query_builder = sqlx::query_as::<Postgres, T>(&data_query);
        for param in &filter_params {
            data_query_builder = data_query_builder.bind(param);
        }

        let data = data_query_builder.fetch_all(&self.pool).await?;

        Ok(PaginatedResult {
            data,
            pagination: PaginationInfo::new(pagination.page, pagination.per_page, total),
        })
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

        // Apply pagination
        let offset = pagination.offset();
        let limit = pagination.limit();
        query_filter.limit = Some(limit);
        query_filter.offset = Some(offset);

        // Build WHERE clause and collect parameters
        let (where_clause, filter_params) = query_filter.build_where_clause();
        let order_clause = query_filter.build_order_by_clause();

        // Count query
        let count_query = format!(
            "SELECT COUNT(*) FROM {}{}",
            self.table_name,
            where_clause
        );

        let mut count_query_builder = sqlx::query_scalar::<_, i64>(&count_query);
        for param in &filter_params {
            count_query_builder = count_query_builder.bind(param);
        }
        let total = count_query_builder.fetch_one(&self.pool).await? as u64;

        // Data query
        let data_query = format!(
            "SELECT * FROM {}{}{} LIMIT {} OFFSET {}",
            self.table_name,
            where_clause,
            order_clause,
            limit,
            offset
        );

        let mut data_query_builder = sqlx::query_as::<Postgres, T>(&data_query);
        for param in &filter_params {
            data_query_builder = data_query_builder.bind(param);
        }

        let data = data_query_builder.fetch_all(&self.pool).await?;

        Ok(PaginatedResult {
            data,
            pagination: PaginationInfo::new(pagination.page, pagination.per_page, total),
        })
    }
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

        // Use jsonb_populate_record which properly handles type conversions
        // by using the table's row type as a template
        let query = format!(
            r#"
            INSERT INTO {table}
            SELECT (jsonb_populate_record(NULL::{table}, $1::jsonb)).*
            RETURNING *
            "#,
            table = self.table_name
        );

        let result = sqlx::query_as::<_, T>(&query)
            .bind(&json_str)
            .fetch_one(&self.pool)
            .await?;

        Ok(result)
    }

    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<T>> {
        // Cast text to UUID for PostgreSQL UUID columns
        let query = format!("SELECT * FROM {} WHERE id = $1::uuid", self.table_name);
        let result = sqlx::query_as::<Postgres, T>(&query)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(result)
    }

    async fn find_all(&self) -> anyhow::Result<Vec<T>> {
        let query = format!("SELECT * FROM {}", self.table_name);
        let results = sqlx::query_as::<Postgres, T>(&query)
            .fetch_all(&self.pool)
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
            .map(|k| format!("\"{}\"", k))
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

        let result = sqlx::query_as::<_, T>(&query)
            .bind(&json_str)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(result)
    }

    async fn delete(&self, id: &str) -> anyhow::Result<bool> {
        let query = format!("DELETE FROM {} WHERE id = $1::uuid", self.table_name);
        let result = sqlx::query(&query)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn count(&self) -> anyhow::Result<u64> {
        let query = format!("SELECT COUNT(*) FROM {}", self.table_name);
        let count = sqlx::query_scalar::<_, i64>(&query)
            .fetch_one(&self.pool)
            .await? as u64;
        Ok(count)
    }

    async fn exists(&self, id: &str) -> anyhow::Result<bool> {
        let query = format!("SELECT 1 FROM {} WHERE id = $1::uuid LIMIT 1", self.table_name);
        let result = sqlx::query_scalar::<_, i32>(&query)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(result.is_some())
    }

    async fn execute_query(&self, query: &str) -> anyhow::Result<u64> {
        let result = sqlx::query(query)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }
}
