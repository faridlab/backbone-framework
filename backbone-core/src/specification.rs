//! Specification Pattern - Business Rules as Objects
//!
//! The Specification pattern encapsulates business rules that can be combined
//! and reused. This is a core DDD tactical pattern.
//!
//! # Example
//!
//! ```rust,ignore
//! use backbone_core::specification::{Specification, AndSpecification};
//!
//! // Define a specification
//! struct UserIsActive;
//!
//! impl Specification<User> for UserIsActive {
//!     type Error = String;
//!
//!     fn is_satisfied_by(&self, user: &User) -> Result<bool, Self::Error> {
//!         Ok(user.status == UserStatus::Active)
//!     }
//! }
//!
//! // Combine specifications
//! let spec = UserIsActive.and(UserHasVerifiedEmail);
//! let is_valid = spec.is_satisfied_by(&user)?;
//! ```

use std::collections::HashMap;
use std::fmt::Debug;
use std::marker::PhantomData;

/// Core Specification trait
///
/// Specifications encapsulate business rules that can be:
/// - Combined with AND, OR, NOT operators
/// - Reused across different parts of the application
/// - Tested in isolation
///
/// # Type Parameters
///
/// - `T`: The type of entity being validated
pub trait Specification<T>: Send + Sync {
    /// Error type for validation failures
    type Error: Debug + Send;

    /// Check if the candidate satisfies this specification
    fn is_satisfied_by(&self, candidate: &T) -> Result<bool, Self::Error>;

    /// Combine with another specification using AND
    fn and<S>(self, other: S) -> AndSpecification<Self, S, T>
    where
        Self: Sized,
        S: Specification<T>,
    {
        AndSpecification::new(self, other)
    }

    /// Combine with another specification using OR
    fn or<S>(self, other: S) -> OrSpecification<Self, S, T>
    where
        Self: Sized,
        S: Specification<T>,
    {
        OrSpecification::new(self, other)
    }

    /// Negate this specification
    fn not(self) -> NotSpecification<Self, T>
    where
        Self: Sized,
    {
        NotSpecification::new(self)
    }
}

// ============================================================================
// Composite Specifications
// ============================================================================

/// AND specification - both left and right must be satisfied
#[derive(Debug, Clone)]
pub struct AndSpecification<L, R, T> {
    left: L,
    right: R,
    _marker: PhantomData<T>,
}

impl<L, R, T> AndSpecification<L, R, T> {
    pub fn new(left: L, right: R) -> Self {
        Self {
            left,
            right,
            _marker: PhantomData,
        }
    }
}

impl<L, R, T> Specification<T> for AndSpecification<L, R, T>
where
    L: Specification<T>,
    R: Specification<T>,
    T: Send + Sync,
{
    type Error = String;

    fn is_satisfied_by(&self, candidate: &T) -> Result<bool, Self::Error> {
        let left_result = self
            .left
            .is_satisfied_by(candidate)
            .map_err(|e| format!("Left specification failed: {:?}", e))?;

        if !left_result {
            return Ok(false);
        }

        self.right
            .is_satisfied_by(candidate)
            .map_err(|e| format!("Right specification failed: {:?}", e))
    }
}

/// OR specification - either left or right must be satisfied
#[derive(Debug, Clone)]
pub struct OrSpecification<L, R, T> {
    left: L,
    right: R,
    _marker: PhantomData<T>,
}

impl<L, R, T> OrSpecification<L, R, T> {
    pub fn new(left: L, right: R) -> Self {
        Self {
            left,
            right,
            _marker: PhantomData,
        }
    }
}

impl<L, R, T> Specification<T> for OrSpecification<L, R, T>
where
    L: Specification<T>,
    R: Specification<T>,
    T: Send + Sync,
{
    type Error = String;

    fn is_satisfied_by(&self, candidate: &T) -> Result<bool, Self::Error> {
        let left_result = self
            .left
            .is_satisfied_by(candidate)
            .map_err(|e| format!("Left specification failed: {:?}", e))?;

        if left_result {
            return Ok(true);
        }

        self.right
            .is_satisfied_by(candidate)
            .map_err(|e| format!("Right specification failed: {:?}", e))
    }
}

/// NOT specification - negates the inner specification
#[derive(Debug, Clone)]
pub struct NotSpecification<S, T> {
    spec: S,
    _marker: PhantomData<T>,
}

impl<S, T> NotSpecification<S, T> {
    pub fn new(spec: S) -> Self {
        Self {
            spec,
            _marker: PhantomData,
        }
    }
}

impl<S, T> Specification<T> for NotSpecification<S, T>
where
    S: Specification<T>,
    T: Send + Sync,
{
    type Error = String;

    fn is_satisfied_by(&self, candidate: &T) -> Result<bool, Self::Error> {
        let result = self
            .spec
            .is_satisfied_by(candidate)
            .map_err(|e| format!("Inner specification failed: {:?}", e))?;
        Ok(!result)
    }
}

// ============================================================================
// Specification Result
// ============================================================================

/// Result of evaluating a specification with detailed information
#[derive(Debug, Clone)]
pub struct SpecificationResult {
    /// Whether the specification was satisfied
    pub satisfied: bool,
    /// Name of the specification
    pub specification_name: String,
    /// Human-readable message
    pub message: String,
    /// Additional details
    pub details: HashMap<String, String>,
}

impl SpecificationResult {
    /// Create a satisfied result
    pub fn satisfied(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            satisfied: true,
            specification_name: name.into(),
            message: message.into(),
            details: HashMap::new(),
        }
    }

    /// Create an unsatisfied result
    pub fn unsatisfied(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            satisfied: false,
            specification_name: name.into(),
            message: message.into(),
            details: HashMap::new(),
        }
    }

    /// Add a detail
    pub fn with_detail(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.details.insert(key.into(), value.into());
        self
    }
}

// ============================================================================
// Specification Evaluator
// ============================================================================

/// Evaluates multiple specifications and collects results
pub struct SpecificationEvaluator<T> {
    _marker: PhantomData<T>,
}

impl<T: Send + Sync> SpecificationEvaluator<T> {
    /// Evaluate all specifications and return all results
    pub fn evaluate_all<S>(
        specifications: &[&S],
        candidate: &T,
    ) -> Vec<Result<bool, String>>
    where
        S: Specification<T, Error = String> + ?Sized,
    {
        specifications
            .iter()
            .map(|spec| spec.is_satisfied_by(candidate))
            .collect()
    }

    /// Check if all specifications are satisfied
    pub fn all_satisfied<S>(specifications: &[&S], candidate: &T) -> Result<bool, String>
    where
        S: Specification<T, Error = String> + ?Sized,
    {
        for spec in specifications {
            if !spec.is_satisfied_by(candidate)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Check if any specification is satisfied
    pub fn any_satisfied<S>(specifications: &[&S], candidate: &T) -> Result<bool, String>
    where
        S: Specification<T, Error = String> + ?Sized,
    {
        for spec in specifications {
            if spec.is_satisfied_by(candidate)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

// ============================================================================
// Common Specifications
// ============================================================================

/// Always returns true
#[derive(Debug, Clone, Default)]
pub struct AlwaysTrue<T>(PhantomData<T>);

impl<T> AlwaysTrue<T> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T: Send + Sync> Specification<T> for AlwaysTrue<T> {
    type Error = std::convert::Infallible;

    fn is_satisfied_by(&self, _candidate: &T) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

/// Always returns false
#[derive(Debug, Clone, Default)]
pub struct AlwaysFalse<T>(PhantomData<T>);

impl<T> AlwaysFalse<T> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T: Send + Sync> Specification<T> for AlwaysFalse<T> {
    type Error = std::convert::Infallible;

    fn is_satisfied_by(&self, _candidate: &T) -> Result<bool, Self::Error> {
        Ok(false)
    }
}

/// Specification using a closure
pub struct PredicateSpecification<T, F>
where
    F: Fn(&T) -> bool + Send + Sync,
{
    predicate: F,
    _marker: PhantomData<T>,
}

impl<T, F> PredicateSpecification<T, F>
where
    F: Fn(&T) -> bool + Send + Sync,
{
    pub fn new(predicate: F) -> Self {
        Self {
            predicate,
            _marker: PhantomData,
        }
    }
}

impl<T, F> Specification<T> for PredicateSpecification<T, F>
where
    T: Send + Sync,
    F: Fn(&T) -> bool + Send + Sync,
{
    type Error = std::convert::Infallible;

    fn is_satisfied_by(&self, candidate: &T) -> Result<bool, Self::Error> {
        Ok((self.predicate)(candidate))
    }
}

/// Helper function to create a predicate specification
pub fn predicate<T, F>(f: F) -> PredicateSpecification<T, F>
where
    F: Fn(&T) -> bool + Send + Sync,
{
    PredicateSpecification::new(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct TestEntity {
        value: i32,
        active: bool,
    }

    struct PositiveValue;
    impl Specification<TestEntity> for PositiveValue {
        type Error = String;
        fn is_satisfied_by(&self, e: &TestEntity) -> Result<bool, Self::Error> {
            Ok(e.value > 0)
        }
    }

    struct IsActive;
    impl Specification<TestEntity> for IsActive {
        type Error = String;
        fn is_satisfied_by(&self, e: &TestEntity) -> Result<bool, Self::Error> {
            Ok(e.active)
        }
    }

    #[test]
    fn test_and_specification() {
        let spec = PositiveValue.and(IsActive);

        let active_positive = TestEntity { value: 10, active: true };
        assert!(spec.is_satisfied_by(&active_positive).unwrap());

        let inactive_positive = TestEntity { value: 10, active: false };
        assert!(!spec.is_satisfied_by(&inactive_positive).unwrap());

        let active_negative = TestEntity { value: -5, active: true };
        assert!(!spec.is_satisfied_by(&active_negative).unwrap());
    }

    #[test]
    fn test_or_specification() {
        let spec = PositiveValue.or(IsActive);

        let inactive_positive = TestEntity { value: 10, active: false };
        assert!(spec.is_satisfied_by(&inactive_positive).unwrap());

        let active_negative = TestEntity { value: -5, active: true };
        assert!(spec.is_satisfied_by(&active_negative).unwrap());

        let inactive_negative = TestEntity { value: -5, active: false };
        assert!(!spec.is_satisfied_by(&inactive_negative).unwrap());
    }

    #[test]
    fn test_not_specification() {
        let spec = PositiveValue.not();

        let positive = TestEntity { value: 10, active: false };
        assert!(!spec.is_satisfied_by(&positive).unwrap());

        let negative = TestEntity { value: -5, active: false };
        assert!(spec.is_satisfied_by(&negative).unwrap());
    }

    #[test]
    fn test_predicate_specification() {
        let spec = predicate(|e: &TestEntity| e.value > 5);

        let high = TestEntity { value: 10, active: false };
        assert!(spec.is_satisfied_by(&high).unwrap());

        let low = TestEntity { value: 3, active: false };
        assert!(!spec.is_satisfied_by(&low).unwrap());
    }

    #[test]
    fn test_complex_composition() {
        // (PositiveValue AND IsActive) OR (value > 100)
        let spec = PositiveValue
            .and(IsActive)
            .or(predicate(|e: &TestEntity| e.value > 100));

        let active_positive = TestEntity { value: 10, active: true };
        assert!(spec.is_satisfied_by(&active_positive).unwrap());

        let inactive_very_high = TestEntity { value: 200, active: false };
        assert!(spec.is_satisfied_by(&inactive_very_high).unwrap());

        let inactive_low = TestEntity { value: 5, active: false };
        assert!(!spec.is_satisfied_by(&inactive_low).unwrap());
    }
}
