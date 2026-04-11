//! Error conversion helpers for GraphQL resolvers

use async_graphql::Error;

/// Convert any displayable error into a GraphQL error
pub fn service_error<E: std::fmt::Display>(e: E) -> Error {
    Error::new(e.to_string())
}

/// Create a "not found" GraphQL error
pub fn not_found_error(entity: &str, id: &str) -> Error {
    Error::new(format!("{} with id '{}' not found", entity, id))
}
