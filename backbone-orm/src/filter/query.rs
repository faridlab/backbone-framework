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
    /// Keyset paging: the opaque cursor from a previous page's
    /// `next_cursor` (forward walk). Takes precedence over page/offset.
    pub cursor_after: Option<String>,
    /// Keyset paging: the opaque cursor from a previous page's
    /// `prev_cursor` (backward walk — the query inverts and the page is
    /// reversed so it still reads forward).
    pub cursor_before: Option<String>,
    /// Replace the exact COUNT (a full scan of the filtered set) with the
    /// planner's row estimate. Screens that need the exact figure use the
    /// separate count endpoint.
    pub estimate_total: bool,
    /// Conditions every query of this filter must satisfy (soft-delete exclusion, the company
    /// fence). They are raw SQL, so only repository code may add them, through
    /// [`QueryFilter::add_base_condition`]; the wire parser never fills this list, and the
    /// field is crate-private so nothing outside the ORM can splice SQL into it.
    pub(crate) base_conditions: Vec<String>,
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

    /// Add a base condition: raw SQL that every row of the result must satisfy.
    ///
    /// Crate-internal on purpose. Base conditions are spliced into the WHERE clause unbound,
    /// so they may only come from repository code (soft-delete guards, the company fence) and
    /// never from a request.
    pub(crate) fn add_base_condition(&mut self, condition: impl Into<String>) {
        self.base_conditions.push(condition.into());
    }

    /// The base conditions this filter carries, in the order they were added.
    pub fn base_conditions(&self) -> &[String] {
        &self.base_conditions
    }

    /// Build the complete WHERE clause.
    ///
    /// The clause is a conjunction of independent groups: each base condition, the search,
    /// and the caller's own conditions. Every group is parenthesised whenever there is more
    /// than one, so an `OR` inside the caller's conditions (or inside a base condition) can
    /// only widen its own group and never escape the soft-delete guard, the company fence or
    /// the search. Parameters are numbered in the order the groups are emitted.
    pub fn build_where_clause(&self) -> (String, Vec<String>) {
        let mut groups: Vec<String> = Vec::new();
        let mut params = Vec::new();
        let mut param_idx = 1usize;

        // Base conditions are repository-authored SQL and carry no parameters.
        for base_condition in &self.base_conditions {
            let trimmed = base_condition.trim();
            if !trimmed.is_empty() {
                groups.push(trimmed.to_string());
            }
        }

        if let Some(search_term) = &self.search {
            if !self.search_fields.is_empty() && !search_term.is_empty() {
                let search_pattern = format!("%{}%", search_term);
                let search_conditions: Vec<String> = self
                    .search_fields
                    .iter()
                    .map(|field| {
                        let condition = format!("{} ILIKE ${}", field, param_idx);
                        param_idx += 1;
                        params.push(search_pattern.clone());
                        condition
                    })
                    .collect();
                groups.push(search_conditions.join(" OR "));
            }
        }

        // The caller's conditions keep their own AND/OR chaining inside one group.
        let mut user = String::new();
        for condition in &self.conditions {
            let condition_sql = condition.build_sql_without_prefix(&mut param_idx);
            params.extend(condition.get_params());
            if !user.is_empty() {
                user.push_str(match condition.logical {
                    FilterLogical::And => " AND ",
                    FilterLogical::Or => " OR ",
                });
            }
            user.push_str(&condition_sql);
        }
        if !user.is_empty() {
            groups.push(user);
        }

        let where_clause = match groups.len() {
            0 => String::new(),
            1 => format!(" WHERE {}", groups[0]),
            _ => format!(
                " WHERE {}",
                groups
                    .iter()
                    .map(|g| format!("({g})"))
                    .collect::<Vec<_>>()
                    .join(" AND ")
            ),
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

    /// True when some condition compares a value that carries no cast — see
    /// [`FilterCondition::needs_value_cast`]. The repository answers it by reading the table's
    /// column types from the catalog before it builds the query.
    pub fn has_uncast_value_conditions(&self) -> bool {
        self.conditions.iter().any(FilterCondition::needs_value_cast)
    }

    /// Whether the table's catalog types must be read before this filter runs: a comparison has
    /// no cast yet, or a cast names a user-defined type without its schema.
    pub fn needs_catalog_types(&self) -> bool {
        self.conditions
            .iter()
            .any(|c| c.needs_value_cast() || c.needs_type_qualification())
    }

    /// Check if has any conditions
    pub fn has_conditions(&self) -> bool {
        !self.conditions.is_empty() || self.search.is_some()
    }
}
