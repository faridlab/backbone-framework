//! GraphQL runtime support for the Backbone Framework
//!
//! Provides common types, pagination, and error handling
//! for generated GraphQL resolvers.

pub mod pagination;
pub mod error;

pub use pagination::*;
pub use error::*;

// Re-export async-graphql for convenience
pub use async_graphql;
