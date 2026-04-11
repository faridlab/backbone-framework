//! CQRS Query Pattern
//!
//! Provides traits for implementing the Query side of CQRS.
//! Queries represent requests for information without side effects.
//!
//! # Example
//!
//! ```ignore
//! use backbone_core::{Query, QueryHandler};
//!
//! // Define a query
//! pub struct GetUserByEmailQuery {
//!     pub email: String,
//! }
//!
//! impl Query for GetUserByEmailQuery {
//!     type Result = Option<UserDto>;
//! }
//!
//! // Implement the handler
//! pub struct GetUserByEmailHandler {
//!     read_model: Arc<dyn UserReadModel>,
//! }
//!
//! #[async_trait::async_trait]
//! impl QueryHandler<GetUserByEmailQuery> for GetUserByEmailHandler {
//!     type Error = QueryError;
//!
//!     async fn handle(&self, query: GetUserByEmailQuery) -> Result<Option<UserDto>, Self::Error> {
//!         self.read_model.find_by_email(&query.email).await
//!     }
//! }
//! ```

use async_trait::async_trait;

/// Marker trait for CQRS queries.
///
/// Queries represent requests for information and should not
/// cause any side effects in the system.
pub trait Query: Send + Sync {
    /// The result type returned by the query.
    type Result: Send + Sync;
}

/// Handler for executing queries.
///
/// Query handlers retrieve data from read models and should
/// be optimized for read performance.
#[async_trait]
pub trait QueryHandler<Q: Query>: Send + Sync {
    /// Error type for query execution failures.
    type Error: std::error::Error + Send + Sync;

    /// Execute the query and return the result.
    async fn handle(&self, query: Q) -> Result<Q::Result, Self::Error>;
}

/// Query dispatcher for routing queries to their handlers.
///
/// Provides a central point for query execution with
/// optional caching and middleware support.
#[async_trait]
pub trait QueryDispatcher: Send + Sync {
    /// Dispatch a query to its handler.
    async fn dispatch<Q: Query>(
        &self,
        query: Q,
    ) -> Result<Q::Result, Box<dyn std::error::Error + Send + Sync>>;
}

/// Trait for queries that support caching.
pub trait CacheableQuery: Query {
    /// Cache key for this query.
    fn cache_key(&self) -> String;

    /// Time-to-live for cached results in seconds.
    fn cache_ttl(&self) -> Option<u64> {
        None // No caching by default
    }
}

/// Trait for paginated queries.
pub trait PaginatedQuery: Query {
    /// Get the page number (1-based).
    fn page(&self) -> u32;

    /// Get the page size.
    fn page_size(&self) -> u32;

    /// Get the offset for database queries.
    fn offset(&self) -> u32 {
        (self.page().saturating_sub(1)) * self.page_size()
    }
}

/// Result wrapper for paginated queries.
#[derive(Debug, Clone)]
pub struct PaginatedQueryResult<T> {
    /// The items for the current page.
    pub items: Vec<T>,
    /// Total number of items across all pages.
    pub total: u64,
    /// Current page number (1-based).
    pub page: u32,
    /// Number of items per page.
    pub page_size: u32,
    /// Total number of pages.
    pub total_pages: u32,
}

impl<T> PaginatedQueryResult<T> {
    /// Create a new paginated result.
    pub fn new(items: Vec<T>, total: u64, page: u32, page_size: u32) -> Self {
        let total_pages = if page_size > 0 {
            ((total as f64) / (page_size as f64)).ceil() as u32
        } else {
            0
        };

        Self {
            items,
            total,
            page,
            page_size,
            total_pages,
        }
    }

    /// Check if there's a next page.
    pub fn has_next(&self) -> bool {
        self.page < self.total_pages
    }

    /// Check if there's a previous page.
    pub fn has_previous(&self) -> bool {
        self.page > 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestQuery {
        id: String,
    }

    impl Query for TestQuery {
        type Result = String;
    }

    struct TestHandler;

    #[async_trait]
    impl QueryHandler<TestQuery> for TestHandler {
        type Error = std::io::Error;

        async fn handle(&self, query: TestQuery) -> Result<String, Self::Error> {
            Ok(format!("Result for {}", query.id))
        }
    }

    #[tokio::test]
    async fn test_query_handler() {
        let handler = TestHandler;
        let query = TestQuery {
            id: "123".to_string(),
        };
        let result = handler.handle(query).await.unwrap();
        assert_eq!(result, "Result for 123");
    }

    #[test]
    fn test_paginated_result() {
        let result: PaginatedQueryResult<i32> = PaginatedQueryResult::new(vec![1, 2, 3], 10, 1, 3);

        assert_eq!(result.total_pages, 4);
        assert!(result.has_next());
        assert!(!result.has_previous());
    }
}
