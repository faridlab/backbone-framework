//! Basic usage examples for backbone-search
//!
//! This example demonstrates how to use both Elasticsearch and Algolia backends
//! for basic search operations including CRUD and search functionality.

use backbone_search::{
    SearchService, ElasticsearchSearch, AlgoliaSearch,
    SearchDocument, SearchQuery, SearchResult
};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("🔍 Backbone Search Examples");
    println!("============================");

    // Example 1: Elasticsearch Backend
    println!("\n1. Elasticsearch Backend Example:");
    elasticsearch_example().await?;

    // Example 2: Algolia Backend
    println!("\n2. Algolia Backend Example:");
    algolia_example().await?;

    // Example 3: Search Operations
    println!("\n3. Search Operations Example:");
    search_operations_example().await?;

    // Example 4: Bulk Operations
    println!("\n4. Bulk Operations Example:");
    bulk_operations_example().await?;

    Ok(())
}

/// Elasticsearch backend example
async fn elasticsearch_example() -> SearchResult<()> {
    // Create Elasticsearch client
    let es_search = ElasticsearchSearch::new("http://localhost:9200").await?;

    // Create an index
    println!("  Creating index 'articles'...");
    es_search.create_index("articles", None).await?;
    println!("  ✅ Index created successfully");

    // Test connection
    println!("  Testing connection...");
    let is_connected = es_search.test_connection().await?;
    println!("  ✅ Connection status: {}", if is_connected { "Connected" } else { "Disconnected" });

    // Index a document
    println!("  Indexing a document...");
    let doc = SearchDocument::builder()
        .id("article-1")
        .title("Getting Started with Rust")
        .content("Rust is a systems programming language that runs blazingly fast...")
        .author("John Doe")
        .tags(vec!["rust".to_string(), "programming".to_string()])
        .published_date("2024-01-15")
        .build();

    let doc_id = es_search.index_document("articles", doc).await?;
    println!("  ✅ Document indexed with ID: {}", doc_id);

    Ok(())
}

/// Algolia backend example
async fn algolia_example() -> SearchResult<()> {
    // Note: Replace with your actual Algolia credentials
    let app_id = "YOUR_APP_ID";
    let api_key = "YOUR_API_KEY";

    // Create Algolia client
    let algolia_search = AlgoliaSearch::new(app_id, api_key);

    // Create an index
    println!("  Creating index 'products'...");
    algolia_search.create_index("products", None).await?;
    println!("  ✅ Index created successfully");

    // Test connection
    println!("  Testing connection...");
    let is_connected = algolia_search.test_connection().await?;
    println!("  ✅ Connection status: {}", if is_connected { "Connected" } else { "Disconnected" });

    // Index a document
    println!("  Indexing a document...");
    let doc = SearchDocument::builder()
        .id("product-1")
        .title("Wireless Headphones")
        .content("High-quality wireless headphones with noise cancellation...")
        .price(299.99)
        .category("electronics")
        .brand("TechBrand")
        .in_stock(true)
        .rating(4.5)
        .build();

    let doc_id = algolia_search.index_document("products", doc).await?;
    println!("  ✅ Document indexed with ID: {}", doc_id);

    Ok(())
}

/// Search operations example
async fn search_operations_example() -> SearchResult<()> {
    // For this example, we'll use a mock search service
    // In real usage, replace with actual ElasticsearchSearch or AlgoliaSearch

    println!("  Creating sample documents for search...");

    // Sample search queries
    let queries = vec![
        ("Basic text search", "rust programming"),
        ("Title search", "Getting Started"),
        ("Author search", "John Doe"),
        ("Tag search", "electronics"),
        ("Price range search", "price:100-500"),
    ];

    for (description, query_text) in queries {
        println!("  📝 {}: '{}'", description, query_text);

        // Create search query
        let query = SearchQuery::builder()
            .text(query_text)
            .limit(10)
            .offset(0)
            .build();

        // In real usage, you would execute:
        // let results = search_service.search("articles", query).await?;
        // println!("    Found {} results", results.total_hits);

        println!("    ✅ Query constructed successfully");
    }

    Ok(())
}

/// Bulk operations example
async fn bulk_operations_example() -> SearchResult<()> {
    println!("  Creating bulk documents...");

    // Create multiple documents for bulk indexing
    let documents = vec![
        SearchDocument::builder()
            .id("doc-1")
            .title("First Document")
            .content("Content of first document")
            .build(),
        SearchDocument::builder()
            .id("doc-2")
            .title("Second Document")
            .content("Content of second document")
            .build(),
        SearchDocument::builder()
            .id("doc-3")
            .title("Third Document")
            .content("Content of third document")
            .build(),
    ];

    println!("  📄 Prepared {} documents for bulk operations", documents.len());

    // In real usage, you would perform bulk operations:
    // let results = search_service.index_documents("articles", documents).await?;
    // println!("  ✅ Bulk operation completed: {} successful, {} failed",
    //          results.successful, results.failed);

    // Bulk update example
    println!("  Preparing bulk update operations...");

    // In real usage, you would create bulk operations:
    // use backbone_search::{BulkOperation, BulkOperationType};
    //
    // let operations = vec![
    //     BulkOperation {
    //         operation_type: BulkOperationType::Update,
    //         id: "doc-1".to_string(),
    //         document: None,
    //         updates: Some(json!({"title": "Updated First Document"})),
    //         routing: None,
    //         timestamp: None,
    //     },
    //     // ... more operations
    // ];
    //
    // let results = search_service.bulk_operation("articles", operations).await?;
    // println!("  ✅ Bulk update completed");

    Ok(())
}

/// Helper function to demonstrate error handling
async fn demonstrate_error_handling() -> SearchResult<()> {
    println!("  Demonstrating error handling...");

    // Example of handling various error types
    match ElasticsearchSearch::new("http://invalid-host:9200").await {
        Ok(_) => println!("  ✅ Unexpected success"),
        Err(e) => println!("  ❌ Expected error: {}", e),
    }

    // Example of handling index not found
    // In real usage:
    // let search = ElasticsearchSearch::new("http://localhost:9200").await?;
    // match search.get_document("nonexistent_index", "doc-id").await {
    //     Ok(Some(doc)) => println!("  ✅ Found document: {:?}", doc.id),
    //     Ok(None) => println!("  ℹ️ Document not found (expected)"),
    //     Err(e) => println!("  ❌ Error: {}", e),
    // }

    Ok(())
}