//! Algolia setup and configuration examples
//!
//! This example demonstrates how to set up and configure Algolia
//! with various authentication methods, indices, and advanced settings.

use backbone_search::{AlgoliaSearch, SearchService, IndexConfig};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    println!("🔍 Algolia Setup Examples");
    println!("===========================");

    // Example 1: Basic Algolia setup
    println!("\n1. Basic Algolia Setup:");
    basic_setup_example().await?;

    // Example 2: Authentication and security
    println!("\n2. Authentication and Security:");
    authentication_examples().await?;

    // Example 3: Index configuration
    println!("\n3. Index Configuration:");
    index_configuration_example().await?;

    // Example 4: Rules and personalization
    println!("\n4. Rules and Personalization:");
    rules_and_personalization_example().await?;

    // Example 5: Analytics and A/B testing
    println!("\n5. Analytics and A/B Testing:");
    analytics_and_testing_example().await?;

    // Example 6: Performance optimization
    println!("\n6. Performance Optimization:");
    performance_optimization_example().await?;

    Ok(())
}

/// Basic Algolia setup example
async fn basic_setup_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("  Setting up basic Algolia connection...");

    // Method 1: Using App ID and API Key
    let app_id = "YOUR_ALGOLIA_APP_ID";
    let api_key = "YOUR_ALGOLIA_API_KEY";

    let algolia_search = AlgoliaSearch::new(app_id, api_key);
    println!("  ✅ Connected to Algolia with App ID: {}", app_id);

    // Method 2: Using different key types
    println!("  🔑 Using different API key types:");
    println!("     - Search-only key: For client-side searches");
    println!("     - Admin key: For full access (indexing, configuration)");
    println!("     - API key: For server-side operations");

    // Test the connection
    let is_healthy = algolia_search.test_connection().await?;
    println!("  🏥 Health check: {}", if is_healthy { "Connected" } else { "Disconnected" });

    // Get cluster info
    let stats = algolia_search.get_stats("default").await?;
    println!("  📊 Basic stats: {} total documents", stats.total_documents);

    Ok(())
}

/// Authentication and security examples
async fn authentication_examples() -> Result<(), Box<dyn std::error::Error>> {
    println!("  Configuring Algolia authentication and security...");

    // Example 1: Different key types
    println!("  🔐 API Key Types:");

    // Search-only key (client-side)
    println!("     1. Search-only Key:");
    println!("        - Purpose: Client-side search queries");
    println!("        - Permissions: search only");
    println!("        - Can be exposed in frontend code");
    println!("        - Example: search-only-api-key-12345");

    // Admin key (server-side)
    println!("     2. Admin Key:");
    println!("        - Purpose: Full server-side access");
    println!("        - Permissions: All operations");
    println!("        - Must be kept secret");
    println!("        - Example: admin-api-key-67890");

    // Monitoring key
    println!("     3. Monitoring Key:");
    println!("        - Purpose: Analytics and monitoring");
    println!("        - Permissions: Read-only analytics");
    println!("        - Usage rate limited");

    // Example 2: Secured API keys
    println!("  🔒 Secured API Keys:");
    println!("     - Add restrictions to API keys");
    println!("     - Filter by user attributes");
    println!("     - Rate limiting per user");
    println!("     - Geographic restrictions");
    println!("     - Referer restrictions");

    // Example of secured key generation
    let secured_key_example = json!({
        "filters": "user_id:12345 OR public:true",
        "validUntil": 1640995200, // Unix timestamp
        "restrictIndices": ["products", "articles"],
        "restrictSources": "192.168.1.0/24,10.0.0.0/8",
        "userToken": "user123",
        "rateLimit": 100
    });

    println!("     Secured key example: {}", secured_key_example);

    // Example 3: Multi-index keys
    println!("  🗂️ Multi-index API Keys:");
    println!("     - Create keys with specific index access");
    println!("     - Separate keys for different applications");
    println!("     - Minimize attack surface");

    Ok(())
}

/// Index configuration examples
async fn index_configuration_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("  Configuring Algolia indices...");

    // Example 1: Basic index settings
    println!("  ⚙️ Basic Index Settings:");
    let basic_settings = json!({
        "searchableAttributes": [
            "title",
            "description",
            "category",
            "brand"
        ],
        "attributesForFaceting": [
            "category",
            "brand",
            "price",
            "rating",
            "in_stock"
        ],
        "attributesToRetrieve": [
            "title",
            "description",
            "price",
            "rating",
            "image_url"
        ],
        "ranking": [
            "typo",
            "geo",
            "words",
            "filters",
            "proximity",
            "attribute",
            "exact",
            "custom"
        ],
        "customRanking": [
            "desc(rating)",
            "desc(created_at)"
        ],
        "unretrievableAttributes": [
            "internal_id",
            "admin_notes"
        ],
        "attributesToHighlight": [
            "title",
            "description"
        ]
    });

    println!("     - Searchable attributes: title, description, category, brand");
    println!("     - Faceting attributes: category, brand, price, rating, in_stock");
    println!("     - Custom ranking: by rating (desc), by creation date (desc)");

    // Example 2: Advanced configuration
    println!("  🔧 Advanced Index Configuration:");
    let advanced_settings = json!({
        "searchableAttributes": [
            "unordered(title)",
            "unordered(description)",
            "category",
            "brand",
            "tags"
        ],
        "attributesForFaceting": [
            "filterOnly(category)",
            "searchable(brand)",
            "price",
            "rating",
            "in_stock"
        ],
        "numericAttributesForFiltering": [
            "price",
            "rating",
            "stock_quantity"
        ],
        "slaves": [
            "products_price_asc",
            "products_price_desc",
            "products_rating_desc"
        ],
        "replicas": [
            "products_virtual",
            "products_novelty"
        ],
        "maxValuesPerFacet": 100,
        "sortFacetValuesBy": "alpha",
        "highlightPreTag": "<em>",
        "highlightPostTag": "</em>",
        "snippetEllipsisText": "…",
        "attributesToSnippet": [
            "description:50"
        ]
    });

    println!("     - Unordered search on title and description");
    println!("     - Filter-only and searchable facets");
    println!("     - Slave indices for different sort orders");
    println!("     - Virtual replicas for alternative rankings");

    // Example 3: Query rules configuration
    println!("  📋 Query Rules Configuration:");
    let query_rules = vec![
        json!({
            "objectID": "brand-promotion",
            "condition": {
                "pattern": "{facet:brand}",
                "anchoring": "is"
            },
            "consequence": {
                "params": {
                    "automaticFacetFilters": [
                        "brand:{facet:brand}"
                    ],
                    "query": {
                        "edits": [
                            {
                                "type": "remove",
                                "delete": "{facet:brand}"
                            }
                        ]
                    }
                }
            }
        }),
        json!({
            "objectID": "out-of-stock-handling",
            "condition": {
                "pattern": "*",
                "context": "out_of_stock"
            },
            "consequence": {
                "params": {
                    "filters": "in_stock:true",
                    "automaticFacetFilters": ["in_stock:true"]
                }
            }
        })
    ];

    println!("     - Brand promotion rules");
    println!("     - Out of stock handling");
    println!("     - Query expansion and correction");
    println!("     - Dynamic result boosting");

    Ok(())
}

/// Rules and personalization examples
async fn rules_and_personalization_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("  Setting up rules and personalization...");

    // Example 1: Query rules
    println!("  📋 Query Rules Examples:");

    // Rule 1: Brand query handling
    let brand_rule = json!({
        "objectID": "brand-query-rule",
        "condition": {
            "pattern": "{facet:brand}",
            "anchoring": "is"
        },
        "consequence": {
            "params": {
                "automaticFacetFilters": ["brand:{facet:brand}"],
                "query": {
                    "remove": ["{facet:brand}"]
                }
            },
            "hide": ["sponsored_results"]
        }
    });

    println!("     1. Brand Query Rule:");
    println!("        - Pattern: exact match on brand facet");
    println!("        - Action: filter by brand and remove from query");
    println!("        - Hide sponsored results for brand queries");

    // Rule 2: Geographic personalization
    let geo_rule = json!({
        "objectID": "geo-personalization",
        "condition": {
            "context": "geo_search"
        },
        "consequence": {
            "params": {
                "aroundLatLngViaIP": true,
                "automaticFacetFilters": ["available_in_region:true"]
            }
        }
    });

    println!("     2. Geographic Personalization:");
    println!("        - Context: geo-based searches");
    println!("        - Action: use IP-based location");
    println!("        - Filter: show items available in user's region");

    // Rule 3: Promotional campaigns
    let promo_rule = json!({
        "objectID": "summer-promo",
        "condition": {
            "pattern": "summer sale",
            "anchoring": "contains"
        },
        "consequence": {
            "promote": [
                {"objectID": "summer-product-1", "position": 1},
                {"objectID": "summer-product-2", "position": 2}
            ],
            "userData": {
                "campaign": "summer_sale_2024",
                "boost": 2.0
            }
        }
    });

    println!("     3. Promotional Campaign Rule:");
    println!("        - Pattern: contains 'summer sale'");
    println!("        - Action: promote specific products to top positions");

    // Example 2: Personalization strategy
    println!("  👤 Personalization Strategy:");
    println!("     1. User Behavior Tracking:");
    println!("        - Click-through rates");
    println!("        - Purchase history");
    println!("        - Search patterns");
    println!("        - Time-based preferences");

    println!("     2. Personalization Events:");
    println!("        - View events");
    println!("        - Click events");
    println!("        - Conversion events");
    println!("        - Add to cart events");

    println!("     3. User Segments:");
    println!("        - New users vs returning users");
    println!("        - High-value customers");
    println!("        - Category preferences");
    println!("        - Geographic segments");

    // Example 3: A/B testing setup
    println!("  🧪 A/B Testing Configuration:");
    let ab_test = json!({
        "name": "Product Ranking A/B Test",
        "description": "Test custom ranking vs default ranking",
        "trafficPercentage": 50,
        "endAt": "2024-12-31T23:59:59Z",
        "variants": [
            {
                "indexName": "products_variant_a",
                "trafficPercentage": 50,
                "description": "Default ranking"
            },
            {
                "indexName": "products_variant_b",
                "trafficPercentage": 50,
                "description": "Custom ranking with rating boost"
            }
        ]
    });

    println!("     - 50/50 traffic split");
    println!("     - Test duration: Until end of year");
    println!("     - Metric: Conversion rate optimization");

    Ok(())
}

/// Analytics and A/B testing examples
async fn analytics_and_testing_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("  Setting up analytics and A/B testing...");

    // Example 1: Analytics configuration
    println!("  📊 Analytics Configuration:");
    println!("     1. Search Analytics:");
    println!("        - Top queries and click-through rates");
    println!("        - No result queries");
    println!("        - Filters usage analytics");
    println!("        - Geographic search patterns");

    println!("     2. Performance Metrics:");
    println!("        - Average query time");
    println!("        - 95th percentile response time");
    println!("        - Error rates and types");
    println!("        - API usage statistics");

    println!("     3. Business Intelligence:");
    println!("        - Revenue attribution");
    println!("        - User journey analysis");
    println!("        - Conversion funnel");
    println!("        - Search ROI calculation");

    // Example 2: Custom event tracking
    println!("  🎯 Custom Event Tracking:");
    let custom_events = vec![
        json!({
            "eventType": "conversion",
            "eventName": "product_purchase",
            "objectData": {
                "objectID": "product_123",
                "queryID": "query_456",
                "price": 299.99,
                "discount": 10.0
            }
        }),
        json!({
            "eventType": "click",
            "eventName": "product_view",
            "objectData": {
                "objectID": "product_456",
                "queryID": "query_789",
                "position": 3
            }
        })
    ];

    println!("     - Purchase conversion events");
    println!("     - Product view events with position tracking");
    println!("     - Cart addition events");
    println!("     - Wishlist addition events");

    // Example 3: Real-time insights
    println!("  ⚡ Real-time Insights:");
    println!("     1. Search Performance Dashboard:");
    println!("        - Live query volume");
    println!("        - Real-time CTR monitoring");
    println!("        - Error rate alerts");
    println!("        - Performance degradation detection");

    println!("     2. Business Impact Metrics:");
    println!("        - Revenue per search");
    println!("        - Search-driven conversions");
    println!("        - User engagement metrics");
    println!("        - Search abandonment rates");

    Ok(())
}

/// Performance optimization examples
async fn performance_optimization_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("  Optimizing Algolia performance...");

    // Example 1: Query optimization
    println!("  🔍 Query Optimization:");
    println!("     1. Index Structure:");
    println!("        - Optimize searchable attributes order");
    println!("        - Use attributesToRetrieve for bandwidth savings");
    println!("        - Implement proper faceting strategy");
    println!("        - Use attributesToHighlight selectively");

    println!("     2. Query Design:");
    println!("        - Use filters instead of complex queries when possible");
    println!("        - Implement proper typo tolerance settings");
    println!("        - Use geo-filtering efficiently");
    println!("        - Implement query caching strategies");

    // Example 2: Index optimization
    println!("  📚 Index Optimization:");
    println!("     1. Data Structure:");
    println!("        - Keep record size under 10KB");
    println!("        - Use nested objects carefully");
    println!("        - Implement proper data types");
    println!("        - Avoid unnecessary attributes");

    println!("     2. Ranking Strategy:");
    println!("        - Optimize custom ranking attributes");
    println!("        - Use business metrics in ranking");
    println!("        - Implement seasonal ranking adjustments");
    println!("        - Test ranking effectiveness");

    // Example 3: Caching strategies
    println!("  💾 Caching Strategies:");
    println!("     1. Client-side Caching:");
    println!("        - Cache popular search results");
    println!("        - Implement TTL strategies");
    println!("        - Use service workers for PWA caching");
    println!("        - Cache facet values");

    println!("     2. Server-side Caching:");
    println!("        - Cache API responses");
    println!("        - Implement CDN for static assets");
    println!("        - Use Redis for session caching");
    println!("        - Cache user-specific results");

    // Example 4: Monitoring and alerts
    println!("  📈 Monitoring and Alerts:");
    println!("     1. Performance Monitoring:");
    println!("        - Monitor response times");
    println!("        - Track error rates");
    println!("        - Set up performance budgets");
    println!("        - Implement alerting thresholds");

    println!("     2. Usage Analytics:");
    println!("        - Track query patterns");
    println!("        - Monitor API usage quotas");
    println!("        - Analyze user behavior");
    println!("        - Identify performance bottlenecks");

    // Example 5: Cost optimization
    println!("  💰 Cost Optimization:");
    println!("     1. Query Efficiency:");
    println!("        - Reduce unnecessary API calls");
    println!("        - Implement query deduplication");
    println!("        - Use batch operations for updates");
    println!("        - Optimize record sizes");

    println!("     2. Infrastructure:");
    println!("        - Choose appropriate plan tiers");
    println!("        - Implement efficient data sync");
    println!("        - Use webhooks for real-time updates");
    println!("        - Optimize data transfer");

    Ok(())
}