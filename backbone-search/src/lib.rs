//! Backbone Framework Search Module
//!
//! Provides search functionality with Elasticsearch and Algolia support.
//!
//! ## Features
//!
//! - **Elasticsearch Backend**: Full-text search with advanced queries
//! - **Algolia Backend**: Fast search-as-a-service integration
//! - **Index Management**: Create, update, delete search indices
//! - **Document Operations**: Index, update, delete documents
//! - **Advanced Search**: Full-text, fuzzy, faceted, and geo search
//! - **Analytics**: Search analytics and performance metrics
//! - **Async/Await**: Full async support with tokio
//! - **Multi-index Support**: Search across multiple indices
//!
//! ## Quick Start
//!
//! ```rust
//! use backbone_search::{SearchService, ElasticsearchSearch, SearchQuery, SearchDocument};
//!
//! // Elasticsearch search service
//! let search = ElasticsearchSearch::new("http://localhost:9200").await?;
//!
//! // Create index
//! search.create_index("products", None).await?;
//!
//! // Index document
//! let doc = SearchDocument::builder()
//!     .id("product-1")
//!     .content("Wireless headphones with noise cancellation")
//!     .field("category", "electronics")
//!     .field("price", 299.99)
//!     .build();
//!
//! search.index_document("products", doc).await?;
//!
//! // Search documents
//! let query = SearchQuery::builder()
//!     .text("headphones")
//!     .filter("category", "electronics")
//!     .build();
//!
//! let results = search.search("products", query).await?;
//! ```

pub mod elasticsearch;
pub mod algolia;
pub mod traits;
pub mod types;

pub use traits::*;
pub use types::*;
pub use elasticsearch::*;
pub use algolia::*;

/// Search module version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default index name
pub const DEFAULT_INDEX_NAME: &str = "default";

/// Maximum document size (100MB for Elasticsearch)
pub const MAX_DOCUMENT_SIZE: usize = 100 * 1024 * 1024;

/// Default search limit
pub const DEFAULT_SEARCH_LIMIT: usize = 100;

/// Maximum search limit
pub const MAX_SEARCH_LIMIT: usize = 10000;

/// Search error types
#[derive(thiserror::Error, Debug)]
pub enum SearchError {
    #[error("Elasticsearch connection error: {0}")]
    ElasticsearchConnection(String),

    #[error("Elasticsearch operation error: {0}")]
    ElasticsearchOperation(String),

    #[error("Algolia API error: {0}")]
    AlgoliaError(String),

    #[error("Index not found: {0}")]
    IndexNotFound(String),

    #[error("Document not found: {0}")]
    DocumentNotFound(String),

    #[error("Invalid query: {0}")]
    InvalidQuery(String),

    #[error("Document serialization error: {0}")]
    Serialization(String),

    #[error("Document deserialization error: {0}")]
    Deserialization(String),

    #[error("Document too large: {size} bytes (max: {max} bytes)")]
    DocumentTooLarge { size: usize, max: usize },

    #[error("Invalid index name: {0}")]
    InvalidIndexName(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Search timeout error")]
    TimeoutError,

    #[error("Search rate limit exceeded")]
    RateLimitExceeded,

    #[error("Search error: {0}")]
    Other(String),
}

/// Result type for search operations
pub type SearchResult<T> = Result<T, SearchError>;

/// Search configuration
#[derive(Debug, Clone)]
pub struct SearchConfig {
    /// Default search limit
    pub default_limit: usize,

    /// Maximum search limit
    pub max_limit: usize,

    /// Enable search analytics
    pub enable_analytics: bool,

    /// Search timeout in milliseconds
    pub timeout_ms: u64,

    /// Enable fuzzy search by default
    pub enable_fuzzy_search: bool,

    /// Fuzziness level (0.0 to 1.0)
    pub fuzziness: f32,

    /// Minimum should match for multi-term queries
    pub minimum_should_match: String,

    /// Enable highlighting
    pub enable_highlighting: bool,

    /// Highlight pre tag
    pub highlight_pre_tag: String,

    /// Highlight post tag
    pub highlight_post_tag: String,

    /// Enable snippet extraction
    pub enable_snippets: bool,

    /// Snippet length
    pub snippet_length: usize,

    /// Enable search suggestions
    pub enable_suggestions: bool,

    /// Suggestion limit
    pub suggestion_limit: usize,

    /// Enable geospatial search
    pub enable_geo_search: bool,

    /// Default coordinate reference system
    pub default_crs: String,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            default_limit: DEFAULT_SEARCH_LIMIT,
            max_limit: MAX_SEARCH_LIMIT,
            enable_analytics: true,
            timeout_ms: 5000,
            enable_fuzzy_search: true,
            fuzziness: 0.7,
            minimum_should_match: "75%".to_string(),
            enable_highlighting: true,
            highlight_pre_tag: "<mark>".to_string(),
            highlight_post_tag: "</mark>".to_string(),
            enable_snippets: true,
            snippet_length: 200,
            enable_suggestions: true,
            suggestion_limit: 10,
            enable_geo_search: false,
            default_crs: "EPSG4326".to_string(),
        }
    }
}

/// Search backend types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchBackend {
    Elasticsearch,
    Algolia,
}

impl SearchBackend {
    /// Get backend name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Elasticsearch => "elasticsearch",
            Self::Algolia => "algolia",
        }
    }
}

/// Search statistics
#[derive(Debug, Clone)]
pub struct SearchStats {
    /// Total number of queries
    pub total_queries: u64,

    /// Total number of documents indexed
    pub total_documents: u64,

    /// Average query time in milliseconds
    pub avg_query_time_ms: f64,

    /// Queries per second
    pub queries_per_second: f64,

    /// Index size in bytes
    pub index_size_bytes: Option<u64>,

    /// Number of indices
    pub total_indices: u64,

    /// Last query timestamp
    pub last_query_at: Option<chrono::DateTime<chrono::Utc>>,

    /// Backend-specific statistics
    pub backend_stats: std::collections::HashMap<String, serde_json::Value>,
}

impl Default for SearchStats {
    fn default() -> Self {
        Self {
            total_queries: 0,
            total_documents: 0,
            avg_query_time_ms: 0.0,
            queries_per_second: 0.0,
            index_size_bytes: None,
            total_indices: 0,
            last_query_at: None,
            backend_stats: std::collections::HashMap::new(),
        }
    }
}

impl SearchStats {
    /// Update statistics with query time
    pub fn update_with_query(&mut self, query_time_ms: u64) {
        self.total_queries += 1;

        // Update average query time
        let total_time = self.avg_query_time_ms * (self.total_queries - 1) as f64 + query_time_ms as f64;
        self.avg_query_time_ms = total_time / self.total_queries as f64;

        // Update last query timestamp
        use chrono::Utc;
        self.last_query_at = Some(Utc::now());
    }

    /// Calculate queries per second
    pub fn update_queries_per_second(&mut self, time_window_seconds: u64) {
        if time_window_seconds > 0 {
            self.queries_per_second = self.total_queries as f64 / time_window_seconds as f64;
        }
    }
}