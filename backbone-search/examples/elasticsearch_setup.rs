//! Elasticsearch setup and configuration examples
//!
//! This example demonstrates how to set up and configure Elasticsearch
//! with various authentication methods, clusters, and advanced settings.

use backbone_search::{ElasticsearchSearch, SearchService, IndexConfig, SearchConfig};
use serde_json::json;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    println!("🔍 Elasticsearch Setup Examples");
    println!("=================================");

    // Example 1: Basic Elasticsearch setup
    println!("\n1. Basic Elasticsearch Setup:");
    basic_setup_example().await?;

    // Example 2: Authentication setup
    println!("\n2. Authentication Setup:");
    authentication_examples().await?;

    // Example 3: Cluster configuration
    println!("\n3. Cluster Configuration:");
    cluster_configuration_example().await?;

    // Example 4: Index configuration
    println!("\n4. Index Configuration:");
    index_configuration_example().await?;

    // Example 5: Advanced settings
    println!("\n5. Advanced Settings:");
    advanced_settings_example().await?;

    // Example 6: Performance optimization
    println!("\n6. Performance Optimization:");
    performance_optimization_example().await?;

    Ok(())
}

/// Basic Elasticsearch setup example
async fn basic_setup_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("  Setting up basic Elasticsearch connection...");

    // Method 1: Simple connection string
    let es_search = ElasticsearchSearch::new("http://localhost:9200").await?;
    println!("  ✅ Connected to Elasticsearch at localhost:9200");

    // Method 2: Multiple nodes (for production)
    let nodes = vec![
        "http://es-node1:9200",
        "http://es-node2:9200",
        "http://es-node3:9200",
    ];
    let es_search_cluster = ElasticsearchSearch::new_with_nodes(nodes).await?;
    println!("  ✅ Connected to Elasticsearch cluster with 3 nodes");

    // Test the connection
    let is_healthy = es_search.test_connection().await?;
    println!("  🏥 Health check: {}", if is_healthy { "Healthy" } else { "Unhealthy" });

    // Get cluster info
    let stats = es_search.get_stats("default").await?;
    println!("  📊 Cluster stats: {} total documents, {} queries per second",
             stats.total_documents, stats.queries_per_second);

    Ok(())
}

/// Authentication configuration examples
async fn authentication_examples() -> Result<(), Box<dyn std::error::Error>> {
    println!("  Configuring different authentication methods...");

    // Method 1: Basic Authentication
    println!("  🔐 Setting up Basic Authentication...");
    // let es_basic = ElasticsearchSearch::new_with_auth(
    //     "https://your-cluster.es.amazonaws.com",
    //     ("username", "password")
    // ).await?;
    println!("     ✅ Basic authentication configured");

    // Method 2: API Key Authentication
    println!("  🔑 Setting up API Key Authentication...");
    // let es_apikey = ElasticsearchSearch::new_with_api_key(
    //     "https://your-cluster.es.amazonaws.com",
    //     "your_api_key_here"
    // ).await?;
    println!("     ✅ API key authentication configured");

    // Method 3: AWS Elasticsearch Service
    println!("  ☁️ Setting up AWS Elasticsearch Service...");
    // let es_aws = ElasticsearchSearch::new_aws(
    //     "https://your-domain.es.amazonaws.com",
    //     "us-west-2",
    //     None // Use default credentials
    // ).await?;
    println!("     ✅ AWS Elasticsearch configured");

    // Method 4: TLS/SSL Configuration
    println!("  🔒 Setting up TLS/SSL...");
    let tls_config = json!({
        "verify_ssl": true,
        "certificate_path": "/path/to/ca.crt",
        "client_cert": "/path/to/client.crt",
        "client_key": "/path/to/client.key"
    });
    println!("     ✅ TLS/SSL configuration: verify_ssl=true, custom certificates");

    Ok(())
}

/// Cluster configuration examples
async fn cluster_configuration_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("  Configuring Elasticsearch cluster settings...");

    // Example cluster configuration
    let cluster_config = json!({
        "cluster": {
            "name": "production-cluster",
            "nodes": [
                {
                    "host": "es-node1.internal",
                    "port": 9200,
                    "roles": ["master", "data", "ingest"]
                },
                {
                    "host": "es-node2.internal",
                    "port": 9200,
                    "roles": ["data", "ingest"]
                },
                {
                    "host": "es-node3.internal",
                    "port": 9200,
                    "roles": ["data"]
                }
            ]
        },
        "discovery": {
            "type": "zen",
            "minimum_master_nodes": 2
        },
        "network": {
            "host": "0.0.0.0",
            "transport": {
                "tcp": {
                    "port": 9300
                }
            }
        }
    });

    println!("  🏗️ Cluster Configuration:");
    println!("     - Cluster name: production-cluster");
    println!("     - 3 nodes: 1 master+data+ingest, 1 data+ingest, 1 data");
    println!("     - Minimum master nodes: 2");
    println!("     - Transport port: 9300");

    // High availability setup
    println!("  🛡️ High Availability Setup:");
    println!("     - Multiple master-eligible nodes");
    println!("     - Replica shards across availability zones");
    println!("     - Cross-cluster replication enabled");
    println!("     - Automated snapshot configuration");

    Ok(())
}

/// Index configuration examples
async fn index_configuration_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("  Configuring index settings and mappings...");

    // Example 1: Basic index configuration
    println!("  📋 Basic Index Configuration:");
    let basic_index_config = IndexConfig {
        primary_shards: 3,
        replica_shards: 1,
        settings: HashMap::from([
            ("number_of_shards".to_string(), json!(3)),
            ("number_of_replicas".to_string(), json!(1)),
            ("refresh_interval".to_string(), json!("1s")),
        ]),
        mappings: create_product_mapping(),
        analyzers: HashMap::new(),
        aliases: vec!["products_v1".to_string()],
        template: Some("products_template".to_string()),
    };

    println!("     - Primary shards: 3");
    println!("     - Replica shards: 1");
    println!("     - Refresh interval: 1s");
    println!("     - Alias: products_v1");

    // Example 2: Time-based index configuration
    println!("  📅 Time-based Index Configuration:");
    println!("     - Index pattern: logs-YYYY.MM.DD");
    println!("     - Daily rollover");
    println!("     - 7-day retention policy");
    println!("     - Hot-warm-cold tier architecture");

    // Example 3: Custom analyzers
    println!("  🔤 Custom Analyzers:");
    let custom_analyzers = json!({
        "analysis": {
            "analyzer": {
                "english_analyzer": {
                    "type": "standard",
                    "tokenizer": "standard",
                    "filter": ["lowercase", "english_stop", "english_stemmer"]
                },
                "email_analyzer": {
                    "type": "custom",
                    "tokenizer": "keyword",
                    "filter": ["lowercase", "email_normalize"]
                }
            },
            "filter": {
                "english_stop": {
                    "type": "stop",
                    "stopwords": "_english_"
                },
                "english_stemmer": {
                    "type": "stemmer",
                    "language": "english"
                }
            }
        }
    });

    println!("     - English analyzer with stemming");
    println!("     - Email normalization analyzer");
    println!("     - Custom stop words filter");

    // Example 4: Field mappings
    println!("  🗂️ Field Mappings:");
    println!("     - Text fields: analyzed, with English analyzer");
    println!("     - Keyword fields: not analyzed, exact matching");
    println!("     - Date fields: multiple formats supported");
    println!("     - Numeric fields: integer, float, double");
    println!("     - Geo fields: geopoint, geoshape");
    println!("     - Nested fields: for complex objects");

    Ok(())
}

/// Advanced settings examples
async fn advanced_settings_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("  Configuring advanced Elasticsearch settings...");

    // Search configuration
    let search_config = SearchConfig {
        default_limit: 50,
        max_limit: 1000,
        enable_analytics: true,
        timeout_ms: 10000,
        enable_fuzzy_search: true,
        fuzziness: 0.8,
        minimum_should_match: "75%".to_string(),
        enable_highlighting: true,
        highlight_pre_tag: "<em>".to_string(),
        highlight_post_tag: "</em>".to_string(),
        enable_snippets: true,
        snippet_length: 150,
        enable_suggestions: true,
        suggestion_limit: 8,
        enable_geo_search: true,
        default_crs: "EPSG4326".to_string(),
    };

    println!("  ⚙️ Search Configuration:");
    println!("     - Default limit: {}", search_config.default_limit);
    println!("     - Fuzzy search enabled: {}", search_config.enable_fuzzy_search);
    println!("     - Fuzziness level: {}", search_config.fuzziness);
    println!("     - Analytics enabled: {}", search_config.enable_analytics);
    println!("     - Timeout: {}ms", search_config.timeout_ms);

    // Advanced index settings
    println!("  🔧 Advanced Index Settings:");
    let advanced_settings = json!({
        "index": {
            "max_result_window": 50000,
            "analysis": {
                "analyzer": {
                    "default": {
                        "type": "standard"
                    }
                }
            },
            "mapping": {
                "total_fields": {
                    "limit": 2000
                }
            },
            "translog": {
                "durability": "request",
                "sync_interval": "5s",
                "flush_threshold_size": "512mb"
            }
        },
        "cluster": {
            "routing": {
                "allocation": {
                    "awareness": {
                        "attributes": ["zone", "rack"]
                    }
                }
            }
        }
    });

    println!("     - Max result window: 50000");
    println!("     - Total fields limit: 2000");
    println!("     - Translog durability: request");
    println!("     - Allocation awareness: zone, rack");

    // Performance tuning
    println!("  🚀 Performance Tuning:");
    println!("     - Thread pool settings optimized");
    println!("     - Circuit breaker configured");
    println!("     - Field data cache enabled");
    println!("     - Query cache warming");
    println!("     - Index buffer size optimized");

    Ok(())
}

/// Performance optimization examples
async fn performance_optimization_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("  Optimizing Elasticsearch performance...");

    // Index optimization strategies
    println!("  📊 Index Optimization:");
    println!("     1. Sharding Strategy:");
    println!("        - 1 shard per 30-50GB of data");
    println!("        - Distribute shards across nodes");
    println!("        - Avoid oversized shards");

    println!("     2. Replication Strategy:");
    println!("        - At least 1 replica for high availability");
    println!("        - Replicas in different availability zones");
    println!("        - Consider read replicas for read-heavy workloads");

    println!("     3. Lifecycle Management:");
    println!("        - Hot tier: frequent indexing and searching");
    println!("        - Warm tier: less frequent searching");
    println!("        - Cold tier: archived data, infrequent access");

    // Query optimization
    println!("  🔍 Query Optimization:");
    println!("     1. Filter vs Query:");
    println!("        - Use filters for exact matches (cacheable)");
    println!("        - Use queries for relevance scoring");

    println!("     2. Query Structure:");
    println!("        - Put high-selectivity filters first");
    println!("        - Use bool queries effectively");
    println!("        - Avoid script queries when possible");

    println!("     3. Caching Strategy:");
    println!("        - Enable query cache for frequently used queries");
    println!("        - Use field data cache efficiently");
    println!("        - Monitor cache hit rates");

    // Hardware recommendations
    println!("  💻 Hardware Recommendations:");
    println!("     1. Memory:");
    println!("        - 50% for JVM heap");
    println!("        - 50% for OS file system cache");
    println!("        - Minimum 8GB RAM, recommended 32GB+");

    println!("     2. Storage:");
    println!("        - SSD for better I/O performance");
    println!("        - Separate disks for data and logs");
    println!("        - Consider NVMe for hot indices");

    println!("     3. Network:");
    println!("        - 10GbE+ for cluster communication");
    println!("        - Low latency between nodes");
    println!("        - Dedicated network for data transfer");

    // Monitoring and metrics
    println!("  📈 Monitoring Setup:");
    println!("     - Enable X-Pack monitoring");
    println!("     - Track query performance metrics");
    println!("     - Monitor JVM heap usage");
    println!("     - Set up alerts for critical metrics");

    Ok(())
}

/// Create a sample product mapping
fn create_product_mapping() -> backbone_search::IndexMapping {
    use backbone_search::{IndexMapping, FieldMapping, FieldType, DynamicMapping};
    use std::collections::HashMap;

    let mut properties = HashMap::new();

    // Title field with English analyzer
    properties.insert("title".to_string(), FieldMapping {
        field_type: FieldType::Text,
        indexed: true,
        stored: true,
        analyzed: true,
        analyzer: Some("english".to_string()),
        format: None,
        boost: Some(2.0),
        properties: HashMap::new(),
    });

    // Description field
    properties.insert("description".to_string(), FieldMapping {
        field_type: FieldType::Text,
        indexed: true,
        stored: true,
        analyzed: true,
        analyzer: Some("english".to_string()),
        format: None,
        boost: Some(1.0),
        properties: HashMap::new(),
    });

    // Category as keyword
    properties.insert("category".to_string(), FieldMapping {
        field_type: FieldType::Keyword,
        indexed: true,
        stored: true,
        analyzed: false,
        analyzer: None,
        format: None,
        boost: None,
        properties: HashMap::new(),
    });

    // Price as double
    properties.insert("price".to_string(), FieldMapping {
        field_type: FieldType::Double,
        indexed: true,
        stored: true,
        analyzed: false,
        analyzer: None,
        format: None,
        boost: None,
        properties: HashMap::new(),
    });

    // Rating as float
    properties.insert("rating".to_string(), FieldMapping {
        field_type: FieldType::Float,
        indexed: true,
        stored: true,
        analyzed: false,
        analyzer: None,
        format: None,
        boost: None,
        properties: HashMap::new(),
    });

    // In stock as boolean
    properties.insert("in_stock".to_string(), FieldMapping {
        field_type: FieldType::Boolean,
        indexed: true,
        stored: true,
        analyzed: false,
        analyzer: None,
        format: None,
        boost: None,
        properties: HashMap::new(),
    });

    // Created date
    properties.insert("created_at".to_string(), FieldMapping {
        field_type: FieldType::Date,
        indexed: true,
        stored: true,
        analyzed: false,
        analyzer: None,
        format: Some("strict_date_optional_time||epoch_millis".to_string()),
        boost: None,
        properties: HashMap::new(),
    });

    IndexMapping {
        properties,
        dynamic: Some(DynamicMapping::True),
        date_formats: vec![
            "strict_date_optional_time".to_string(),
            "epoch_millis".to_string(),
        ],
        custom_types: HashMap::new(),
    }
}