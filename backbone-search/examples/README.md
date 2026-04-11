# Backbone Search Examples

This directory contains comprehensive examples demonstrating how to use the `backbone-search` crate for various search scenarios and use cases.

## 📋 Available Examples

### 1. [Basic Usage](./basic_usage.rs)
**Getting started with backbone-search**

Demonstrates fundamental search operations including:
- Setting up Elasticsearch and Algolia clients
- Basic CRUD operations (Create, Read, Update, Delete)
- Simple search queries
- Bulk operations
- Error handling patterns

**Key features shown:**
- Client initialization for both backends
- Document creation and indexing
- Search query building
- Result processing
- Connection testing

### 2. [Advanced Search](./advanced_search.rs)
**Complex search patterns and features**

Covers advanced search functionality:
- Complex queries with multiple filters
- Faceted search implementation
- Geospatial search capabilities
- Search suggestions and autocomplete
- Analytics and statistics
- Multi-index search
- Performance optimization techniques

**Key features shown:**
- Advanced query builders
- Filter combinations
- Geospatial indexing and queries
- Autocomplete implementation
- Analytics dashboard
- Cross-index searching

### 3. [Elasticsearch Setup](./elasticsearch_setup.rs)
**Complete Elasticsearch configuration**

Comprehensive Elasticsearch setup and configuration:
- Basic and cluster configuration
- Authentication methods (Basic, API Key, AWS)
- Index configuration and mappings
- Custom analyzers and settings
- Performance optimization
- Hardware recommendations

**Key features shown:**
- Multi-node cluster setup
- Security configuration
- Index lifecycle management
- Performance tuning
- Monitoring setup
- Best practices

### 4. [Algolia Setup](./algolia_setup.rs)
**Complete Algolia configuration**

Comprehensive Algolia setup and configuration:
- Authentication and security best practices
- Index configuration and settings
- Query rules and personalization
- Analytics and A/B testing
- Performance optimization
- Cost optimization strategies

**Key features shown:**
- API key management
- Secured API keys
- Query rules implementation
- Personalization strategies
- Analytics configuration
- Performance tuning

### 5. [Testing Examples](./testing_examples.rs)
**Comprehensive testing strategies**

Complete testing framework for search functionality:
- Unit testing patterns
- Integration testing
- Performance testing (load, latency, throughput)
- Mock testing approaches
- End-to-end testing
- Accessibility testing

**Key features shown:**
- Test data management
- Mock services
- Performance benchmarking
- Error scenario testing
- User journey testing
- Cross-platform testing

## 🚀 Getting Started

### Prerequisites

1. **Rust** (1.70+)
2. **Elasticsearch** (for Elasticsearch examples)
   ```bash
   # Using Docker
   docker run -d --name elasticsearch -p 9200:9200 -e "discovery.type=single-node" elasticsearch:8.15.0
   ```

3. **Algolia Account** (for Algolia examples)
   - Sign up at [algolia.com](https://www.algolia.com)
   - Get your App ID and API Key

### Running Examples

1. **Clone the repository:**
   ```bash
   git clone <repository-url>
   cd monorepo-backbone
   ```

2. **Install dependencies:**
   ```bash
   cargo build --examples
   ```

3. **Run specific examples:**
   ```bash
   # Basic usage
   cargo run --example basic_usage

   # Advanced search
   cargo run --example advanced_search

   # Elasticsearch setup
   cargo run --example elasticsearch_setup

   # Algolia setup
   cargo run --example algolia_setup

   # Testing examples
   cargo run --example testing_examples
   ```

### Environment Setup

Create a `.env` file in the project root for configuration:

```env
# Elasticsearch Configuration
ELASTICSEARCH_URL=http://localhost:9200
ELASTICSEARCH_USERNAME=elastic
ELASTICSEARCH_PASSWORD=changeme

# Algolia Configuration
ALGOLIA_APP_ID=your_app_id_here
ALGOLIA_API_KEY=your_api_key_here
```

## 📖 Usage Patterns

### Basic Search Pattern

```rust
use backbone_search::{SearchService, ElasticsearchSearch, SearchQuery};

// Initialize search service
let search = ElasticsearchSearch::new("http://localhost:9200").await?;

// Create a search query
let query = SearchQuery::builder()
    .text("laptop computer")
    .filter("category", "electronics")
    .limit(10)
    .build();

// Execute search
let results = search.search("products", query).await?;
println!("Found {} results", results.total_hits);
```

### Document Indexing Pattern

```rust
use backbone_search::{SearchDocument, SearchService};

// Create a document
let doc = SearchDocument::builder()
    .id("product-123")
    .title("Wireless Laptop")
    .content("High-performance wireless laptop")
    .price(999.99)
    .category("electronics")
    .build();

// Index the document
let doc_id = search.index_document("products", doc).await?;
```

### Advanced Query Pattern

```rust
// Complex query with multiple filters
let query = SearchQuery::builder()
    .text("gaming laptop")
    .filter("category", "electronics")
    .filter("price_range", "1000-2000")
    .filter("brand", "ASUS")
    .sort_by("rating")
    .facets(vec!["brand".to_string(), "price_range".to_string()])
    .limit(20)
    .build();
```

## 🧪 Testing

Run the comprehensive test suite:

```bash
# Run all tests
cargo test

# Run tests with examples
cargo test --examples

# Run specific example tests
cargo test --example testing_examples
```

## 📚 Learning Path

### Beginner (1-2 weeks)
1. Start with `basic_usage.rs`
2. Understand document structures
3. Master basic search queries
4. Learn error handling

### Intermediate (2-4 weeks)
1. Study `advanced_search.rs`
2. Implement faceted search
3. Add geospatial capabilities
4. Set up analytics

### Advanced (1-2 months)
1. Complete setup configurations
2. Implement comprehensive testing
3. Optimize performance
4. Deploy to production

## 🔧 Customization

### Custom Analyzers (Elasticsearch)

```rust
let custom_analyzer = json!({
    "analysis": {
        "analyzer": {
            "my_analyzer": {
                "type": "custom",
                "tokenizer": "standard",
                "filter": ["lowercase", "stop"]
            }
        }
    }
});
```

### Custom Ranking (Algolia)

```rust
let custom_ranking = vec![
    "desc(rating)",
    "desc(created_at)",
    "asc(price)"
];
```

### Custom Document Types

```rust
#[derive(Serialize, Deserialize)]
struct ProductDocument {
    #[serde(flatten)]
    base: SearchDocument,
    sku: String,
    inventory_count: u32,
    variants: Vec<ProductVariant>,
}

impl From<ProductDocument> for SearchDocument {
    fn from(product: ProductDocument) -> Self {
        // Conversion logic
        todo!()
    }
}
```

## 🚀 Production Deployment

### Elasticsearch Production Checklist

- [ ] Configure proper sharding strategy
- [ ] Set up replica shards for high availability
- [ ] Implement index lifecycle management
- [ ] Configure monitoring and alerts
- [ ] Set up backup and restore
- [ ] Optimize hardware allocation

### Algolia Production Checklist

- [ ] Use appropriate API keys (search-only for clients)
- [ ] Implement secured API keys for user-specific access
- [ ] Set up query rules for business logic
- [ ] Configure analytics and monitoring
- [ ] Implement A/B testing for ranking optimization
- [ ] Set up cost optimization strategies

## 🤝 Contributing

Contributions to the examples are welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Add your example with documentation
4. Include tests if applicable
5. Submit a pull request

## 📄 License

These examples are part of the backbone-search crate and follow the same license terms.

## 🔗 Additional Resources

- [Elasticsearch Documentation](https://www.elastic.co/guide/)
- [Algolia Documentation](https://www.algolia.com/doc/)
- [Rust Async Book](https://rust-lang.github.io/async-book/)
- [Tokio Documentation](https://tokio.rs/docs)

## 🆘 Support

If you encounter issues or have questions:

1. Check the [troubleshooting guide](TROUBLESHOOTING.md)
2. Review existing [GitHub issues](https://github.com/your-repo/issues)
3. Create a new issue with detailed information
4. Include example code and error messages

Happy searching! 🎉