//! Query builder for PostgreSQL with parameterized queries

use sqlx::{PgPool, FromRow, Postgres};
use super::raw_query::{JoinType, JoinClause};

/// Query parameter values
#[derive(Debug, Clone)]
pub enum QueryValue {
    Text(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Uuid(uuid::Uuid),
    Timestamp(chrono::NaiveDateTime),
    Null,
}

/// SQL query builder with parameterized queries and JOIN support
pub struct QueryBuilder {
    table: String,
    fields: Vec<String>,
    joins: Vec<JoinClause>,
    conditions: Vec<(String, QueryValue)>,
    order_by: Vec<String>,
    limit: Option<u32>,
    offset: Option<u32>,
    next_param_id: usize,
}

impl QueryBuilder {
    pub fn new(table: &str) -> Self {
        Self {
            table: table.to_string(),
            fields: vec!["*".to_string()],
            joins: Vec::new(),
            conditions: Vec::new(),
            order_by: Vec::new(),
            limit: None,
            offset: None,
            next_param_id: 1,
        }
    }

    /// Select specific fields
    pub fn select(mut self, fields: &[&str]) -> Self {
        self.fields = fields.iter().map(|f| f.to_string()).collect();
        self
    }

    /// Add WHERE field = ? condition (parameterized)
    pub fn where_eq(mut self, field: &str, value: QueryValue) -> Self {
        self.conditions.push((format!("{} = ${}", field, self.next_param_id), value));
        self.next_param_id += 1;
        self
    }

    /// Add WHERE field != ? condition (parameterized)
    pub fn where_ne(mut self, field: &str, value: QueryValue) -> Self {
        self.conditions.push((format!("{} != ${}", field, self.next_param_id), value));
        self.next_param_id += 1;
        self
    }

    /// Add WHERE field > ? condition (parameterized)
    pub fn where_gt(mut self, field: &str, value: QueryValue) -> Self {
        self.conditions.push((format!("{} > ${}", field, self.next_param_id), value));
        self.next_param_id += 1;
        self
    }

    /// Add WHERE field < ? condition (parameterized)
    pub fn where_lt(mut self, field: &str, value: QueryValue) -> Self {
        self.conditions.push((format!("{} < ${}", field, self.next_param_id), value));
        self.next_param_id += 1;
        self
    }

    /// Add WHERE field LIKE ? condition (parameterized)
    pub fn where_like(mut self, field: &str, value: QueryValue) -> Self {
        self.conditions.push((format!("{} LIKE ${}", field, self.next_param_id), value));
        self.next_param_id += 1;
        self
    }

    /// Add WHERE field IN (?) condition (parameterized)
    pub fn where_in(mut self, field: &str, values: Vec<QueryValue>) -> Self {
        if values.is_empty() {
            return self;
        }

        let placeholders: Vec<String> = (0..values.len())
            .map(|i| format!("${}", self.next_param_id + i))
            .collect();

        self.conditions.push((
            format!("{} IN ({})", field, placeholders.join(", ")),
            values[0].clone(), // First value for simplicity (would need list handling)
        ));
        self.next_param_id += values.len();
        self
    }

    /// Add JOIN clause
    pub fn join(mut self, join_type: JoinType, table: &str, on_condition: &str) -> Self {
        self.joins.push(JoinClause {
            join_type,
            table: table.to_string(),
            on_condition: on_condition.to_string(),
            alias: None,
        });
        self
    }

    /// Add JOIN clause with table alias
    pub fn join_alias(mut self, join_type: JoinType, table: &str, alias: &str, on_condition: &str) -> Self {
        self.joins.push(JoinClause {
            join_type,
            table: table.to_string(),
            on_condition: on_condition.to_string(),
            alias: Some(alias.to_string()),
        });
        self
    }

    /// Add raw WHERE condition (for complex conditions)
    pub fn where_raw(mut self, condition: &str, param: QueryValue) -> Self {
        self.conditions.push((condition.to_string(), param));
        self.next_param_id += 1;
        self
    }

    /// Add ORDER BY clause
    pub fn order_by(mut self, field: &str, direction: &str) -> Self {
        let direction = direction.to_uppercase();
        if direction == "ASC" || direction == "DESC" {
            self.order_by.push(format!("{} {}", field, direction));
        } else {
            self.order_by.push(format!("{} ASC", field));
        }
        self
    }

    /// Add LIMIT clause
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Add OFFSET clause
    pub fn offset(mut self, offset: u32) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Build the SQL query string
    pub fn build_sql(&self) -> String {
        let mut query = format!("SELECT {} FROM {}", self.fields.join(", "), self.table);

        // Add JOINs
        for join in &self.joins {
            let join_str = match join.join_type {
                JoinType::Inner => "INNER JOIN",
                JoinType::Left => "LEFT JOIN",
                JoinType::Right => "RIGHT JOIN",
                JoinType::Full => "FULL JOIN",
                JoinType::Cross => "CROSS JOIN",
            };

            query.push(' ');
            query.push_str(join_str);
            query.push(' ');
            query.push_str(&join.table);

            if let Some(alias) = &join.alias {
                query.push_str(" AS ");
                query.push_str(alias);
            }

            query.push_str(" ON ");
            query.push_str(&join.on_condition);
        }

        if !self.conditions.is_empty() {
            let condition_strings: Vec<String> = self.conditions.iter()
                .map(|(condition, _)| condition.clone())
                .collect();
            query.push_str(" WHERE ");
            query.push_str(&condition_strings.join(" AND "));
        }

        if !self.order_by.is_empty() {
            query.push_str(" ORDER BY ");
            query.push_str(&self.order_by.join(", "));
        }

        if let Some(limit) = self.limit {
            query.push_str(&format!(" LIMIT {}", limit));
        }

        if let Some(offset) = self.offset {
            query.push_str(&format!(" OFFSET {}", offset));
        }

        query
    }

    /// Build query with parameters for execution
    pub fn build_query(&self) -> (String, Vec<QueryValue>) {
        let sql = self.build_sql();
        let params: Vec<QueryValue> = self.conditions.iter()
            .map(|(_, value)| value.clone())
            .collect();

        (sql, params)
    }

    /// Execute query and map results to a struct
    pub async fn execute<T>(&self, pool: &PgPool) -> anyhow::Result<Vec<T>>
    where
        T: for<'r> FromRow<'r, sqlx::postgres::PgRow> + Send + Unpin,
    {
        let (sql, params) = self.build_query();

        let mut query = sqlx::query_as::<Postgres, T>(&sql);

        // Bind parameters in order
        for param in params {
            match param {
                QueryValue::Text(val) => query = query.bind(val),
                QueryValue::Integer(val) => query = query.bind(val),
                QueryValue::Float(val) => query = query.bind(val),
                QueryValue::Boolean(val) => query = query.bind(val),
                QueryValue::Uuid(val) => query = query.bind(val),
                QueryValue::Timestamp(val) => query = query.bind(val),
                QueryValue::Null => query = query.bind::<Option<String>>(None),
            }
        }

        let results = query.fetch_all(pool).await?;
        Ok(results)
    }

    /// Execute query and return first result
    pub async fn execute_first<T>(&self, pool: &PgPool) -> anyhow::Result<Option<T>>
    where
        T: for<'r> FromRow<'r, sqlx::postgres::PgRow> + Send + Unpin,
    {
        let (sql, params) = self.build_query();

        let mut query = sqlx::query_as::<Postgres, T>(&sql);

        // Bind parameters in order
        for param in params {
            match param {
                QueryValue::Text(val) => query = query.bind(val),
                QueryValue::Integer(val) => query = query.bind(val),
                QueryValue::Float(val) => query = query.bind(val),
                QueryValue::Boolean(val) => query = query.bind(val),
                QueryValue::Uuid(val) => query = query.bind(val),
                QueryValue::Timestamp(val) => query = query.bind(val),
                QueryValue::Null => query = query.bind::<Option<String>>(None),
            }
        }

        let result = query.fetch_optional(pool).await?;
        Ok(result)
    }
}

/// Convenience functions for creating query values
impl QueryValue {
    pub fn text<T: Into<String>>(value: T) -> Self {
        QueryValue::Text(value.into())
    }

    pub fn integer(value: i64) -> Self {
        QueryValue::Integer(value)
    }

    pub fn float(value: f64) -> Self {
        QueryValue::Float(value)
    }

    pub fn boolean(value: bool) -> Self {
        QueryValue::Boolean(value)
    }

    pub fn uuid(value: uuid::Uuid) -> Self {
        QueryValue::Uuid(value)
    }

    pub fn timestamp(value: chrono::NaiveDateTime) -> Self {
        QueryValue::Timestamp(value)
    }

    pub fn null() -> Self {
        QueryValue::Null
    }
}