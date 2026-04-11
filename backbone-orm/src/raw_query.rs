//! Raw SQL query builder with parameter binding and advanced SQL features

use anyhow::Result;
use sqlx::{PgPool, FromRow, postgres::PgRow, Postgres};
use crate::query_builder::QueryValue;

/// Raw SQL query builder with parameter binding
pub struct RawQueryBuilder {
    sql: String,
    params: Vec<QueryValue>,
    param_count: usize,
}

impl RawQueryBuilder {
    /// Create a new raw query builder
    pub fn new(sql: &str) -> Self {
        Self {
            sql: sql.to_string(),
            params: Vec::new(),
            param_count: 1,
        }
    }

    /// Add a parameter to the query
    pub fn bind<T: Into<QueryValue>>(mut self, value: T) -> Self {
        self.params.push(value.into());
        self.param_count += 1;
        self
    }

    /// Add multiple parameters
    pub fn bind_many<T: Into<QueryValue>>(mut self, values: Vec<T>) -> Self {
        for value in values {
            self.params.push(value.into());
            self.param_count += 1;
        }
        self
    }

    /// Build the final query with parameter substitution
    pub fn build(self) -> (String, Vec<QueryValue>) {
        let mut sql = self.sql;

        // Replace $1, $2, etc. with actual parameter values in order
        // Note: In a real implementation, we'd let SQLx handle parameter binding
        // This is a simplified version for demonstration
        for (i, param) in self.params.iter().enumerate() {
            let placeholder = format!("${}", i + 1);
            if let Some(pos) = sql.find(&placeholder) {
                let replacement = match param {
                    QueryValue::Text(s) => format!("'{}'", s.replace('\'', "''")),
                    QueryValue::Integer(n) => n.to_string(),
                    QueryValue::Float(f) => f.to_string(),
                    QueryValue::Boolean(b) => b.to_string(),
                    QueryValue::Uuid(u) => format!("'{}'", u),
                    QueryValue::Timestamp(ts) => format!("'{}'", ts),
                    QueryValue::Null => "NULL".to_string(),
                };
                sql.replace_range(pos..pos + placeholder.len(), &replacement);
            }
        }

        (sql, self.params)
    }

    /// Execute the raw query and map results to a struct
    pub async fn execute<T>(self, pool: &PgPool) -> Result<Vec<T>>
    where
        T: for<'r> FromRow<'r, PgRow> + Send + Unpin,
    {
        let (sql, params) = self.build_parameterized();

        let mut query = sqlx::query_as::<Postgres, T>(&sql);

        // Bind parameters in order for SQLx
        for param in params {
            query = match param {
                QueryValue::Text(val) => query.bind(val),
                QueryValue::Integer(val) => query.bind(val),
                QueryValue::Float(val) => query.bind(val),
                QueryValue::Boolean(val) => query.bind(val),
                QueryValue::Uuid(val) => query.bind(val),
                QueryValue::Timestamp(val) => query.bind(val),
                QueryValue::Null => query.bind::<Option<String>>(None),
            };
        }

        let results = query.fetch_all(pool).await?;
        Ok(results)
    }

    /// Execute the raw query and return the first result
    pub async fn execute_first<T>(self, pool: &PgPool) -> Result<Option<T>>
    where
        T: for<'r> FromRow<'r, PgRow> + Send + Unpin,
    {
        let (sql, params) = self.build_parameterized();

        let mut query = sqlx::query_as::<Postgres, T>(&sql);

        // Bind parameters in order for SQLx
        for param in params {
            query = match param {
                QueryValue::Text(val) => query.bind(val),
                QueryValue::Integer(val) => query.bind(val),
                QueryValue::Float(val) => query.bind(val),
                QueryValue::Boolean(val) => query.bind(val),
                QueryValue::Uuid(val) => query.bind(val),
                QueryValue::Timestamp(val) => query.bind(val),
                QueryValue::Null => query.bind::<Option<String>>(None),
            };
        }

        let result = query.fetch_optional(pool).await?;
        Ok(result)
    }

    /// Execute the raw query and return affected row count
    pub async fn execute_raw(self, pool: &PgPool) -> Result<u64> {
        let (sql, params) = self.build_parameterized();

        let mut query = sqlx::query(&sql);

        // Bind parameters in order for SQLx
        for param in params {
            query = match param {
                QueryValue::Text(val) => query.bind(val),
                QueryValue::Integer(val) => query.bind(val),
                QueryValue::Float(val) => query.bind(val),
                QueryValue::Boolean(val) => query.bind(val),
                QueryValue::Uuid(val) => query.bind(val),
                QueryValue::Timestamp(val) => query.bind(val),
                QueryValue::Null => query.bind::<Option<String>>(None),
            };
        }

        let result = query.execute(pool).await?;
        Ok(result.rows_affected())
    }

    /// Build query with parameters for SQLx (keeps parameter placeholders)
    fn build_parameterized(self) -> (String, Vec<QueryValue>) {
        (self.sql, self.params)
    }
}

/// Advanced Query Builder with JOIN support
pub struct AdvancedQueryBuilder {
    base_table: String,
    fields: Vec<String>,
    joins: Vec<JoinClause>,
    conditions: Vec<(String, QueryValue)>,
    group_by: Vec<String>,
    having: Vec<(String, QueryValue)>,
    order_by: Vec<String>,
    limit: Option<u32>,
    offset: Option<u32>,
    ctes: Vec<CteClause>,
    window_functions: Vec<WindowFunction>,
    next_param_id: usize,
}

/// JOIN clause specification
#[derive(Debug, Clone)]
pub struct JoinClause {
    pub join_type: JoinType,
    pub table: String,
    pub on_condition: String,
    pub alias: Option<String>,
}

/// Join types
#[derive(Debug, Clone)]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
    Cross,
}

/// Common Table Expression (CTE) clause
#[derive(Debug, Clone)]
pub struct CteClause {
    pub name: String,
    pub query: String,
}

/// Window function specification
#[derive(Debug, Clone)]
pub struct WindowFunction {
    pub expression: String,
    pub alias: String,
    pub partition_by: Vec<String>,
    pub order_by: Vec<String>,
    pub frame: Option<String>,
}

impl AdvancedQueryBuilder {
    /// Create a new advanced query builder
    pub fn new(base_table: &str) -> Self {
        Self {
            base_table: base_table.to_string(),
            fields: vec!["*".to_string()],
            joins: Vec::new(),
            conditions: Vec::new(),
            group_by: Vec::new(),
            having: Vec::new(),
            order_by: Vec::new(),
            limit: None,
            offset: None,
            ctes: Vec::new(),
            window_functions: Vec::new(),
            next_param_id: 1,
        }
    }

    /// Select specific fields
    pub fn select(mut self, fields: &[&str]) -> Self {
        self.fields = fields.iter().map(|f| f.to_string()).collect();
        self
    }

    /// Add a JOIN clause
    pub fn join(mut self, join_type: JoinType, table: &str, on_condition: &str) -> Self {
        self.joins.push(JoinClause {
            join_type,
            table: table.to_string(),
            on_condition: on_condition.to_string(),
            alias: None,
        });
        self
    }

    /// Add a JOIN clause with alias
    pub fn join_alias(mut self, join_type: JoinType, table: &str, alias: &str, on_condition: &str) -> Self {
        self.joins.push(JoinClause {
            join_type,
            table: table.to_string(),
            on_condition: on_condition.to_string(),
            alias: Some(alias.to_string()),
        });
        self
    }

    /// Add WHERE condition (raw SQL)
    pub fn where_raw(mut self, condition: &str, param: QueryValue) -> Self {
        self.conditions.push((condition.to_string(), param));
        self.next_param_id += 1;
        self
    }

    /// Add GROUP BY clause
    pub fn group_by(mut self, fields: &[&str]) -> Self {
        self.group_by = fields.iter().map(|f| f.to_string()).collect();
        self
    }

    /// Add HAVING condition
    pub fn having(mut self, condition: &str, param: QueryValue) -> Self {
        self.having.push((condition.to_string(), param));
        self.next_param_id += 1;
        self
    }

    /// Add a Common Table Expression (CTE)
    pub fn with_cte(mut self, name: &str, query: &str) -> Self {
        self.ctes.push(CteClause {
            name: name.to_string(),
            query: query.to_string(),
        });
        self
    }

    /// Add a window function
    pub fn window_fn(
        mut self,
        expression: &str,
        alias: &str,
        partition_by: &[&str],
        order_by: &[&str]
    ) -> Self {
        self.window_functions.push(WindowFunction {
            expression: expression.to_string(),
            alias: alias.to_string(),
            partition_by: partition_by.iter().map(|f| f.to_string()).collect(),
            order_by: order_by.iter().map(|f| f.to_string()).collect(),
            frame: None,
        });
        self
    }

    /// Add window function with frame clause
    pub fn window_fn_with_frame(
        mut self,
        expression: &str,
        alias: &str,
        partition_by: &[&str],
        order_by: &[&str],
        frame: &str
    ) -> Self {
        self.window_functions.push(WindowFunction {
            expression: expression.to_string(),
            alias: alias.to_string(),
            partition_by: partition_by.iter().map(|f| f.to_string()).collect(),
            order_by: order_by.iter().map(|f| f.to_string()).collect(),
            frame: Some(frame.to_string()),
        });
        self
    }

    /// Add ORDER BY clause
    pub fn order_by(mut self, field: &str, direction: &str) -> Self {
        self.order_by.push(format!("{} {}", field, direction));
        self
    }

    /// Set LIMIT
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set OFFSET
    pub fn offset(mut self, offset: u32) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Build the complete SQL query
    pub fn build_sql(&self) -> (String, Vec<QueryValue>) {
        let mut sql = String::new();
        let mut params = Vec::new();

        // Add CTEs if any
        if !self.ctes.is_empty() {
            sql.push_str("WITH ");
            let cte_strings: Vec<String> = self.ctes.iter()
                .map(|cte| format!("{} AS ({})", cte.name, cte.query))
                .collect();
            sql.push_str(&cte_strings.join(", "));
            sql.push(' ');
        }

        // Build SELECT clause
        sql.push_str("SELECT ");

        // Add window functions to fields
        let mut all_fields = self.fields.clone();
        for wf in &self.window_functions {
            let mut wf_expr = wf.expression.clone();

            // Check if expression already contains OVER clause
            if wf_expr.contains("OVER") {
                // Expression is already complete, just add alias
                all_fields.push(format!("{} AS {}", wf_expr, wf.alias));
            } else {
                // Build OVER clause dynamically
                if !wf.partition_by.is_empty() {
                    wf_expr = format!("{} OVER (PARTITION BY {}", wf_expr, wf.partition_by.join(", "));
                    if !wf.order_by.is_empty() {
                        wf_expr = format!("{} ORDER BY {}", wf_expr, wf.order_by.join(", "));
                    }

                    // Add frame if present
                    if let Some(frame) = &wf.frame {
                        wf_expr = format!("{} {})", wf_expr, frame);
                    } else {
                        wf_expr = format!("{})", wf_expr);
                    }
                } else if !wf.order_by.is_empty() {
                    wf_expr = format!("{} OVER (ORDER BY {}", wf_expr, wf.order_by.join(", "));
                    if let Some(frame) = &wf.frame {
                        wf_expr = format!("{} {})", wf_expr, frame);
                    } else {
                        wf_expr = format!("{})", wf_expr);
                    }
                }

                all_fields.push(format!("{} AS {}", wf_expr, wf.alias));
            }
        }

        sql.push_str(&all_fields.join(", "));
        sql.push_str(" FROM ");
        sql.push_str(&self.base_table);

        // Add JOINs
        for join in &self.joins {
            let join_str = match join.join_type {
                JoinType::Inner => "INNER JOIN",
                JoinType::Left => "LEFT JOIN",
                JoinType::Right => "RIGHT JOIN",
                JoinType::Full => "FULL JOIN",
                JoinType::Cross => "CROSS JOIN",
            };

            sql.push(' ');
            sql.push_str(join_str);
            sql.push(' ');
            sql.push_str(&join.table);

            if let Some(alias) = &join.alias {
                sql.push_str(" AS ");
                sql.push_str(alias);
            }

            sql.push_str(" ON ");
            sql.push_str(&join.on_condition);
        }

        // Add WHERE conditions
        if !self.conditions.is_empty() {
            sql.push_str(" WHERE ");
            let condition_strings: Vec<String> = self.conditions.iter()
                .map(|(cond, _)| cond.clone())
                .collect();
            sql.push_str(&condition_strings.join(" AND "));

            // Collect parameters
            for (_, param) in &self.conditions {
                params.push(param.clone());
            }
        }

        // Add GROUP BY
        if !self.group_by.is_empty() {
            sql.push_str(" GROUP BY ");
            sql.push_str(&self.group_by.join(", "));
        }

        // Add HAVING
        if !self.having.is_empty() {
            sql.push_str(" HAVING ");
            let having_strings: Vec<String> = self.having.iter()
                .map(|(cond, _)| cond.clone())
                .collect();
            sql.push_str(&having_strings.join(" AND "));

            // Collect parameters
            for (_, param) in &self.having {
                params.push(param.clone());
            }
        }

        // Add ORDER BY
        if !self.order_by.is_empty() {
            sql.push_str(" ORDER BY ");
            sql.push_str(&self.order_by.join(", "));
        }

        // Add LIMIT and OFFSET
        if let Some(limit) = self.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }
        if let Some(offset) = self.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        (sql, params)
    }

    /// Execute the advanced query
    pub async fn execute<T>(self, pool: &PgPool) -> Result<Vec<T>>
    where
        T: for<'r> FromRow<'r, PgRow> + Send + Unpin,
    {
        let (sql, params) = self.build_sql();

        let mut query = sqlx::query_as::<Postgres, T>(&sql);

        // Bind parameters in order
        for param in params {
            query = match param {
                QueryValue::Text(val) => query.bind(val),
                QueryValue::Integer(val) => query.bind(val),
                QueryValue::Float(val) => query.bind(val),
                QueryValue::Boolean(val) => query.bind(val),
                QueryValue::Uuid(val) => query.bind(val),
                QueryValue::Timestamp(val) => query.bind(val),
                QueryValue::Null => query.bind::<Option<String>>(None),
            };
        }

        let results = query.fetch_all(pool).await?;
        Ok(results)
    }

    /// Execute and return first result
    pub async fn execute_first<T>(self, pool: &PgPool) -> Result<Option<T>>
    where
        T: for<'r> FromRow<'r, PgRow> + Send + Unpin,
    {
        let (sql, params) = self.build_sql();

        let mut query = sqlx::query_as::<Postgres, T>(&sql);

        // Bind parameters in order
        for param in params {
            query = match param {
                QueryValue::Text(val) => query.bind(val),
                QueryValue::Integer(val) => query.bind(val),
                QueryValue::Float(val) => query.bind(val),
                QueryValue::Boolean(val) => query.bind(val),
                QueryValue::Uuid(val) => query.bind(val),
                QueryValue::Timestamp(val) => query.bind(val),
                QueryValue::Null => query.bind::<Option<String>>(None),
            };
        }

        let result = query.fetch_optional(pool).await?;
        Ok(result)
    }
}

/// Convenience functions for common raw query patterns
pub struct RawQuery;

impl RawQuery {
    /// Execute a simple scalar query (returns single value)
    pub async fn scalar<T>(pool: &PgPool, sql: &str, params: Vec<QueryValue>) -> Result<T>
    where
        T: for<'r> sqlx::Decode<'r, sqlx::Postgres> + sqlx::Type<sqlx::Postgres> + Send + Unpin,
    {
        let mut query = sqlx::query_scalar::<Postgres, T>(sql);

        for param in params {
            query = match param {
                QueryValue::Text(val) => query.bind(val),
                QueryValue::Integer(val) => query.bind(val),
                QueryValue::Float(val) => query.bind(val),
                QueryValue::Boolean(val) => query.bind(val),
                QueryValue::Uuid(val) => query.bind(val),
                QueryValue::Timestamp(val) => query.bind(val),
                QueryValue::Null => query.bind::<Option<String>>(None),
            };
        }

        let result = query.fetch_one(pool).await?;
        Ok(result)
    }

    /// Execute a query and return multiple values
    pub async fn many<T>(pool: &PgPool, sql: &str, params: Vec<QueryValue>) -> Result<Vec<T>>
    where
        T: for<'r> sqlx::Decode<'r, sqlx::Postgres> + sqlx::Type<sqlx::Postgres> + Send + Unpin,
    {
        let mut query = sqlx::query_scalar::<Postgres, T>(sql);

        for param in params {
            query = match param {
                QueryValue::Text(val) => query.bind(val),
                QueryValue::Integer(val) => query.bind(val),
                QueryValue::Float(val) => query.bind(val),
                QueryValue::Boolean(val) => query.bind(val),
                QueryValue::Uuid(val) => query.bind(val),
                QueryValue::Timestamp(val) => query.bind(val),
                QueryValue::Null => query.bind::<Option<String>>(None),
            };
        }

        let result = query.fetch_all(pool).await?;
        Ok(result)
    }
}