//! DDD Value Object Pattern
//!
//! Provides traits for implementing Value Objects in Domain-Driven Design.
//! Value Objects are immutable objects that are defined by their attributes
//! rather than by identity.
//!
//! # Example
//!
//! ```ignore
//! use backbone_core::ValueObject;
//!
//! #[derive(Clone, Debug, PartialEq, Eq, Hash)]
//! pub struct EmailAddress(String);
//!
//! impl ValueObject for EmailAddress {
//!     type Error = EmailError;
//!
//!     fn validate(&self) -> Result<(), Self::Error> {
//!         if self.0.contains('@') && self.0.len() > 3 {
//!             Ok(())
//!         } else {
//!             Err(EmailError::InvalidFormat)
//!         }
//!     }
//! }
//!
//! impl EmailAddress {
//!     pub fn new(email: impl Into<String>) -> Result<Self, EmailError> {
//!         Self(email.into()).new_validated()
//!     }
//! }
//! ```

use std::fmt::Debug;
use std::hash::Hash;

/// DDD Value Object trait.
///
/// Value Objects are characterized by:
/// - Immutability (once created, they don't change)
/// - Equality based on attributes, not identity
/// - Self-validation
///
/// # Required Bounds
///
/// - `Clone`: Value objects should be copyable
/// - `Eq + Hash`: Equality based on attributes
/// - `Debug`: For debugging and logging
/// - `Send + Sync`: Thread safety for async contexts
pub trait ValueObject: Clone + Eq + Hash + Debug + Send + Sync {
    /// Error type for validation failures.
    type Error: std::error::Error + Send + Sync;

    /// Validate the value object.
    ///
    /// Returns `Ok(())` if the value object is valid,
    /// or an error describing the validation failure.
    fn validate(&self) -> Result<(), Self::Error>;

    /// Create a validated instance.
    ///
    /// This is a convenience method that validates after construction.
    fn new_validated(self) -> Result<Self, Self::Error>
    where
        Self: Sized,
    {
        self.validate()?;
        Ok(self)
    }

    /// Check if this value object is valid without returning an error.
    fn is_valid(&self) -> bool {
        self.validate().is_ok()
    }
}

/// Extension trait for optional value objects.
pub trait OptionalValueObject<T: ValueObject> {
    /// Validate the inner value if present.
    fn validate_if_present(&self) -> Result<(), T::Error>;
}

impl<T: ValueObject> OptionalValueObject<T> for Option<T> {
    fn validate_if_present(&self) -> Result<(), T::Error> {
        match self {
            Some(value) => value.validate(),
            None => Ok(()),
        }
    }
}

/// Common validation error for value objects.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ValueObjectError {
    /// The value is empty when it shouldn't be.
    #[error("Value cannot be empty")]
    Empty,

    /// The value exceeds the maximum length.
    #[error("Value exceeds maximum length of {max} characters")]
    TooLong { max: usize },

    /// The value is shorter than the minimum length.
    #[error("Value must be at least {min} characters")]
    TooShort { min: usize },

    /// The value doesn't match the expected format.
    #[error("Invalid format: {message}")]
    InvalidFormat { message: String },

    /// The value is out of the allowed range.
    #[error("Value out of range: {message}")]
    OutOfRange { message: String },

    /// Custom validation error.
    #[error("{0}")]
    Custom(String),
}

/// Helper macro for implementing simple string-based value objects.
#[macro_export]
macro_rules! define_string_value_object {
    ($name:ident, $min_len:expr, $max_len:expr) => {
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub struct $name(String);

        impl $crate::ValueObject for $name {
            type Error = $crate::ValueObjectError;

            fn validate(&self) -> Result<(), Self::Error> {
                if self.0.is_empty() {
                    return Err($crate::ValueObjectError::Empty);
                }
                if self.0.len() < $min_len {
                    return Err($crate::ValueObjectError::TooShort { min: $min_len });
                }
                if self.0.len() > $max_len {
                    return Err($crate::ValueObjectError::TooLong { max: $max_len });
                }
                Ok(())
            }
        }

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, $crate::ValueObjectError> {
                use $crate::ValueObject;
                Self(value.into()).new_validated()
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    struct TestEmail(String);

    impl ValueObject for TestEmail {
        type Error = ValueObjectError;

        fn validate(&self) -> Result<(), Self::Error> {
            if !self.0.contains('@') {
                return Err(ValueObjectError::InvalidFormat {
                    message: "Email must contain @".to_string(),
                });
            }
            Ok(())
        }
    }

    #[test]
    fn test_valid_value_object() {
        let email = TestEmail("test@example.com".to_string());
        assert!(email.validate().is_ok());
        assert!(email.is_valid());
    }

    #[test]
    fn test_invalid_value_object() {
        let email = TestEmail("invalid".to_string());
        assert!(email.validate().is_err());
        assert!(!email.is_valid());
    }

    #[test]
    fn test_new_validated() {
        let valid = TestEmail("test@example.com".to_string()).new_validated();
        assert!(valid.is_ok());

        let invalid = TestEmail("invalid".to_string()).new_validated();
        assert!(invalid.is_err());
    }

    #[test]
    fn test_optional_value_object() {
        let some_valid: Option<TestEmail> = Some(TestEmail("test@example.com".to_string()));
        assert!(some_valid.validate_if_present().is_ok());

        let some_invalid: Option<TestEmail> = Some(TestEmail("invalid".to_string()));
        assert!(some_invalid.validate_if_present().is_err());

        let none: Option<TestEmail> = None;
        assert!(none.validate_if_present().is_ok());
    }
}
