//! Query parameter parser: converts HTTP query params into QueryFilter

use std::collections::HashMap;
use std::collections::HashSet;
use anyhow::Result;

use super::types::{FilterOperator, FilterValue, FilterLogical, SortDirection, SortSpec};
use super::condition::FilterCondition;
use super::query::QueryFilter;
use super::validation::{is_valid_field, sanitize_field_name};

/// Built-in PostgreSQL types that should NOT have their values normalized.
/// Any column_type not in this list is treated as a custom enum type whose
/// values should be converted from PascalCase/camelCase to snake_case.
const BUILTIN_PG_TYPES: &[&str] = &[
    "uuid", "numeric", "decimal", "integer", "int", "int4", "int8",
    "bigint", "smallint", "int2", "real", "float", "float4", "float8",
    "double precision", "boolean", "bool", "text", "varchar", "char",
    "timestamp", "timestamptz", "date", "time", "timetz",
    "interval", "jsonb", "json", "bytea", "inet", "cidr", "macaddr",
];

/// Check if a column type is a custom enum (not a built-in PostgreSQL type)
pub(crate) fn is_custom_enum_type(col_type: &str) -> bool {
    !BUILTIN_PG_TYPES.contains(&col_type.to_lowercase().as_str())
}

/// Audit-metadata timestamp fields that live inside the `metadata` JSONB
/// column rather than as top-level columns. When a filter targets one of
/// these, the field is rewritten to `(metadata->>'<field>')::timestamptz`
/// so the generated SQL is valid PostgreSQL.
///
/// All entities in the schema-driven migrations follow this convention —
/// timestamps live in `metadata` to keep them out of the entity's typed
/// schema and let triggers manage them uniformly. Without this rewrite,
/// any client sending `?updated_at[gte]=...` (e.g. mobile delta-sync)
/// gets a `column "updated_at" does not exist` 500 error.
const AUDIT_METADATA_FIELDS: &[&str] = &["created_at", "updated_at", "deleted_at"];

/// If `field` is one of the audit-metadata timestamps, return the
/// SQL expression that reads it from the `metadata` JSONB column with
/// a timestamptz cast (so range comparisons work). Otherwise return
/// `None` and the caller should use the field name as-is.
pub(crate) fn audit_metadata_sql_expr(field: &str) -> Option<String> {
    if AUDIT_METADATA_FIELDS.contains(&field) {
        Some(format!("(metadata->>'{}')::timestamptz", field))
    } else {
        None
    }
}

/// Convert PascalCase or camelCase string to snake_case.
/// E.g., "Detergent" → "detergent", "StainRemover" → "stain_remover",
///       "DryCleanChemical" → "dry_clean_chemical"
pub(crate) fn to_snake_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 4);
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(ch.to_lowercase().next().unwrap_or(ch));
    }
    result
}

/// Normalize a filter value for custom enum types (PascalCase → snake_case).
/// Handles both single and comma-separated values.
fn normalize_enum_value(value: String) -> String {
    if value.contains(',') {
        value.split(',')
            .map(|v| to_snake_case(v.trim()))
            .collect::<Vec<_>>()
            .join(",")
    } else {
        to_snake_case(&value)
    }
}

/// Parse filter from query parameters HashMap
///
/// # Arguments
///
/// * `params` - Query parameters from the HTTP request
/// * `column_types` - PostgreSQL enum type mappings for casting
/// * `allowed_fields` - Optional allow-list of valid field names for security
///
/// # Example Input
///
/// ```text
/// {
///     "username[contain]": "john",
///     "age[gt]": "18",
///     "status[eq]": "active",
///     "orderby[name]": "asc",
///     "search": "query"
/// }
/// ```
pub fn parse_filters(
    params: &HashMap<String, String>,
    column_types: &HashMap<String, String>,
    allowed_fields: Option<&HashSet<String>>,
) -> Result<QueryFilter> {
    let mut filter = QueryFilter::new();
    let mut or_conditions: Vec<FilterCondition> = Vec::new();

    for (key, value) in params {
        // Parse bracket notation: field[operator]
        if let Some(bracket_pos) = key.find('[') {
            if key.ends_with(']') {
                let field = &key[..bracket_pos];
                let sanitized_field = sanitize_field_name(field)?;

                // Validate against allow-list if provided
                if let Some(allowed) = allowed_fields {
                    if !is_valid_field(&sanitized_field, allowed) {
                        continue; // Skip invalid fields
                    }
                }

                let operator_str = &key[bracket_pos + 1..key.len() - 1];

                // Handle special operators
                match operator_str.to_ascii_lowercase().as_str() {
                    "orderby" => {
                        // orderby[field]=direction
                        let sort_field = audit_metadata_sql_expr(&sanitized_field)
                            .unwrap_or_else(|| sanitized_field.clone());
                        filter.add_sort(SortSpec::new(
                            sort_field,
                            SortDirection::from_str(value)
                        ));
                        continue;
                    }
                    "or" | "orwhere" => {
                        // Store OR conditions to process later
                        let condition = FilterCondition::new(
                            sanitized_field.clone(),
                            FilterOperator::Equal,
                            FilterValue::from_string(value.clone(), false)
                        ).with_logical(FilterLogical::Or);
                        or_conditions.push(condition);
                        continue;
                    }
                    _ => {
                        // Standard operator
                        if let Some(op) = FilterOperator::from_str(operator_str) {
                            // Normalize enum values (PascalCase → snake_case) for custom enum types
                            let filter_value = if let Some(col_type) = column_types.get(&sanitized_field) {
                                if is_custom_enum_type(col_type) {
                                    normalize_enum_value(value.clone())
                                } else {
                                    value.clone()
                                }
                            } else {
                                value.clone()
                            };

                            // Audit timestamps live in `metadata` JSONB — rewrite the
                            // field to the SQL expression that reads from there.
                            let condition_field = audit_metadata_sql_expr(&sanitized_field)
                                .unwrap_or_else(|| sanitized_field.clone());

                            let condition = FilterCondition::new(
                                condition_field,
                                op.clone(),
                                FilterValue::from_string(filter_value, false)
                            );

                            // Add column type for casting. Audit-metadata fields need
                            // `::timestamptz` on the RHS too — the LHS expression casts the
                            // JSONB extract, but the bound parameter is still text and Postgres
                            // has no implicit `timestamptz <op> text` operator.
                            let condition = if audit_metadata_sql_expr(&sanitized_field).is_some() {
                                condition.with_column_type("timestamptz".to_string())
                            } else if let Some(col_type) = column_types.get(&sanitized_field) {
                                condition.with_column_type(col_type.clone())
                            } else {
                                condition
                            };

                            filter.add_condition(condition);
                        }
                    }
                }
            }
        } else {
            // Handle non-bracket keys (simple equality or special keys)
            match key.to_ascii_lowercase().as_str() {
                "orderby" | "sort" => {
                    // Simple orderby: orderby=field or orderby[field]=dir
                    if value.contains(',') {
                        // Multiple fields: orderby=name,-age
                        for part in value.split(',') {
                            let part = part.trim();
                            let direction = if part.starts_with('-') {
                                SortDirection::Desc
                            } else {
                                SortDirection::Asc
                            };
                            let field = part.trim_start_matches('-');
                            if let Ok(sanitized) = sanitize_field_name(field) {
                                let sort_field = audit_metadata_sql_expr(&sanitized)
                                    .unwrap_or_else(|| sanitized.clone());
                                // Validate against allow-list if provided
                                if let Some(allowed) = allowed_fields {
                                    if is_valid_field(&sanitized, allowed) {
                                        filter.add_sort(SortSpec::new(sort_field, direction));
                                    }
                                } else {
                                    filter.add_sort(SortSpec::new(sort_field, direction));
                                }
                            }
                        }
                    } else {
                        #[allow(clippy::collapsible_else_if)]
                        if let Ok(sanitized) = sanitize_field_name(value) {
                            let sort_field = audit_metadata_sql_expr(&sanitized)
                                .unwrap_or_else(|| sanitized.clone());
                            if let Some(allowed) = allowed_fields {
                                if is_valid_field(&sanitized, allowed) {
                                    filter.add_sort(SortSpec::new(sort_field, SortDirection::Asc));
                                }
                            } else {
                                filter.add_sort(SortSpec::new(sort_field, SortDirection::Asc));
                            }
                        }
                    }
                }
                "search" => {
                    filter.search = Some(value.clone());
                }
                "searchfields" => {
                    filter.search_fields = value.split(',').filter_map(|s| {
                        let trimmed = s.trim();
                        sanitize_field_name(trimmed).ok().filter(|sanitized| {
                            if let Some(allowed) = allowed_fields {
                                is_valid_field(sanitized, allowed)
                            } else {
                                true
                            }
                        })
                    }).collect();
                }
                "limit" => {
                    if let Ok(l) = value.parse::<u32>() {
                        filter.limit = Some(l);
                    }
                }
                "offset" => {
                    if let Ok(o) = value.parse::<u32>() {
                        filter.offset = Some(o);
                    }
                }
                "page" => {
                    if let Ok(p) = value.parse::<u32>() {
                        filter.page = Some(p);
                    }
                }
                "pagesize" | "perpage" | "per_page" => {
                    if let Ok(p) = value.parse::<u32>() {
                        filter.limit = Some(p);
                    }
                }
                "__base_condition" => {
                    // Raw SQL condition to be ANDed with all other conditions
                    // Used for soft delete exclusion, tenant filtering, etc.
                    filter.base_conditions.push(value.clone());
                }
                _ => {
                    // Treat as simple equality filter
                    // Skip if it's a known non-filter parameter
                    if !matches!(key.as_str(), "fields" | "include" | "with") {
                        // Validate and sanitize the field name
                        let sanitized_field = match sanitize_field_name(key) {
                            Ok(f) => f,
                            Err(_) => continue, // Skip invalid field names
                        };

                        // Validate against allow-list if provided
                        if let Some(allowed) = allowed_fields {
                            if !is_valid_field(&sanitized_field, allowed) {
                                continue; // Skip invalid fields
                            }
                        }

                        // Normalize enum values (PascalCase → snake_case) for custom enum types
                        let filter_value = if let Some(col_type) = column_types.get(&sanitized_field) {
                            if is_custom_enum_type(col_type) {
                                normalize_enum_value(value.clone())
                            } else {
                                value.clone()
                            }
                        } else {
                            value.clone()
                        };

                        // Audit timestamps live in `metadata` JSONB — rewrite the
                        // field to the SQL expression that reads from there.
                        let condition_field = audit_metadata_sql_expr(&sanitized_field)
                            .unwrap_or_else(|| sanitized_field.clone());

                        let condition = FilterCondition::new(
                            condition_field,
                            FilterOperator::Equal,
                            FilterValue::from_string(filter_value, value.contains(','))
                        );

                        let condition = if audit_metadata_sql_expr(&sanitized_field).is_some() {
                            condition.with_column_type("timestamptz".to_string())
                        } else if let Some(col_type) = column_types.get(&sanitized_field) {
                            condition.with_column_type(col_type.clone())
                        } else {
                            condition
                        };

                        filter.add_condition(condition);
                    }
                }
            }
        }
    }

    // Add OR conditions at the end
    for condition in or_conditions {
        filter.add_condition(condition);
    }

    Ok(filter)
}
