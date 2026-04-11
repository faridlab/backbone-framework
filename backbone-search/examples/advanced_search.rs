//! Advanced search examples for backbone-search
//!
//! This example demonstrates advanced search features including:
//! - Complex queries with filters and aggregations
//! - Faceted search
//! - Geospatial search
//! - Search suggestions
//! - Analytics and statistics

use backbone_search::{
    SearchService, SearchQuery, SearchDocument, Filter, SortOrder,
    SearchResult, SearchBackend, ElasticsearchSearch, AlgoliaSearch
};
use serde_json::json;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    println!("🔍 Advanced Search Examples");
    println!("============================");

    // Example 1: Complex Queries with Filters
    println!("\n1. Complex Queries with Filters:");
    complex_filters_example().await?;

    // Example 2: Faceted Search
    println!("\n2. Faceted Search Example:");
    faceted_search_example().await?;

    // Example 3: Geospatial Search
    println!("\n3. Geospatial Search Example:");
    geospatial_search_example().await?;

    // Example 4: Search Suggestions
    println!("\n4. Search Suggestions Example:");
    suggestions_example().await?;

    // Example 5: Analytics and Statistics
    println!("\n5. Analytics and Statistics Example:");
    analytics_example().await?;

    // Example 6: Multi-index Search
    println!("\n6. Multi-index Search Example:");
    multi_index_search_example().await?;

    Ok(())
}

/// Example of complex queries with multiple filters
async fn complex_filters_example() -> SearchResult<()> {
    println!("  Building complex search queries...");

    // Example 1: Multiple field filters
    let query = SearchQuery::builder()
        .text("laptop computer")
        .filter("category", "electronics")
        .filter("price_range", "500-2000")
        .filter("brand", "Apple")
        .filter("in_stock", "true")
        .limit(20)
        .build();

    println!("  🔍 Multi-filter query: {}", query.text.as_ref().unwrap());
    println!("     Filters: category=electronics, price=500-2000, brand=Apple, in_stock=true");

    // Example 2: Range queries
    let query = SearchQuery::builder()
        .text("smartphone")
        .filter("price_min", "200")
        .filter("price_max", "1000")
        .filter("rating_min", "4.0")
        .filter("release_date", "2023-01-01..2024-12-31")
        .sort_by("rating")
        .sort_order(SortOrder::Desc)
        .build();

    println!("  📊 Range query: {}", query.text.as_ref().unwrap());
    println!("     Price range: $200-$1000, Rating: 4.0+, Released: 2023-2024");

    // Example 3: Boolean logic with filters
    let query = SearchQuery::builder()
        .text("gaming laptop")
        .filter("gpu_type", "RTX")
        .filter("ram_gb", "16+")
        .filter("storage_type", "SSD")
        .filter("has_gsync", "true")
        .limit(10)
        .build();

    println!("  🎮 Gaming laptop query with specific hardware requirements");
    println!("     GPU: RTX, RAM: 16GB+, Storage: SSD, G-Sync: true");

    Ok(())
}

/// Example of faceted search implementation
async fn faceted_search_example() -> SearchResult<()> {
    println!("  Setting up faceted search...");

    // Faceted search allows users to filter by categories, brands, price ranges, etc.
    let query = SearchQuery::builder()
        .text("headphones")
        .facets(vec!["category".to_string(), "brand".to_string(), "price_range".to_string()])
        .limit(50)
        .build();

    println!("  🔍 Faceted search for: 'headphones'");
    println!("     Facets: category, brand, price_range");

    // Mock faceted results
    let mock_facets = HashMap::from([
        ("category", json!({
            "electronics": 245,
            "audio": 189,
            "wireless": 156,
            "noise-cancelling": 98
        })),
        ("brand", json!({
            "Sony": 67,
            "Bose": 54,
            "Apple": 43,
            "Sennheiser": 38,
            "JBL": 31
        })),
        ("price_range", json!({
            "$0-$50": 12,
            "$50-$100": 45,
            "$100-$200": 89,
            "$200-$500": 134,
            "$500+": 58
        }))
    ]);

    println!("  📊 Mock faceted results:");
    for (facet_name, facet_data) in mock_facets {
        println!("     {}: {}", facet_name, facet_data);
    }

    // Example of drilling down with facet filters
    println!("  🔎 Drilling down: wireless headphones under $200");
    let refined_query = SearchQuery::builder()
        .text("headphones")
        .filter("category", "wireless")
        .filter("price_range", "100-200")
        .facets(vec!["brand".to_string(), "features".to_string()])
        .limit(20)
        .build();

    println!("     Refined query applied with additional facet filters");

    Ok(())
}

/// Example of geospatial search functionality
async fn geospatial_search_example() -> SearchResult<()> {
    println!("  Setting up geospatial search...");

    // Example 1: Search within radius
    let query = SearchQuery::builder()
        .text("restaurants")
        .geo_distance("location", 37.7749, -122.4194, 5000.0) // 5km radius
        .limit(20)
        .build();

    println!("  🗺️ Restaurants within 5km of San Francisco (37.7749, -122.4194)");

    // Example 2: Bounding box search
    let query = SearchQuery::builder()
        .text("hotels")
        .geo_bbox("location", 37.7081, -122.4785, 37.8044, -122.3919) // SF bounds
        .filter("rating_min", "4.0")
        .limit(10)
        .build();

    println!("  🏨 Hotels in San Francisco with rating 4.0+");
    println!("     Bounding box: (37.7081, -122.4785) to (37.8044, -122.3919)");

    // Example 3: Sort by distance
    let query = SearchQuery::builder()
        .text("coffee shops")
        .geo_distance("location", 37.7749, -122.4194, 2000.0) // 2km radius
        .sort_by("distance")
        .sort_order(SortOrder::Asc)
        .limit(15)
        .build();

    println!("  ☕ Coffee shops within 2km, sorted by distance");

    // Document with geospatial data example
    let geo_doc = SearchDocument::builder()
        .id("place-1")
        .title("Golden Gate Park")
        .content("Large urban park in San Francisco")
        .location(json!({
            "lat": 37.7694,
            "lon": -122.4862,
            "type": "Point"
        }))
        .category("park")
        .city("San Francisco")
        .build();

    println!("  📍 Example geospatial document created for: {}", geo_doc.title);

    Ok(())
}

/// Example of search suggestions and autocomplete
async fn suggestions_example() -> SearchResult<()> {
    println!("  Setting up search suggestions...");

    // Get suggestions for partial queries
    let partial_queries = vec![
        "lapt", "smart", "wireless h", "gaming l", "blue",
    ];

    for query in partial_queries {
        println!("  💡 Suggestions for '{}':", query);

        // In real usage:
        // let suggestions = search_service.suggestions("products", query, 5).await?;
        // for (i, suggestion) in suggestions.iter().enumerate() {
        //     println!("     {}. {}", i + 1, suggestion);
        // }

        // Mock suggestions
        let mock_suggestions = match query {
            "lapt" => vec!["laptop", "laptop stand", "laptop bag", "laptop charger"],
            "smart" => vec!["smartphone", "smartwatch", "smart TV", "smart home"],
            "wireless h" => vec!["wireless headphones", "wireless headset", "wireless earbuds"],
            "gaming l" => vec!["gaming laptop", "gaming laptop 2024", "gaming laptop RTX"],
            "blue" => vec!["Bluetooth speaker", "Bluetooth headphones", "blue jeans"],
            _ => vec!["No suggestions"],
        };

        for (i, suggestion) in mock_suggestions.iter().enumerate() {
            if i < 5 { // Limit to 5 suggestions
                println!("     {}. {}", i + 1, suggestion);
            }
        }
    }

    // Example of building an autocomplete feature
    println!("  🔧 Building autocomplete feature...");
    println!("     - Configured for real-time suggestions");
    println!("     - Popularity-weighted results");
    println!("     - Category-specific suggestions");
    println!("     - Recent search history integration");

    Ok(())
}

/// Example of search analytics and statistics
async fn analytics_example() -> SearchResult<()> {
    println!("  Retrieving search analytics...");

    // Example analytics data
    let mock_analytics = json!({
        "total_searches": 15420,
        "avg_query_time_ms": 45.2,
        "popular_terms": [
            {"term": "laptop", "count": 2341, "ctr": 0.42},
            {"term": "smartphone", "count": 1876, "ctr": 0.38},
            {"term": "headphones", "count": 1543, "ctr": 0.51},
            {"term": "tablet", "count": 987, "ctr": 0.29},
            {"term": "smartwatch", "count": 765, "ctr": 0.33}
        ],
        "no_result_terms": [
            {"term": "quantum computer", "count": 45},
            {"term": "flying car", "count": 23},
            {"term": "time machine", "count": 12}
        ],
        "search_frequency": {
            "Monday": 2341,
            "Tuesday": 2567,
            "Wednesday": 2890,
            "Thursday": 3124,
            "Friday": 3456,
            "Saturday": 1234,
            "Sunday": 808
        }
    });

    println!("  📊 Search Analytics Dashboard:");
    println!("     Total searches: {}", mock_analytics["total_searches"]);
    println!("     Average query time: {}ms", mock_analytics["avg_query_time_ms"]);

    println!("  🔥 Top 5 Popular Search Terms:");
    if let Some(popular_terms) = mock_analytics["popular_terms"].as_array() {
        for (i, term) in popular_terms.iter().enumerate().take(5) {
            if let Some(term_data) = term.as_object() {
                let term_name = term_data.get("term").unwrap().as_str().unwrap();
                let count = term_data.get("count").unwrap().as_u64().unwrap();
                let ctr = term_data.get("ctr").unwrap().as_f64().unwrap();
                println!("     {}. '{}' ({} searches, {:.1}% CTR)", i + 1, term_name, count, ctr * 100.0);
            }
        }
    }

    println!("  ❌ Common 'No Result' Queries:");
    if let Some(no_result_terms) = mock_analytics["no_result_terms"].as_array() {
        for term in no_result_terms.iter().take(3) {
            if let Some(term_data) = term.as_object() {
                let term_name = term_data.get("term").unwrap().as_str().unwrap();
                let count = term_data.get("count").unwrap().as_u64().unwrap();
                println!("     - '{}' ({} times)", term_name, count);
            }
        }
    }

    // Performance metrics
    println!("  ⚡ Performance Metrics:");
    println!("     - Query time P95: 120ms");
    println!("     - Index size: 2.3GB");
    println!("     - Cache hit rate: 78%");
    println!("     - QPS: 45.2");

    Ok(())
}

/// Example of multi-index search
async fn multi_index_search_example() -> SearchResult<()> {
    println!("  Performing multi-index search...");

    // Search across multiple indices simultaneously
    let indices = vec![
        "products".to_string(),
        "articles".to_string(),
        "reviews".to_string(),
    ];

    let query = SearchQuery::builder()
        .text("wireless technology")
        .limit(10)
        .build();

    println!("  🔍 Searching across indices: {:?}", indices);
    println!("     Query: '{}'", query.text.as_ref().unwrap());

    // Mock multi-index results
    let mock_results = vec![
        ("products", "Wireless Bluetooth Headphones", "Electronics"),
        ("articles", "The Future of Wireless Technology", "Technology"),
        ("reviews", "Review of Latest Wireless Earbuds", "Reviews"),
        ("products", "Wireless Charging Pad", "Electronics"),
        ("articles", "How Wireless Networks Work", "Technology"),
    ];

    println!("  📋 Multi-index search results:");
    for (i, (index, title, category)) in mock_results.iter().enumerate() {
        println!("     {}. [{}] {} ({})", i + 1, index, title, category);
    }

    // Example of weighted multi-index search
    println!("  ⚖️ Weighted multi-index search:");
    println!("     - Products: weight 2.0 (higher priority)");
    println!("     - Articles: weight 1.5");
    println!("     - Reviews: weight 1.0");

    // Cross-index faceting
    println!("  📊 Cross-index faceting:");
    println!("     - Combined categories from all indices");
    println!("     - Unified relevance scoring");
    println!("     - Deduplicated results");

    Ok(())
}