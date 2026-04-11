//! Field validation and sanitization for filter queries

use std::collections::HashSet;
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

    Ok(field.to_string())
}
