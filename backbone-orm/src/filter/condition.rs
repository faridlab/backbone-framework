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
            FilterOperator::Between => {
                let result = format!("{} BETWEEN ${} AND ${}", self.field, *param_idx, *param_idx + 1);
                *param_idx += 2;
                result
            }
            FilterOperator::NotBetween => {
                let result = format!("{} NOT BETWEEN ${} AND ${}", self.field, *param_idx, *param_idx + 1);
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
    fn in_placeholders_bare_without_a_hint() {
        let c = cond(FilterOperator::In, None);
        let mut idx = 1;
        let sql = c.build_sql_without_prefix(&mut idx);
        assert!(sql.contains("IN ($1)") && !sql.contains("::"), "{sql}");
    }
}
