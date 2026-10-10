//! Filter condition: a single WHERE clause component

use super::types::{FilterOperator, FilterValue, FilterLogical};

/// A single filter condition
#[derive(Debug, Clone)]
pub struct FilterCondition {
    pub field: String,
    pub operator: FilterOperator,
    pub value: FilterValue,
    pub logical: FilterLogical,  // AND or OR
    pub column_type: Option<String>,  // For casting (e.g., "user_status")
}

impl FilterCondition {
    /// Create a new filter condition
    pub fn new(field: String, operator: FilterOperator, value: FilterValue) -> Self {
        Self {
            field,
            operator,
            value,
            logical: FilterLogical::And,
            column_type: None,
        }
    }

    /// True when this condition binds a value it compares with `=`, `<`, `IN`, `BETWEEN` and the
    /// like, yet carries no cast for it. Such a value is bound as text, and PostgreSQL has no
    /// implicit comparison between text and a boolean, number, uuid, date or timestamp column, so
    /// the caller must find the column's type before the query can run. Pattern operators (`LIKE`,
    /// `contain`, …) and null checks are text matches or bind nothing, so they never need one.
    pub fn needs_value_cast(&self) -> bool {
        self.column_type.is_none()
            && matches!(
                self.operator,
                FilterOperator::Equal
                    | FilterOperator::NotEqual
                    | FilterOperator::GreaterThan
                    | FilterOperator::GreaterThanOrEqual
                    | FilterOperator::LessThan
                    | FilterOperator::LessThanOrEqual
                    | FilterOperator::In
                    | FilterOperator::NotIn
                    | FilterOperator::Between
                    | FilterOperator::NotBetween
            )
    }

    /// Whether this condition's cast names a user-defined type without its schema (an enum such
    /// as `task_status` from a generated hint). Such a name only resolves when the type's schema
    /// is on the search path, which a tenant database need not have; the caller qualifies it from
    /// the catalog before the query runs.
    pub fn needs_type_qualification(&self) -> bool {
        self.column_type
            .as_deref()
            .is_some_and(|t| !t.contains('.') && !is_builtin_type(t))
    }

    /// Set the logical operator
    pub fn with_logical(mut self, logical: FilterLogical) -> Self {
        self.logical = logical;
        self
    }

    /// Set the column type for casting (e.g., "user_status")
    pub fn with_column_type(mut self, column_type: String) -> Self {
        self.column_type = Some(column_type);
        self
    }

    /// Build SQL WHERE clause for this condition (without logical prefix)
    /// Cast each IN-list placeholder when a column type hint is present:
    /// the wire binds text, and typed columns refuse text comparisons.
    fn cast_placeholders(placeholders: Vec<String>, column_type: &Option<String>) -> String {
        match column_type {
            Some(t) => placeholders
                .iter()
                .map(|p| format!("{p}::{t}"))
                .collect::<Vec<_>>()
                .join(", "),
            None => placeholders.join(", "),
        }
    }

    pub(crate) fn build_sql_without_prefix(&self, param_idx: &mut usize) -> String {
        match &self.operator {
            FilterOperator::IsNull => {
                format!("{} IS NULL", self.field)
            }
            FilterOperator::IsNotNull => {
                format!("{} IS NOT NULL", self.field)
            }
            FilterOperator::In => {
                let placeholders: Vec<String> = match &self.value {
                    FilterValue::Multiple(values) => {
                        values.iter().map(|_| {
                            let p = format!("${}", param_idx);
                            *param_idx += 1;
                            p
                        }).collect()
                    }
                    _ => vec![format!("${}", {
                        let p = *param_idx;
                        *param_idx += 1;
                        p
                    })],
                };
                let list = Self::cast_placeholders(placeholders, &self.column_type);
                format!("{} {} ({})", self.field, self.operator.as_sql(), list)
            }
            FilterOperator::NotIn => {
                let placeholders: Vec<String> = match &self.value {
                    FilterValue::Multiple(values) => {
                        values.iter().map(|_| {
                            let p = format!("${}", param_idx);
                            *param_idx += 1;
                            p
                        }).collect()
                    }
                    _ => vec![format!("${}", {
                        let p = *param_idx;
                        *param_idx += 1;
                        p
                    })],
                };
                let list = Self::cast_placeholders(placeholders, &self.column_type);
                format!("{} {} ({})", self.field, self.operator.as_sql(), list)
            }
            FilterOperator::Between | FilterOperator::NotBetween => {
                let cast = self.column_type.as_ref().map(|t| format!("::{t}")).unwrap_or_default();
                let result = format!(
                    "{} {} ${}{cast} AND ${}{cast}",
                    self.field,
                    self.operator.as_sql(),
                    *param_idx,
                    *param_idx + 1
                );
                *param_idx += 2;
                result
            }
            FilterOperator::Contains => {
                let result = format!("{} ILIKE ${}", self.field, param_idx);
                *param_idx += 1;
                result
            }
            FilterOperator::NotContains => {
                let result = format!("{} NOT ILIKE ${}", self.field, param_idx);
                *param_idx += 1;
                result
            }
            FilterOperator::StartsWith => {
                let result = format!("{} ILIKE ${}", self.field, param_idx);
                *param_idx += 1;
                result
            }
            FilterOperator::EndsWith => {
                let result = format!("{} ILIKE ${}", self.field, param_idx);
                *param_idx += 1;
                result
            }
            _ => {
                // Standard operator with type casting if specified
                let result = if let Some(col_type) = &self.column_type {
                    format!("{} {} ${}::{}", self.field, self.operator.as_sql(), param_idx, col_type)
                } else {
                    format!("{} {} ${}", self.field, self.operator.as_sql(), param_idx)
                };
                *param_idx += 1;
                result
            }
        }
    }

    /// Build SQL WHERE clause for this condition (with logical prefix)
    pub fn build_sql(&self, param_idx: &mut usize) -> String {
        let mut sql = String::new();

        // Add logical operator (except for first condition)
        sql.push_str(match self.logical {
            FilterLogical::And => " AND ",
            FilterLogical::Or => " OR ",
        });

        sql.push_str(&self.build_sql_without_prefix(param_idx));
        sql
    }

    /// Get the parameter values for binding
    pub fn get_params(&self) -> Vec<String> {
        match &self.operator {
            FilterOperator::IsNull | FilterOperator::IsNotNull => vec![],
            FilterOperator::In | FilterOperator::NotIn => {
                match &self.value {
                    FilterValue::Multiple(values) => values.clone(),
                    FilterValue::Single(v) => vec![v.clone()],
                    FilterValue::Null => vec![],
                }
            }
            FilterOperator::Between | FilterOperator::NotBetween => {
                match &self.value {
                    FilterValue::Multiple(v) if v.len() >= 2 => vec![v[0].clone(), v[1].clone()],
                    _ => vec![],
                }
            }
            FilterOperator::Contains | FilterOperator::NotContains => {
                match &self.value {
                    FilterValue::Single(v) => vec![format!("%{}%", v)],
                    FilterValue::Multiple(v) => v.iter().map(|s| format!("%{}%", s)).collect(),
                    FilterValue::Null => vec!["%%".to_string()],
                }
            }
            FilterOperator::StartsWith => {
                match &self.value {
                    FilterValue::Single(v) => vec![format!("{}%", v)],
                    FilterValue::Multiple(v) => v.iter().map(|s| format!("{}%", s)).collect(),
                    FilterValue::Null => vec!["%".to_string()],
                }
            }
            FilterOperator::EndsWith => {
                match &self.value {
                    FilterValue::Single(v) => vec![format!("%{}", v)],
                    FilterValue::Multiple(v) => v.iter().map(|s| format!("%{}", s)).collect(),
                    FilterValue::Null => vec!["%".to_string()],
                }
            }
            _ => {
                match &self.value {
                    FilterValue::Single(v) => vec![v.clone()],
                    FilterValue::Multiple(v) => vec![v.first().cloned().unwrap_or_default()],
                    FilterValue::Null => vec![],
                }
            }
        }
    }
}

#[cfg(test)]
mod in_cast_tests {
    use super::*;
    use crate::filter::types::{FilterLogical, FilterValue};

    fn cond(op: FilterOperator, ct: Option<&str>) -> FilterCondition {
        let mut c = FilterCondition::new(
            "employee_id".into(),
            op,
            FilterValue::from_string("x".into(), false),
        )
        .with_logical(FilterLogical::And);
        if let Some(t) = ct {
            c = c.with_column_type(t.into());
        }
        c
    }

    #[test]
    fn in_placeholders_carry_the_column_cast() {
        let c = cond(FilterOperator::In, Some("uuid"));
        let mut idx = 1;
        let sql = c.build_sql_without_prefix(&mut idx);
        assert!(sql.contains("IN ($1::uuid)"), "{sql}");
    }

    #[test]
    fn between_bounds_carry_the_column_cast() {
        let c = cond(FilterOperator::Between, Some("timestamptz"));
        let mut idx = 3;
        let sql = c.build_sql_without_prefix(&mut idx);
        assert_eq!(sql, "employee_id BETWEEN $3::timestamptz AND $4::timestamptz");
        assert_eq!(idx, 5);
        let c = cond(FilterOperator::NotBetween, None);
        let mut idx = 1;
        assert_eq!(c.build_sql_without_prefix(&mut idx), "employee_id NOT BETWEEN $1 AND $2");
    }

    #[test]
    fn only_uncast_comparisons_ask_for_a_cast() {
        assert!(cond(FilterOperator::Equal, None).needs_value_cast());
        assert!(cond(FilterOperator::In, None).needs_value_cast());
        assert!(cond(FilterOperator::Between, None).needs_value_cast());
        assert!(!cond(FilterOperator::Equal, Some("uuid")).needs_value_cast());
        assert!(!cond(FilterOperator::Contains, None).needs_value_cast());
        assert!(!cond(FilterOperator::IsNull, None).needs_value_cast());
    }

    #[test]
    fn in_placeholders_bare_without_a_hint() {
        let c = cond(FilterOperator::In, None);
        let mut idx = 1;
        let sql = c.build_sql_without_prefix(&mut idx);
        assert!(sql.contains("IN ($1)") && !sql.contains("::"), "{sql}");
    }
}

/// A type PostgreSQL resolves anywhere, with no schema: the casts filters use for built-in
/// columns. Anything else named bare is a user-defined type that needs its schema.
pub(crate) fn is_builtin_type(type_name: &str) -> bool {
    matches!(
        type_name.trim().to_ascii_lowercase().as_str(),
        "uuid"
            | "boolean"
            | "bool"
            | "smallint"
            | "int2"
            | "integer"
            | "int"
            | "int4"
            | "bigint"
            | "int8"
            | "numeric"
            | "decimal"
            | "real"
            | "float4"
            | "double precision"
            | "float8"
            | "date"
            | "interval"
            | "inet"
            | "cidr"
            | "macaddr"
            | "time"
            | "timetz"
            | "timestamp"
            | "timestamptz"
            | "text"
            | "varchar"
            | "character varying"
            | "char"
            | "character"
            | "json"
            | "jsonb"
            | "bytea"
    )
}
