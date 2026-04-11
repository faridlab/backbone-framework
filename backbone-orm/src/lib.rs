//! Backbone Framework ORM
//!
//! Database layer with PostgreSQL support and generic repository patterns.
//!
//! # Features
//!
//! - PostgreSQL repository with migrations
//! - Query builder for type-safe queries
//! - Connection pooling
//! - Database seeding utilities
//! - In-memory store for testing

pub mod repository;
pub mod generic_repository;
pub mod migrations;
pub mod query_builder;
pub mod connection;
pub mod seeding;
pub mod raw_query;
pub mod filter;
pub mod in_memory;

// Include test modules
#[cfg(test)]
mod repository_tests;
#[cfg(test)]
mod migrations_tests;
#[cfg(test)]
mod query_builder_tests;
#[cfg(test)]
mod seeding_tests;
#[cfg(test)]
mod raw_query_tests;

// Re-export commonly used types
pub use repository::*;
pub use generic_repository::{GenericCrudRepository, SoftDelete, HardDelete, EntityRepoMeta};
pub use migrations::*;
pub use query_builder::*;
pub use connection::*;
pub use seeding::*;
pub use raw_query::*;
// Re-export filter module types with aliases to avoid conflicts with repository types
// The repository's FilterCondition/SortDirection are kept for backward compatibility
pub use filter::{
    QueryFilter,
    FilterOperator as QueryFilterOperator,
    FilterCondition as QueryFilterCondition,
    FilterValue,
    FilterLogical,
    SortDirection as FilterSortDirection,
    SortSpec,
    FilterableEntity,
    parse_filters,
    is_valid_field,
    sanitize_field_name,
};
pub use in_memory::InMemoryStore;

/// ORM version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Database connection traits and utilities
pub use sqlx::postgres::PgPool;
pub use sqlx::postgres::PgPoolOptions;

/// PostgreSQL specific types
pub use sqlx::types::chrono::NaiveDateTime;
pub use sqlx::types::uuid::Uuid;