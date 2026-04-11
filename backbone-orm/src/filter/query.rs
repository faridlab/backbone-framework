//! QueryFilter: complete query specification with conditions, sorting, search, and pagination

use super::types::{FilterLogical, SortDirection, SortSpec};
use super::condition::FilterCondition;

/// Complete query filter specification
#[derive(Debug, Clone, Default)]
pub struct QueryFilter {
    pub conditions: Vec<FilterCondition>,
    pub sorts: Vec<SortSpec>,
    pub search: Option<String>,
    pub search_fields: Vec<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub page: Option<u32>,
    /// Base conditions that are always applied (e.g., soft delete exclusion)
    /// These are raw SQL conditions that will be ANDed with other conditions
    pub base_conditions: Vec<String>,
}

impl QueryFilter {
    /// Create a new empty query filter
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a filter condition
    pub fn add_condition(&mut self, condition: FilterCondition) {
        self.conditions.push(condition);
    }

    /// Add a sort specification
    pub fn add_sort(&mut self, sort: SortSpec) {
        self.sorts.push(sort);
    }

    /// Set search term
    pub fn with_search(&mut self, search: String, fields: Vec<String>) {
        self.search = Some(search);
        self.search_fields = fields;
    }

    /// Set pagination
    pub fn with_pagination(&mut self, page: u32, limit: u32) {
        self.page = Some(page);
        self.limit = Some(limit);
    }

    /// Add a base condition (raw SQL that is always ANDed with other conditions)
    /// Used for soft delete exclusion, tenant filtering, etc.
    pub fn add_base_condition(&mut self, condition: String) {
        self.base_conditions.push(condition);
    }

    /// Build the complete WHERE clause
    pub fn build_where_clause(&self) -> (String, Vec<String>) {
        let mut parts = Vec::new();
        let mut params = Vec::new();
        let mut param_idx = 1usize;

        // Add base conditions first (e.g., soft delete exclusion)
        // These are raw SQL that don't require parameter binding
        for base_condition in &self.base_conditions {
            parts.push(base_condition.clone());
        }

        // Add search condition (if specified)
        if let Some(search_term) = &self.search {
            if !self.search_fields.is_empty() && !search_term.is_empty() {
                let search_pattern = format!("%{}%", search_term);
                let search_conditions: Vec<String> = self.search_fields.iter()
                    .map(|field| {
                        let condition = format!("{} ILIKE ${}", field, param_idx);
                        param_idx += 1;
                        params.push(search_pattern.clone());
                        condition
                    })
                    .collect();

                if !search_conditions.is_empty() {
                    if parts.is_empty() {
                        parts.push(format!("({})", search_conditions.join(" OR ")));
                    } else {
                        parts.push(format!(" AND ({})", search_conditions.join(" OR ")));
                    }
                }
            }
        }

        // Add all filter conditions
        for condition in &self.conditions {
            let condition_sql = condition.build_sql_without_prefix(&mut param_idx);
            params.extend(condition.get_params());

            // Prepend logical operator if not the first condition
            let logical_prefix = match condition.logical {
                FilterLogical::And => " AND ",
                FilterLogical::Or => " OR ",
            };

            if !parts.is_empty() {
                parts.push(format!("{}{}", logical_prefix, condition_sql));
            } else {
                parts.push(condition_sql);
            }
        }

        let where_clause = if parts.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", parts.join(""))
        };

        (where_clause, params)
    }

    /// Build ORDER BY clause
    pub fn build_order_by_clause(&self) -> String {
        if self.sorts.is_empty() {
            String::new()
        } else {
            let order_parts: Vec<String> = self.sorts.iter()
                .map(|s| format!("{} {}", s.field, if s.direction == SortDirection::Desc { "DESC" } else { "ASC" }))
                .collect();
            format!(" ORDER BY {}", order_parts.join(", "))
        }
    }

    /// Check if has any conditions
    pub fn has_conditions(&self) -> bool {
        !self.conditions.is_empty() || self.search.is_some()
    }
}
