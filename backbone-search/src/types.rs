//! Search types and structures

use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Search document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchDocument {
    /// Document identifier
    pub id: String,

    /// Document content (full text)
    pub content: String,

    /// Document title
    pub title: Option<String>,

    /// Document fields
    pub fields: HashMap<String, serde_json::Value>,

    /// Document metadata
    pub metadata: DocumentMetadata,

    /// Document timestamp
    pub timestamp: DateTime<Utc>,

    /// Document tags
    pub tags: Vec<String>,

    /// Document language
    pub language: Option<String>,

    /// Boost factor for search relevance
    pub boost: f32,
}

/// Document metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    /// Content type (MIME type)
    pub content_type: Option<String>,

    /// File size in bytes
    pub file_size: Option<u64>,

    /// Source URL or reference
    pub source: Option<String>,

    /// Author or creator
    pub author: Option<String>,

    /// Document category
    pub category: Option<String>,

    /// Keywords
    pub keywords: Vec<String>,

    /// Document permissions
    pub permissions: Vec<String>,

    /// Custom attributes
    pub attributes: HashMap<String, String>,
}

impl Default for DocumentMetadata {
    fn default() -> Self {
        Self {
            content_type: None,
            file_size: None,
            source: None,
            author: None,
            category: None,
            keywords: Vec::new(),
            permissions: Vec::new(),
            attributes: HashMap::new(),
        }
    }
}

/// Search query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    /// Text search query
    pub text: Option<String>,

    /// Field-specific queries
    pub fields: Vec<FieldQuery>,

    /// Filters
    pub filters: Vec<Filter>,

    /// Sorting
    pub sort: Vec<SortField>,

    /// Pagination
    pub pagination: Pagination,

    /// Search options
    pub options: SearchOptions,

    /// Facets to compute
    pub facets: Vec<FacetConfig>,

    /// Aggregations
    pub aggregations: Vec<Aggregation>,
}

/// Field-specific query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldQuery {
    /// Field name
    pub field: String,

    /// Query text
    pub query: String,

    /// Query operator
    pub operator: QueryOperator,

    /// Boost factor
    pub boost: Option<f32>,

    /// Fuzziness
    pub fuzziness: Option<f32>,
}

/// Query operator
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryOperator {
    And,
    Or,
    Not,
    Phrase,
    Prefix,
    Wildcard,
    Fuzzy,
}

/// Search filter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    /// Field name
    pub field: String,

    /// Filter value
    pub value: FilterValue,

    /// Filter operator
    pub operator: FilterOperator,
}

/// Filter value
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FilterValue {
    String(String),
    Number(f64),
    Boolean(bool),
    Array(Vec<serde_json::Value>),
    Range(FilterRange),
    Exists(String),
}

/// Range filter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterRange {
    pub gte: Option<f64>,
    pub gt: Option<f64>,
    pub lte: Option<f64>,
    pub lt: Option<f64>,
}

/// Filter operator
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterOperator {
    Equals,
    NotEquals,
    In,
    NotIn,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    Exists,
    NotExists,
    Contains,
    StartsWith,
    EndsWith,
    GeoWithin,
    GeoDistance,
}

/// Sort field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SortField {
    /// Field name
    pub field: String,

    /// Sort order
    pub order: SortOrder,

    /// Sort mode
    pub mode: Option<SortMode>,

    /// Distance unit for geo sorting
    pub unit: Option<String>,

    /// Location for geo sorting
    pub location: Option<GeoPoint>,
}

/// Sort order
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortOrder {
    Asc,
    Desc,
}

/// Sort mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortMode {
    Min,
    Max,
    Sum,
    Avg,
}

/// Geo point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoPoint {
    pub lat: f64,
    pub lon: f64,
}

/// Pagination
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Pagination {
    /// Page number (1-based)
    pub page: u32,

    /// Page size
    pub size: u32,

    /// Offset (for offset-based pagination)
    pub offset: Option<u32>,
}

/// Search options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchOptions {
    /// Enable fuzzy search
    pub fuzzy: bool,

    /// Fuzziness level
    pub fuzziness: f32,

    /// Minimum should match
    pub minimum_should_match: String,

    /// Enable highlighting
    pub highlight: bool,

    /// Highlight configuration
    pub highlight_config: Option<HighlightConfig>,

    /// Enable snippets
    pub snippets: bool,

    /// Snippet length
    pub snippet_length: usize,

    /// Enable suggestions
    pub suggestions: bool,

    /// Suggestion limit
    pub suggestion_limit: usize,

    /// Boost query
    pub boost_query: Option<Box<SearchQuery>>,

    /// Search timeout in milliseconds
    pub timeout_ms: Option<u64>,

    /// Track total hits
    pub track_total_hits: bool,

    /// Explain scores
    pub explain: bool,
}

/// Highlight configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighlightConfig {
    /// Pre tag
    pub pre_tag: String,

    /// Post tag
    pub post_tag: String,

    /// Fields to highlight
    pub fields: Vec<String>,

    /// Fragment size
    pub fragment_size: usize,

    /// Number of fragments
    pub number_of_fragments: usize,
}

/// Facet configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacetConfig {
    /// Field name
    pub field: String,

    /// Facet type
    pub facet_type: FacetType,

    /// Size (number of buckets)
    pub size: u32,

    /// Filter for facet calculation
    pub filter: Option<Box<Filter>>,

    /// Sort order for facets
    pub sort: Option<FacetSort>,
}

/// Facet type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FacetType {
    Terms,
    Range,
    DateRange,
    Histogram,
    Stats,
}

/// Facet sort
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FacetSort {
    Count,
    Key,
    Alpha,
}

/// Aggregation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Aggregation {
    /// Aggregation name
    pub name: String,

    /// Aggregation type
    pub agg_type: AggregationType,

    /// Field to aggregate
    pub field: String,

    /// Aggregation parameters
    pub params: HashMap<String, serde_json::Value>,
}

/// Aggregation type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AggregationType {
    Terms,
    DateHistogram,
    Histogram,
    Stats,
    ExtendedStats,
    Min,
    Max,
    Sum,
    Avg,
    Cardinality,
    Percentiles,
}

/// Search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResults {
    /// Total number of hits
    pub total_hits: u64,

    /// Maximum score
    pub max_score: Option<f64>,

    /// Documents
    pub hits: Vec<SearchHit>,

    /// Aggregations
    pub aggregations: HashMap<String, serde_json::Value>,

    /// Facets
    pub facets: HashMap<String, FacetResult>,

    /// Suggestions
    pub suggestions: Vec<Suggestion>,

    /// Search metadata
    pub metadata: SearchMetadata,
}

/// Search hit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    /// Document
    pub document: SearchDocument,

    /// Relevance score
    pub score: Option<f64>,

    /// Highlighted fragments
    pub highlights: HashMap<String, Vec<String>>,

    /// Snippets
    pub snippets: HashMap<String, String>,

    /// Sort values
    pub sort_values: Vec<serde_json::Value>,

    /// Matched queries
    pub matched_queries: Vec<String>,

    /// Inner hits (for nested documents)
    pub inner_hits: HashMap<String, Vec<SearchHit>>,

    /// Explanation
    pub explanation: Option<serde_json::Value>,
}

/// Facet result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacetResult {
    /// Facet buckets
    pub buckets: Vec<FacetBucket>,

    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Facet bucket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacetBucket {
    /// Bucket key
    pub key: serde_json::Value,

    /// Document count
    pub count: u64,

    /// Bucket sub-aggregations
    pub sub_aggregations: HashMap<String, serde_json::Value>,
}

/// Search suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    /// Suggestion text
    pub text: String,

    /// Suggestion type
    pub suggestion_type: SuggestionType,

    /// Score
    pub score: f64,

    /// Additional information
    pub info: HashMap<String, serde_json::Value>,
}

/// Suggestion type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SuggestionType {
    Term,
    Phrase,
    Completion,
    DidYouMean,
}

/// Search metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMetadata {
    /// Query time in milliseconds
    pub query_time_ms: u64,

    /// Number of shards searched
    pub total_shards: u32,

    /// Number of successful shards
    pub successful_shards: u32,

    /// Number of failed shards
    pub failed_shards: u32,

    /// Search scroll ID (if applicable)
    pub scroll_id: Option<String>,

    /// Backend-specific metadata
    pub backend_metadata: HashMap<String, serde_json::Value>,
}

impl SearchDocument {
    /// Create new document builder
    pub fn builder() -> SearchDocumentBuilder {
        SearchDocumentBuilder::new()
    }

    /// Get field value
    pub fn get_field<T>(&self, field: &str) -> Option<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        self.fields.get(field)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Check if document has field
    pub fn has_field(&self, field: &str) -> bool {
        self.fields.contains_key(field)
    }

    /// Get all field names
    pub fn field_names(&self) -> Vec<String> {
        self.fields.keys().cloned().collect()
    }
}

/// Search document builder
pub struct SearchDocumentBuilder {
    document: SearchDocument,
}

impl SearchDocumentBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            document: SearchDocument {
                id: Uuid::new_v4().to_string(),
                content: String::new(),
                title: None,
                fields: HashMap::new(),
                metadata: DocumentMetadata::default(),
                timestamp: Utc::now(),
                tags: Vec::new(),
                language: None,
                boost: 1.0,
            },
        }
    }

    /// Set document ID
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.document.id = id.into();
        self
    }

    /// Set content
    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.document.content = content.into();
        self
    }

    /// Set title
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.document.title = Some(title.into());
        self
    }

    /// Add field
    pub fn field<T: Serialize>(mut self, name: impl Into<String>, value: T) -> Result<Self, serde_json::Error> {
        let json_value = serde_json::to_value(value)?;
        self.document.fields.insert(name.into(), json_value);
        Ok(self)
    }

    /// Add JSON field
    pub fn json_field(mut self, name: impl Into<String>, value: serde_json::Value) -> Self {
        self.document.fields.insert(name.into(), value);
        self
    }

    /// Set metadata
    pub fn metadata(mut self, metadata: DocumentMetadata) -> Self {
        self.document.metadata = metadata;
        self
    }

    /// Add tag
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.document.tags.push(tag.into());
        self
    }

    /// Add multiple tags
    pub fn tags(mut self, tags: Vec<String>) -> Self {
        self.document.tags.extend(tags);
        self
    }

    /// Set language
    pub fn language(mut self, language: impl Into<String>) -> Self {
        self.document.language = Some(language.into());
        self
    }

    /// Set boost
    pub fn boost(mut self, boost: f32) -> Self {
        self.document.boost = boost;
        self
    }

    /// Build the search document
    pub fn build(self) -> SearchDocument {
        self.document
    }
}

impl Default for SearchDocumentBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchQuery {
    /// Create new query builder
    pub fn builder() -> SearchQueryBuilder {
        SearchQueryBuilder::new()
    }
}

/// Search query builder
pub struct SearchQueryBuilder {
    query: SearchQuery,
}

impl SearchQueryBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            query: SearchQuery {
                text: None,
                fields: Vec::new(),
                filters: Vec::new(),
                sort: Vec::new(),
                pagination: Pagination {
                    page: 1,
                    size: 20,
                    offset: None,
                },
                options: SearchOptions {
                    fuzzy: false,
                    fuzziness: 0.7,
                    minimum_should_match: "75%".to_string(),
                    highlight: true,
                    highlight_config: None,
                    snippets: true,
                    snippet_length: 200,
                    suggestions: false,
                    suggestion_limit: 10,
                    boost_query: None,
                    timeout_ms: None,
                    track_total_hits: true,
                    explain: false,
                },
                facets: Vec::new(),
                aggregations: Vec::new(),
            },
        }
    }

    /// Set text query
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.query.text = Some(text.into());
        self
    }

    /// Add field query
    pub fn field_query(mut self, field: impl Into<String>, query: impl Into<String>) -> Self {
        self.query.fields.push(FieldQuery {
            field: field.into(),
            query: query.into(),
            operator: QueryOperator::And,
            boost: None,
            fuzziness: None,
        });
        self
    }

    /// Add filter
    pub fn filter(mut self, filter: Filter) -> Self {
        self.query.filters.push(filter);
        self
    }

    /// Add simple filter
    pub fn field_filter(mut self, field: impl Into<String>, value: impl Into<FilterValue>) -> Self {
        self.query.filters.push(Filter {
            field: field.into(),
            value: value.into(),
            operator: FilterOperator::Equals,
        });
        self
    }

    /// Add sort field
    pub fn sort(mut self, field: impl Into<String>, order: SortOrder) -> Self {
        self.query.sort.push(SortField {
            field: field.into(),
            order,
            mode: None,
            unit: None,
            location: None,
        });
        self
    }

    /// Set pagination
    pub fn pagination(mut self, page: u32, size: u32) -> Self {
        self.query.pagination = Pagination {
            page,
            size,
            offset: None,
        };
        self
    }

    /// Set limit (for convenience)
    pub fn limit(mut self, size: u32) -> Self {
        self.query.pagination.size = size;
        self
    }

    /// Enable fuzzy search
    pub fn fuzzy(mut self, enabled: bool) -> Self {
        self.query.options.fuzzy = enabled;
        self
    }

    /// Set fuzziness
    pub fn fuzziness(mut self, fuzziness: f32) -> Self {
        self.query.options.fuzziness = fuzziness;
        self
    }

    /// Enable highlighting
    pub fn highlight(mut self, enabled: bool) -> Self {
        self.query.options.highlight = enabled;
        self
    }

    /// Add facet
    pub fn facet(mut self, facet: FacetConfig) -> Self {
        self.query.facets.push(facet);
        self
    }

    /// Add aggregation
    pub fn aggregation(mut self, aggregation: Aggregation) -> Self {
        self.query.aggregations.push(aggregation);
        self
    }

    /// Build the search query
    pub fn build(self) -> SearchQuery {
        self.query
    }
}

impl Default for SearchQueryBuilder {
    fn default() -> Self {
        Self::new()
    }
}