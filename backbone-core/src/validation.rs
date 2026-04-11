//! Generic entity validation — composable field rules with typed errors.
//!
//! Generated services get a type alias:
//!
//! ```rust,ignore
//! // Generated:
//! pub type StoredFileValidator = EntityValidator<StoredFile>;
//! ```
//!
//! Custom decorators then add field rules without touching generated code:
//!
//! ```rust,ignore
//! // Custom:
//! impl StoredFileValidator {
//!     pub fn with_business_rules() -> Self {
//!         EntityValidator::new()
//!             .rule(RequiredString::new("name", |e: &StoredFile| &e.name))
//!             .rule(MaxLength::new("name", |e: &StoredFile| &e.name, 255))
//!             .rule(NonNegative::new("size_bytes", |e: &StoredFile| e.size_bytes))
//!     }
//! }
//! ```

use std::marker::PhantomData;
use std::sync::Arc;

// ─── Validation error ────────────────────────────────────────────────────────

/// A single field-level validation failure.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationError {
    /// The field name (dot-separated for nested: `"address.street"`).
    pub field: String,
    /// Human-readable message.
    pub message: String,
    /// Optional machine-readable code for API clients.
    pub code: Option<String>,
}

impl ValidationError {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
            code: None,
        }
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

/// All validation errors collected from a single `validate()` call.
#[derive(Debug, Clone, Default)]
pub struct ValidationErrors(Vec<ValidationError>);

impl ValidationErrors {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn push(&mut self, error: ValidationError) {
        self.0.push(error);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn errors(&self) -> &[ValidationError] {
        &self.0
    }

    pub fn into_errors(self) -> Vec<ValidationError> {
        self.0
    }

    /// Returns `Ok(())` if no errors, `Err(self)` otherwise.
    pub fn into_result(self) -> Result<(), ValidationErrors> {
        if self.is_empty() {
            Ok(())
        } else {
            Err(self)
        }
    }
}

impl std::fmt::Display for ValidationErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for error in &self.0 {
            writeln!(f, "{error}")?;
        }
        Ok(())
    }
}

// ─── Field rule trait ────────────────────────────────────────────────────────

/// A single validation rule applied to an entity.
///
/// Implement this to create custom rules beyond the built-ins.
pub trait FieldRule<E>: Send + Sync {
    fn validate(&self, entity: &E) -> Vec<ValidationError>;
}

// ─── EntityValidator ─────────────────────────────────────────────────────────

/// Composable validator for any entity type `E`.
///
/// Rules are evaluated in registration order and all failures are collected
/// before returning — no fail-fast behaviour.
pub struct EntityValidator<E> {
    rules: Vec<Arc<dyn FieldRule<E>>>,
    _phantom: PhantomData<E>,
}

impl<E: Send + Sync + 'static> EntityValidator<E> {
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            _phantom: PhantomData,
        }
    }

    /// Add a rule to this validator (builder pattern).
    pub fn rule(mut self, rule: impl FieldRule<E> + 'static) -> Self {
        self.rules.push(Arc::new(rule));
        self
    }

    /// Run all rules and collect errors.
    pub fn validate(&self, entity: &E) -> ValidationErrors {
        let mut errors = ValidationErrors::new();
        for rule in &self.rules {
            for error in rule.validate(entity) {
                errors.push(error);
            }
        }
        errors
    }

    /// Convenience — returns `Ok(())` or `Err(ValidationErrors)`.
    pub fn validate_result(&self, entity: &E) -> Result<(), ValidationErrors> {
        self.validate(entity).into_result()
    }
}

impl<E: Send + Sync + 'static> Default for EntityValidator<E> {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Built-in rules ──────────────────────────────────────────────────────────

/// Fails if a string field is empty or whitespace-only.
pub struct RequiredString<E, F> {
    field_name: &'static str,
    accessor: F,
    _phantom: PhantomData<E>,
}

impl<E, F: Fn(&E) -> &str + Send + Sync> RequiredString<E, F> {
    pub fn new(field_name: &'static str, accessor: F) -> Self {
        Self {
            field_name,
            accessor,
            _phantom: PhantomData,
        }
    }
}

impl<E: Send + Sync, F: Fn(&E) -> &str + Send + Sync> FieldRule<E> for RequiredString<E, F> {
    fn validate(&self, entity: &E) -> Vec<ValidationError> {
        let value = (self.accessor)(entity);
        if value.trim().is_empty() {
            vec![ValidationError::new(
                self.field_name,
                format!("{} is required", self.field_name),
            )
            .with_code("required")]
        } else {
            vec![]
        }
    }
}

/// Fails if a string field exceeds `max_len` Unicode scalar values.
pub struct MaxLength<E, F> {
    field_name: &'static str,
    accessor: F,
    max_len: usize,
    _phantom: PhantomData<E>,
}

impl<E, F: Fn(&E) -> &str + Send + Sync> MaxLength<E, F> {
    pub fn new(field_name: &'static str, accessor: F, max_len: usize) -> Self {
        Self {
            field_name,
            accessor,
            max_len,
            _phantom: PhantomData,
        }
    }
}

impl<E: Send + Sync, F: Fn(&E) -> &str + Send + Sync> FieldRule<E> for MaxLength<E, F> {
    fn validate(&self, entity: &E) -> Vec<ValidationError> {
        let value = (self.accessor)(entity);
        if value.chars().count() > self.max_len {
            vec![ValidationError::new(
                self.field_name,
                format!(
                    "{} must be at most {} characters",
                    self.field_name, self.max_len
                ),
            )
            .with_code("max_length")]
        } else {
            vec![]
        }
    }
}

/// Fails if a numeric field is negative.
pub struct NonNegative<E, F> {
    field_name: &'static str,
    accessor: F,
    _phantom: PhantomData<E>,
}

impl<E, F: Fn(&E) -> i64 + Send + Sync> NonNegative<E, F> {
    pub fn new(field_name: &'static str, accessor: F) -> Self {
        Self {
            field_name,
            accessor,
            _phantom: PhantomData,
        }
    }
}

impl<E: Send + Sync, F: Fn(&E) -> i64 + Send + Sync> FieldRule<E> for NonNegative<E, F> {
    fn validate(&self, entity: &E) -> Vec<ValidationError> {
        let value = (self.accessor)(entity);
        if value < 0 {
            vec![ValidationError::new(
                self.field_name,
                format!("{} must be 0 or greater", self.field_name),
            )
            .with_code("non_negative")]
        } else {
            vec![]
        }
    }
}

/// Fails if an `Option<String>` field is `Some("")` or `Some("   ")`.
pub struct OptionalNotBlank<E, F> {
    field_name: &'static str,
    accessor: F,
    _phantom: PhantomData<E>,
}

impl<E, F: Fn(&E) -> Option<&str> + Send + Sync> OptionalNotBlank<E, F> {
    pub fn new(field_name: &'static str, accessor: F) -> Self {
        Self {
            field_name,
            accessor,
            _phantom: PhantomData,
        }
    }
}

impl<E: Send + Sync, F: Fn(&E) -> Option<&str> + Send + Sync> FieldRule<E>
    for OptionalNotBlank<E, F>
{
    fn validate(&self, entity: &E) -> Vec<ValidationError> {
        if let Some(value) = (self.accessor)(entity) {
            if value.trim().is_empty() {
                return vec![ValidationError::new(
                    self.field_name,
                    format!("{} must not be blank when provided", self.field_name),
                )
                .with_code("not_blank")];
            }
        }
        vec![]
    }
}

/// Fails if a UUID string field is empty or not a valid v4 UUID.
///
/// Checks that the field is a non-empty string that can be parsed as a UUID.
pub struct RequiredUuid<E, F> {
    field_name: &'static str,
    accessor: F,
    _phantom: PhantomData<E>,
}

impl<E, F: Fn(&E) -> &str + Send + Sync> RequiredUuid<E, F> {
    pub fn new(field_name: &'static str, accessor: F) -> Self {
        Self {
            field_name,
            accessor,
            _phantom: PhantomData,
        }
    }
}

impl<E: Send + Sync, F: Fn(&E) -> &str + Send + Sync> FieldRule<E> for RequiredUuid<E, F> {
    fn validate(&self, entity: &E) -> Vec<ValidationError> {
        let value = (self.accessor)(entity);
        if value.trim().is_empty() {
            return vec![ValidationError::new(
                self.field_name,
                format!("{} is required", self.field_name),
            )
            .with_code("required")];
        }
        if uuid::Uuid::parse_str(value).is_err() {
            return vec![ValidationError::new(
                self.field_name,
                format!("{} must be a valid UUID", self.field_name),
            )
            .with_code("invalid_uuid")];
        }
        vec![]
    }
}

/// Fails if a string field doesn't match the given regex pattern.
pub struct Regex<E, F> {
    field_name: &'static str,
    accessor: F,
    pattern: &'static str,
    message: &'static str,
    _phantom: PhantomData<E>,
}

impl<E, F: Fn(&E) -> &str + Send + Sync> Regex<E, F> {
    pub fn new(
        field_name: &'static str,
        accessor: F,
        pattern: &'static str,
        message: &'static str,
    ) -> Self {
        Self {
            field_name,
            accessor,
            pattern,
            message,
            _phantom: PhantomData,
        }
    }
}

impl<E: Send + Sync, F: Fn(&E) -> &str + Send + Sync> FieldRule<E> for Regex<E, F> {
    fn validate(&self, entity: &E) -> Vec<ValidationError> {
        let value = (self.accessor)(entity);
        match regex::Regex::new(self.pattern) {
            Ok(re) if re.is_match(value) => vec![],
            Ok(_) => vec![ValidationError::new(self.field_name, self.message).with_code("pattern")],
            Err(_) => vec![ValidationError::new(
                self.field_name,
                format!("invalid regex pattern: {}", self.pattern),
            )
            .with_code("invalid_pattern")],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct User {
        name: String,
        age: i64,
        bio: Option<String>,
    }

    #[test]
    fn required_string_rejects_blank() {
        let rule = RequiredString::new("name", |u: &User| u.name.as_str());
        let user = User {
            name: "  ".into(),
            age: 30,
            bio: None,
        };
        let errors = rule.validate(&user);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code.as_deref(), Some("required"));
    }

    #[test]
    fn max_length_allows_exact_length() {
        let rule = MaxLength::new("name", |u: &User| u.name.as_str(), 3);
        let user = User {
            name: "abc".into(),
            age: 30,
            bio: None,
        };
        assert!(rule.validate(&user).is_empty());
    }

    #[test]
    fn max_length_rejects_over_limit() {
        let rule = MaxLength::new("name", |u: &User| u.name.as_str(), 3);
        let user = User {
            name: "abcd".into(),
            age: 30,
            bio: None,
        };
        assert!(!rule.validate(&user).is_empty());
    }

    #[test]
    fn regex_rule_validates_pattern() {
        let rule = Regex::new("phone", |u: &User| u.name.as_str(), r"^\+\d{7,15}$", "must be E.164");

        let valid = User { name: "+628123456789".into(), age: 0, bio: None };
        assert!(rule.validate(&valid).is_empty(), "valid E.164 should pass");

        let invalid = User { name: "no-plus".into(), age: 0, bio: None };
        let errors = rule.validate(&invalid);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code.as_deref(), Some("pattern"));
    }

    #[test]
    fn entity_validator_collects_all_errors() {
        let validator = EntityValidator::new()
            .rule(RequiredString::new("name", |u: &User| u.name.as_str()))
            .rule(NonNegative::new("age", |u: &User| u.age));

        let user = User {
            name: "".into(),
            age: -1,
            bio: None,
        };

        let errors = validator.validate(&user);
        assert_eq!(errors.errors().len(), 2);
    }
}
