//! Field validation and sanitization for filter queries

use std::collections::HashSet;
use super::parser::to_snake_case;
use anyhow::Result;

/// Trait for entities to declare which fields are allowed in filter queries.
///
/// Implementing this trait on an entity enables field-level access control
/// for the filter DSL, preventing clients from filtering on internal or
/// sensitive columns (e.g., `password_hash`, `internal_notes`).
///
/// # Example
///
/// ```ignore
/// use backbone_orm::FilterableEntity;
/// use std::collections::HashSet;
///
/// struct User;
///
/// impl FilterableEntity for User {
///     fn filterable_fields() -> HashSet<String> {
///         ["id", "username", "email", "status", "created_at"]
///             .iter().map(|s| s.to_string()).collect()
///     }
/// }
/// ```
pub trait FilterableEntity {
    /// Returns the set of field names that are allowed in filter expressions.
    fn filterable_fields() -> HashSet<String>;

    /// Returns the set of field names that are allowed in sort expressions.
    /// Defaults to the same as filterable_fields.
    fn sortable_fields() -> HashSet<String> {
        Self::filterable_fields()
    }
}

/// Validate a field name against an allow-list
///
/// This function should be called before creating filter conditions with user input.
/// Returns true if the field is in the allow-list, false otherwise.
///
/// # Example
///
/// ```ignore
/// let allowed_fields: HashSet<String> = ["username", "email", "status"]
///     .iter()
///     .map(|s| s.to_string())
///     .collect();
///
/// if !is_valid_field("username", &allowed_fields) {
///     return Err(anyhow::anyhow!("Invalid field name"));
/// }
/// ```
pub fn is_valid_field(field: &str, allowed_fields: &HashSet<String>) -> bool {
    allowed_fields.contains(field)
}

/// Sanitize a field name for SQL identifier use
///
/// This function validates that a field name contains only safe characters
/// (alphanumeric, underscore) to prevent SQL injection in field names.
pub fn sanitize_field_name(field: &str) -> Result<String> {
    if field.is_empty() {
        return Err(anyhow::anyhow!("Field name cannot be empty"));
    }

    // Allow only alphanumeric and underscore
    if !field.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(anyhow::anyhow!("Invalid field name: '{}'", field));
    }

    // Prevent SQL injection by checking for dangerous patterns
    let lower = field.to_lowercase();
    if lower.contains("--") || lower.contains("/*") || lower.contains(";")
        || lower.contains("drop ") || lower.contains("delete ") || lower.contains("truncate ")
        || lower.contains("update ") || lower.contains("insert ") || lower.contains("exec ")
        || lower.contains("execute ") || lower.contains("script>") {
        return Err(anyhow::anyhow!("Potentially dangerous field name: '{}'", field));
    }

    // The wire speaks camelCase (the same convention the responses carry)
    // while columns are snake_case: fold the field name here, once, for
    // every consumer (bracket filters, plain equality, orderby). The fold
    // is idempotent for already-snake names, and PostgreSQL column names
    // are lowercase by construction so nothing legitimate is lost.
    Ok(to_snake_case(field))
}

#[cfg(test)]
mod casing_tests {
    use super::sanitize_field_name;

    #[test]
    fn camel_case_fields_fold_to_snake_columns() {
        assert_eq!(sanitize_field_name("employeeId").unwrap(), "employee_id");
        assert_eq!(sanitize_field_name("date").unwrap(), "date");
        assert_eq!(sanitize_field_name("employee_id").unwrap(), "employee_id");
        assert_eq!(sanitize_field_name("billableCost").unwrap(), "billable_cost");
    }

    #[test]
    fn dangerous_names_still_refused() {
        assert!(sanitize_field_name("drop table").is_err());
        assert!(sanitize_field_name("").is_err());
    }
}
