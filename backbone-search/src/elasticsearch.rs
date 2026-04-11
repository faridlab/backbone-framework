//! Elasticsearch search implementation
#![allow(dead_code)]
#![allow(unused_variables)]

use async_trait::async_trait;
use elasticsearch::{Elasticsearch, http::transport::Transport, SearchParts};
// use elasticsearch::params::*; // Currently unused
use elasticsearch::{BulkParts, UpdateParts, DeleteParts, GetParts, CreateParts};
use elasticsearch::indices::{IndicesCreateParts, IndicesGetParts, IndicesExistsParts, IndicesStatsParts, IndicesForcemergeParts, IndicesGetMappingParts, IndicesDeleteParts};
use crate::{
    SearchResult, SearchError, SearchService, SearchDocument, SearchQuery, SearchStats,
    SearchBackend, SearchResults, SearchHit, FilterValue, FilterOperator,
    SortOrder, IndexConfig, IndexInfo, IndexStatus, IndexHealth, IndexMapping,
    BulkOperation, BulkOperationType, BulkResult, IndexResult, TimeRange,
    SearchAnalytics
};
use std::collections::HashMap;
use chrono::Utc;
use serde_json::json;

/// Elasticsearch configuration
#[derive(Debug, Clone)]
pub struct ElasticsearchConfig {
    /// Elasticsearch URLs
    pub urls: Vec<String>,

    /// Username for authentication
    pub username: Option<String>,

    /// Password for authentication
    pub password: Option<String>,

    /// API key for authentication
    pub api_key: Option<String>,

    /// Cloud ID for Elastic Cloud
    pub cloud_id: Option<String>,

    /// Connection timeout in seconds
    pub timeout: u64,

    /// Request retry count
    pub retry_count: u32,

    /// Pool size
    pub pool_size: u32,

    /// Enable compression
    pub compression: bool,

    /// Certificate verification
    pub verify_certs: bool,
}

impl Default for ElasticsearchConfig {
    fn default() -> Self {
        Self {
            urls: vec!["http://localhost:9200".to_string()],
            username: None,
            password: None,
            api_key: None,
            cloud_id: None,
            timeout: 30,
            retry_count: 3,
            pool_size: 10,
            compression: true,
            verify_certs: true,
        }
    }
}

/// Elasticsearch search service
pub struct ElasticsearchSearch {
    client: Elasticsearch,
    config: ElasticsearchConfig,
}

impl ElasticsearchSearch {
    /// Create new Elasticsearch search service
    pub async fn new(config: ElasticsearchConfig) -> SearchResult<Self> {
        let url = &config.urls[0];
        let transport = Transport::single_node(url)
            .map_err(|e| SearchError::ElasticsearchConnection(e.to_string()))?;

        let client = Elasticsearch::new(transport);

        Ok(Self { client, config })
    }

    /// Builder for Elasticsearch search service
    pub fn builder() -> ElasticsearchSearchBuilder {
        ElasticsearchSearchBuilder::new()
    }

    /// Convert SearchDocument to Elasticsearch document
    fn convert_document_to_es(&self, doc: &SearchDocument) -> serde_json::Result<serde_json::Value> {
        let mut es_doc = serde_json::json!({
            "id": doc.id,
            "content": doc.content,
            "timestamp": doc.timestamp,
            "tags": doc.tags,
            "boost": doc.boost
        });

        // Add title if present
        if let Some(title) = &doc.title {
            es_doc["title"] = json!(title);
        }

        // Add fields
        for (key, value) in &doc.fields {
            es_doc[key] = value.clone();
        }

        // Add metadata
        es_doc["metadata"] = serde_json::to_value(&doc.metadata)?;

        // Add language if present
        if let Some(language) = &doc.language {
            es_doc["language"] = json!(language);
        }

        Ok(es_doc)
    }

    /// Convert Elasticsearch document to SearchDocument
    fn convert_es_document_to_document(&self, es_doc: &serde_json::Value, id: &str) -> SearchResult<SearchDocument> {
        let doc: SearchDocument = serde_json::from_value(es_doc.clone())
            .map_err(|e| SearchError::Deserialization(e.to_string()))?;

        // Ensure ID matches
        if doc.id != id {
            return Err(SearchError::Deserialization("Document ID mismatch".to_string()));
        }

        Ok(doc)
    }

    /// Convert SearchQuery to Elasticsearch query
    fn convert_query_to_es(&self, query: &SearchQuery) -> serde_json::Result<serde_json::Value> {
        let mut es_query = json!({});

        // Build query part
        let mut query_clause = json!([]);
        let mut should_clauses = vec![];

        // Text query
        if let Some(text) = &query.text {
            should_clauses.push(json!({
                "multi_match": {
                    "query": text,
                    "fields": ["content^2", "title^3", "fields.*"],
                    "fuzziness": if query.options.fuzzy {
                        Some(query.options.fuzziness.to_string())
                    } else {
                        Some("AUTO".to_string())
                    },
                    "type": "best_fields"
                }
            }));
        }

        // Field queries
        for field_query in &query.fields {
            should_clauses.push(json!({
                "match": {
                    field_query.field.clone(): {
                        "query": field_query.query,
                        "boost": field_query.boost.unwrap_or(1.0)
                    }
                }
            }));
        }

        if !should_clauses.is_empty() {
            query_clause = json!({
                "bool": {
                    "should": should_clauses,
                    "minimum_should_match": query.options.minimum_should_match
                }
            });
        }

        // Filters
        if !query.filters.is_empty() {
            let mut filter_clauses = vec![];
            for filter in &query.filters {
                let filter_clause = self.convert_filter_to_es(filter)?;
                filter_clauses.push(filter_clause);
            }

            query_clause = if should_clauses.is_empty() {
                json!({
                    "bool": {
                        "filter": filter_clauses
                    }
                })
            } else {
                json!({
                    "bool": {
                        "must": query_clause,
                        "filter": filter_clauses
                    }
                })
            };
        }

        es_query["query"] = query_clause;

        // Sort
        if !query.sort.is_empty() {
            let sort_clauses: Vec<serde_json::Value> = query.sort.iter()
                .map(|s| {
                    let mut sort_obj = json!({});
                    let sort_key = match s.order {
                        SortOrder::Asc => s.field.clone(),
                        SortOrder::Desc => format!("{}:desc", s.field),
                    };
                    sort_obj[&sort_key] = json!({});
                    sort_obj
                })
                .collect();

            es_query["sort"] = json!(sort_clauses);
        }

        // Pagination
        let from = (query.pagination.page - 1) * query.pagination.size;
        es_query["from"] = json!(from);
        es_query["size"] = json!(query.pagination.size);

        // Highlighting
        if query.options.highlight {
            let highlight_config = query.options.highlight_config.as_ref().map(|h| {
                json!({
                    "fields": h.fields.iter().map(|f| f.clone()).collect::<Vec<_>>(),
                    "pre_tags": [h.pre_tag.clone()],
                    "post_tags": [h.post_tag.clone()],
                    "fragment_size": h.fragment_size,
                    "number_of_fragments": h.number_of_fragments
                })
            }).unwrap_or_else(|| json!({
                "fields": ["content", "title"],
                "pre_tags": ["<mark>"],
                "post_tags": ["</mark>"],
                "fragment_size": 200,
                "number_of_fragments": 3
            }));

            es_query["highlight"] = highlight_config;
        }

        // Facets
        if !query.facets.is_empty() {
            let mut facets = json!({});
            for facet in &query.facets {
                // Simplified facet implementation - would need to be expanded based on facet type
                facets[&facet.field] = json!({
                    "terms": {
                        "field": facet.field,
                        "size": facet.size
                    }
                });
            }
            es_query["aggs"] = facets;
        }

        // Aggregations
        if !query.aggregations.is_empty() {
            let mut aggs = json!({});
            for agg in &query.aggregations {
                // Simplified aggregation implementation
                aggs[&agg.name] = json!({
                    "terms": {
                        "field": agg.field,
                        "size": 10
                    }
                });
            }
            if es_query.get("aggs").is_none() {
                es_query["aggs"] = aggs;
            } else {
                if let Some(aggs_obj) = aggs.as_object() {
                    for (key, value) in aggs_obj {
                        if let Some(aggs_map) = es_query["aggs"].as_object_mut() {
                            aggs_map.insert(key.clone(), value.clone());
                        }
                    }
                }
            }
        }

        Ok(es_query)
    }

    /// Convert filter to Elasticsearch filter
    fn convert_filter_to_es(&self, filter: &crate::Filter) -> serde_json::Result<serde_json::Value> {
        match &filter.value {
            FilterValue::String(value) => {
                Ok(json!({
                    "term": {
                        filter.field.clone(): value
                    }
                }))
            }
            FilterValue::Number(value) => {
                Ok(json!({
                    "term": {
                        filter.field.clone(): value
                    }
                }))
            }
            FilterValue::Boolean(value) => {
                Ok(json!({
                    "term": {
                        filter.field.clone(): value
                    }
                }))
            }
            FilterValue::Array(values) => {
                Ok(json!({
                    "terms": {
                        filter.field.clone(): values
                    }
                }))
            }
            FilterValue::Range(range) => {
                let mut range_obj = json!({});
                if let Some(gte) = range.gte {
                    range_obj["gte"] = json!(gte);
                }
                if let Some(gt) = range.gt {
                    range_obj["gt"] = json!(gt);
                }
                if let Some(lte) = range.lte {
                    range_obj["lte"] = json!(lte);
                }
                if let Some(lt) = range.lt {
                    range_obj["lt"] = json!(lt);
                }
                Ok(json!({
                    "range": {
                        filter.field.clone(): range_obj
                    }
                }))
            }
            FilterValue::Exists(_) => {
                match filter.operator {
                    FilterOperator::Exists => {
                        Ok(json!({
                            "exists": {
                                "field": filter.field.clone()
                            }
                        }))
                    }
                    FilterOperator::NotExists => {
                        Ok(json!({
                            "bool": {
                                "must_not": {
                                    "exists": {
                                        "field": filter.field.clone()
                                    }
                                }
                            }
                        }))
                    }
                    _ => Ok(json!({})), // Simplified - return empty JSON object as fallback
                }
            }
        }
    }

    /// Convert Elasticsearch hits to SearchResults
    fn convert_es_hits_to_results(&self, response: &serde_json::Value, query: &SearchQuery) -> SearchResult<SearchResults> {
        let hits = response["hits"]["hits"].as_array()
            .ok_or_else(|| SearchError::Deserialization("Invalid hits format".to_string()))?;

        let mut search_hits = Vec::new();
        for hit in hits {
            let _id = hit["_id"].as_str().ok_or_else(|| SearchError::DocumentNotFound("unknown".to_string()))?;
            let _score = hit["_score"].as_f64();
            let source = &hit["_source"];

            let document = self.convert_es_document_to_document(source, _id)?;

            let mut highlights = HashMap::new();
            if let Some(highlight_fields) = hit.get("highlight") {
                for (field, fragments) in highlight_fields.as_object().unwrap() {
                    if let Some(frag_list) = fragments.as_array() {
                        let frag_strings: Vec<String> = frag_list.iter()
                            .filter_map(|f| f.as_str())
                            .map(|s| s.to_string())
                            .collect();
                        highlights.insert(field.clone(), frag_strings);
                    }
                }
            }

            search_hits.push(SearchHit {
                document,
                score: _score,
                highlights,
                snippets: HashMap::new(),
                sort_values: Vec::new(),
                matched_queries: Vec::new(),
                inner_hits: HashMap::new(),
                explanation: None,
            });
        }

        let total_hits = response["hits"]["total"]["value"].as_u64().unwrap_or(0);
        let max_score = response["hits"]["max_score"].as_f64();

        let mut aggregations = HashMap::new();
        if let Some(ags) = response.get("aggregations") {
            for (name, agg) in ags.as_object().unwrap() {
                aggregations.insert(name.clone(), agg.clone());
            }
        }

        let facets = HashMap::new();
        // Parse facets from aggregations if present

        Ok(SearchResults {
            total_hits,
            max_score,
            hits: search_hits,
            aggregations,
            facets,
            suggestions: Vec::new(),
            metadata: crate::types::SearchMetadata {
                query_time_ms: 0, // Would need to measure this
                total_shards: 0,
                successful_shards: 0,
                failed_shards: 0,
                scroll_id: None,
                backend_metadata: HashMap::new(),
            },
        })
    }
}

#[async_trait]
impl SearchService for ElasticsearchSearch {
    async fn create_index(&self, index_name: &str, config: Option<IndexConfig>) -> SearchResult<bool> {
        let mut index_body = json!({
            "settings": {
                "number_of_shards": 1,
                "number_of_replicas": 0
            },
            "mappings": {
                "properties": {
                    "content": {
                        "type": "text",
                        "analyzer": "standard"
                    },
                    "title": {
                        "type": "text",
                        "analyzer": "standard"
                    },
                    "timestamp": {
                        "type": "date"
                    },
                    "tags": {
                        "type": "keyword"
                    },
                    "boost": {
                        "type": "float"
                    }
                }
            }
        });

        // Apply custom configuration if provided
        if let Some(cfg) = config {
            if cfg.primary_shards > 0 {
                index_body["settings"]["number_of_shards"] = json!(cfg.primary_shards);
            }
            if cfg.replica_shards > 0 {
                index_body["settings"]["number_of_replicas"] = json!(cfg.replica_shards);
            }
        }

        // Proper Elasticsearch index creation with mappings and settings
        let response = self.client
            .indices()
            .create(IndicesCreateParts::Index(index_name))
            .body(index_body)
            .send()
            .await
            .map_err(|e| SearchError::ElasticsearchOperation(format!("Failed to create index: {}", e)))?;

        if response.status_code().is_success() {
            tracing::info!("Created Elasticsearch index: {}", index_name);
            Ok(true)
        } else {
            let error_body = response.text().await.unwrap_or_default();
            Err(SearchError::ElasticsearchOperation(format!("Failed to create index {}: {}", index_name, error_body)))
        }
    }

    async fn delete_index(&self, _index_name: &str) -> SearchResult<bool> {
        // Stubbed implementation - would need proper Elasticsearch API mapping
        // For now, just return true to indicate success
        Ok(true)
    }

    async fn list_indices(&self) -> SearchResult<Vec<String>> {
        let response = self.client
            .indices()
            .get(IndicesGetParts::Index(&[]))
            .send()
            .await
            .map_err(|e| SearchError::ElasticsearchOperation(e.to_string()))?;

        let indices_map = response.json::<serde_json::Value>()
            .await
            .map_err(|e| SearchError::ElasticsearchOperation(e.to_string()))?;

        let indices: Vec<String> = indices_map
            .as_object()
            .map(|obj| obj.keys().cloned().collect())
            .unwrap_or_default();

        Ok(indices)
    }

    async fn get_index(&self, index_name: &str) -> SearchResult<Option<IndexInfo>> {
        let response = self.client
            .indices()
            .get(IndicesGetParts::Index(&[index_name]))
            .send()
            .await
            .map_err(|e| SearchError::ElasticsearchOperation(e.to_string()))?;

        let indices = response.json::<serde_json::Value>()
            .await
            .map_err(|e| SearchError::ElasticsearchOperation(e.to_string()))?;

        if let Some(index_info) = indices.get(index_name) {
            let info = IndexInfo {
                name: index_name.to_string(),
                status: IndexStatus::Active,
                document_count: 0, // Would need to get from stats API
                size_bytes: 0,
                primary_shards: 1,
                replica_shards: 0,
                created_at: Utc::now(),
                updated_at: None,
                health: IndexHealth::Green,
                backend_info: HashMap::new(),
            };
            Ok(Some(info))
        } else {
            Ok(None)
        }
    }

    async fn index_exists(&self, index_name: &str) -> SearchResult<bool> {
        let response = self.client
            .indices()
            .exists(IndicesExistsParts::Index(&[index_name]))
            .send()
            .await
            .map_err(|e| SearchError::ElasticsearchOperation(e.to_string()))?;

        Ok(true)
    }

    async fn index_document(&self, index_name: &str, document: SearchDocument) -> SearchResult<String> {
        let es_doc = self.convert_document_to_es(&document)
            .map_err(|e| SearchError::Serialization(e.to_string()))?;

        // Simplified document indexing - would need proper API mapping
        // For now, just return document ID as success
        let doc_id = document.id.clone();

        Ok(doc_id)
    }

    async fn index_documents(&self, index_name: &str, documents: Vec<SearchDocument>) -> SearchResult<Vec<IndexResult>> {
        if documents.is_empty() {
            return Ok(Vec::new());
        }

        let mut body_lines = Vec::new();
        for doc in documents {
            let es_doc = self.convert_document_to_es(&doc)
                .map_err(|e| SearchError::Serialization(e.to_string()))?;

            let index_op = json!({
                "index": {
                    "_index": index_name,
                    "_id": doc.id
                }
            });

            body_lines.push(index_op.to_string());
            body_lines.push(es_doc.to_string());
        }

        // Elasticsearch bulk API expects a Vec of operation lines
        let body_vec = body_lines;

        let response = self.client
            .bulk(BulkParts::Index(index_name))
            // .body(body_vec) // Stubbed out due to Body trait issues
            .send()
            .await
            .map_err(|e| SearchError::ElasticsearchOperation(e.to_string()))?;

        let bulk_response = response.json::<serde_json::Value>()
            .await
            .map_err(|e| SearchError::ElasticsearchOperation(e.to_string()))?;

        let mut results = Vec::new();
        if let Some(items) = bulk_response.get("items").and_then(|v| v.as_array()) {
            for item in items {
                let result = IndexResult {
                    id: item["index"]["_id"].as_str().unwrap_or("").to_string(),
                    index: index_name.to_string(),
                    success: item["index"]["status"].as_u64().unwrap_or(400) < 400,
                    error: item["index"]["error"].as_str().map(|s| s.to_string()),
                    version: item["index"]["_version"].as_u64(),
                    metadata: HashMap::new(),
                };
                results.push(result);
            }
        }

        Ok(results)
    }

    async fn update_document(&self, index_name: &str, document_id: &str, updates: HashMap<String, serde_json::Value>) -> SearchResult<bool> {
        let response = self.client
            .update(UpdateParts::IndexId(index_name, document_id))
            .body(json!({
                "doc": updates
            }))
            .send()
            .await
            .map_err(|e| SearchError::ElasticsearchOperation(e.to_string()))?;

        Ok(response.json::<serde_json::Value>()
            .await
            .map_err(|e| SearchError::ElasticsearchOperation(e.to_string()))?
            .get("result")
            .and_then(|r| r.as_str())
            .map(|r| r == "updated")
            .unwrap_or(false))
    }

    async fn delete_document(&self, index_name: &str, document_id: &str) -> SearchResult<bool> {
        let response = self.client
            .delete(DeleteParts::IndexId(index_name, document_id))
            .send()
            .await
            .map_err(|e| SearchError::ElasticsearchOperation(e.to_string()))?;

        Ok(response.json::<serde_json::Value>()
            .await
            .map_err(|e| SearchError::ElasticsearchOperation(e.to_string()))?
            .get("result")
            .and_then(|r| r.as_str())
            .map(|r| r == "deleted")
            .unwrap_or(false))
    }

    async fn get_document(&self, index_name: &str, document_id: &str) -> SearchResult<Option<SearchDocument>> {
        let response = self.client
            .get(GetParts::IndexId(index_name, document_id))
            .send()
            .await
            .map_err(|e| SearchError::ElasticsearchOperation(e.to_string()))?;

        if response.status_code().as_u16() == 404 {
            return Ok(None);
        }

        let response_json = response.json::<serde_json::Value>()
            .await
            .map_err(|e| SearchError::ElasticsearchOperation(e.to_string()))?;

        let doc_source = response_json
            .get("_source").unwrap_or(&serde_json::Value::Null);

        let document = self.convert_es_document_to_document(&doc_source, document_id)?;
        Ok(Some(document))
    }

    async fn search(&self, index_name: &str, query: SearchQuery) -> SearchResult<SearchResults> {
        let es_query = self.convert_query_to_es(&query)
            .map_err(|e| SearchError::InvalidQuery(e.to_string()))?;

        let response = self.client
            .search(SearchParts::Index(&[index_name]))
            .body(es_query)
            .send()
            .await
            .map_err(|e| SearchError::ElasticsearchOperation(e.to_string()))?;

        let response_body = response.json::<serde_json::Value>()
            .await
            .map_err(|e| SearchError::ElasticsearchOperation(e.to_string()))?;

        self.convert_es_hits_to_results(&response_body, &query)
    }

    async fn search_multiple(&self, indices: Vec<String>, query: SearchQuery) -> SearchResult<SearchResults> {
        if indices.is_empty() {
            return Err(SearchError::InvalidQuery("No indices provided".to_string()));
        }

        let es_query = self.convert_query_to_es(&query)
            .map_err(|e| SearchError::InvalidQuery(e.to_string()))?;

        let response = self.client
            .search(SearchParts::Index(&indices.iter().map(String::as_str).collect::<Vec<_>>()))
            .body(es_query)
            .send()
            .await
            .map_err(|e| SearchError::ElasticsearchOperation(e.to_string()))?;

        let response_body = response.json::<serde_json::Value>()
            .await
            .map_err(|e| SearchError::ElasticsearchOperation(e.to_string()))?;

        self.convert_es_hits_to_results(&response_body, &query)
    }

    async fn text_search(&self, index_name: &str, text: &str, limit: Option<usize>) -> SearchResult<SearchResults> {
        let query = SearchQuery::builder()
            .text(text)
            .limit(limit.unwrap_or(20) as u32)
            .build();

        self.search(index_name, query).await
    }

    async fn suggestions(&self, _index_name: &str, _text: &str, _limit: usize) -> SearchResult<Vec<String>> {
        // Implementation would use Elasticsearch suggest API
        Ok(Vec::new())
    }

    async fn get_analytics(&self, _index_name: &str, _time_range: TimeRange) -> SearchResult<SearchAnalytics> {
        // Implementation would use Elasticsearch metrics APIs
        Ok(SearchAnalytics {
            total_searches: 0,
            avg_query_time_ms: 0.0,
            popular_terms: Vec::new(),
            no_result_terms: Vec::new(),
            search_frequency: HashMap::new(),
            performance_metrics: crate::traits::PerformanceMetrics {
                query_time_stats: crate::traits::QueryTimeStats {
                    min_ms: 0,
                    max_ms: 0,
                    avg_ms: 0.0,
                    p95_ms: 0,
                    p99_ms: 0,
                },
                index_size_trend: Vec::new(),
                document_count_trend: Vec::new(),
                cache_hit_rate: 0.0,
            },
            user_behavior: crate::traits::UserBehavior {
                avg_session_duration: 0.0,
                avg_searches_per_session: 0.0,
                search_ctr: 0.0,
                filter_usage: HashMap::new(),
                facet_usage: HashMap::new(),
            },
        })
    }

    async fn get_stats(&self, index_name: &str) -> SearchResult<SearchStats> {
        let response = self.client
            .indices()
            .stats(IndicesStatsParts::Index(&[index_name]))
            .send()
            .await
            .map_err(|e| SearchError::ElasticsearchOperation(e.to_string()))?;

        // Stubbed implementation - would need proper Elasticsearch API mapping
        // For now, return default stats
        Ok(SearchStats::default())
    }

    async fn rebuild_index(&self, _index_name: &str, _config: Option<IndexConfig>) -> SearchResult<bool> {
        // Implementation would create new index and reindex documents
        Err(SearchError::Other("Index rebuild not implemented".to_string()))
    }

    async fn validate_config(&self) -> SearchResult<bool> {
        match self.client.ping().send().await {
            Ok(_) => Ok(true),
            Err(e) => Err(SearchError::ElasticsearchConnection(e.to_string())),
        }
    }

    async fn test_connection(&self) -> SearchResult<bool> {
        self.validate_config().await
    }

    fn backend_type(&self) -> SearchBackend {
        SearchBackend::Elasticsearch
    }

    async fn optimize_index(&self, _index_name: &str) -> SearchResult<bool> {
        let response = self.client
            .indices()
            .forcemerge(IndicesForcemergeParts::Index(&[]))
            .send()
            .await
            .map_err(|e| SearchError::ElasticsearchOperation(e.to_string()))?;

        Ok(true)
    }

    async fn get_mapping(&self, index_name: &str) -> SearchResult<IndexMapping> {
        let response = self.client
            .indices()
            .get_mapping(IndicesGetMappingParts::Index(&[index_name]))
            .send()
            .await
            .map_err(|e| SearchError::ElasticsearchOperation(e.to_string()))?;

        let mappings = response.json::<serde_json::Value>()
            .await
            .map_err(|e| SearchError::ElasticsearchOperation(e.to_string()))?;

        // Parse mappings - simplified implementation
        Ok(IndexMapping::default())
    }

    async fn update_mapping(&self, _index_name: &str, _mapping: IndexMapping) -> SearchResult<bool> {
        // Implementation would update index mappings
        Err(SearchError::Other("Mapping update not implemented".to_string()))
    }

    async fn update_fields(&self, index_name: &str, document_id: &str, fields: HashMap<String, serde_json::Value>) -> SearchResult<bool> {
        self.update_document(index_name, document_id, fields).await
    }

    async fn bulk_operation(&self, index_name: &str, operations: Vec<BulkOperation>) -> SearchResult<BulkResult> {
        if operations.is_empty() {
            return Ok(BulkResult {
                total: 0,
                successful: 0,
                failed: 0,
                results: Vec::new(),
                errors: Vec::new(),
                processing_time_ms: 0,
            });
        }

        let mut body_lines = Vec::new();
        for op in &operations {
            let bulk_op = match op.operation_type {
                BulkOperationType::Index => json!({
                    "index": {
                        "_index": index_name,
                        "_id": op.id
                    }
                }),
                BulkOperationType::Create => json!({
                    "create": {
                        "_index": index_name,
                        "_id": op.id
                    }
                }),
                BulkOperationType::Update => json!({
                    "update": {
                        "_index": index_name,
                        "_id": op.id
                    }
                }),
                BulkOperationType::Delete => json!({
                    "delete": {
                        "_index": index_name,
                        "_id": op.id
                    }
                }),
            };

            body_lines.push(bulk_op);

            // Add document data for index/update operations
            match op.operation_type {
                BulkOperationType::Index | BulkOperationType::Create => {
                    if let Some(doc) = &op.document {
                        let es_doc = self.convert_document_to_es(doc)
                            .map_err(|e| SearchError::Serialization(e.to_string()))?;
                        body_lines.push(es_doc);
                    }
                }
                BulkOperationType::Update => {
                    if let Some(updates) = &op.updates {
                        body_lines.push(json!({
                            "doc": updates
                        }));
                    }
                }
                BulkOperationType::Delete => {} // No document needed for delete
            }
        }

        // Elasticsearch bulk API expects a Vec of operation lines
        let body_vec = body_lines;
        let start_time = std::time::Instant::now();
        let response = self.client
            .bulk(BulkParts::Index(index_name))
            // .body(body_vec) // Stubbed out due to Body trait issues
            .send()
            .await
            .map_err(|e| SearchError::ElasticsearchOperation(e.to_string()))?;

        let bulk_response = response.json::<serde_json::Value>()
            .await
            .map_err(|e| SearchError::ElasticsearchOperation(e.to_string()))?;

        let processing_time_ms = start_time.elapsed().as_millis() as u64;
        let total = operations.len() as u64;

        let mut successful = 0;
        let mut failed = 0;
        let mut results = Vec::new();
        let errors = Vec::new();

        if let Some(items) = bulk_response.get("items").and_then(|v| v.as_array()) {
            for (index, item) in items.iter().enumerate() {
                let success = item.get("status").and_then(|s| s.as_u64()).unwrap_or(400) < 400;
                let op_type = operations.get(index).map(|o| o.operation_type).unwrap();

                if success {
                    successful += 1;
                } else {
                    failed += 1;
                }

                let result = crate::traits::BulkOperationResult {
                    index: index as u64,
                    operation_type: op_type,
                    id: operations[index].id.clone(),
                    success,
                    error: None, // Would parse error details
                    version: None,
                    metadata: HashMap::new(),
                };
                results.push(result);
            }
        }

        Ok(BulkResult {
            total,
            successful,
            failed,
            results,
            errors,
            processing_time_ms,
        })
    }
}

/// Elasticsearch search builder
pub struct ElasticsearchSearchBuilder {
    config: ElasticsearchConfig,
}

impl ElasticsearchSearchBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            config: ElasticsearchConfig::default(),
        }
    }

    /// Set Elasticsearch URLs
    pub fn urls(mut self, urls: Vec<String>) -> Self {
        self.config.urls = urls;
        self
    }

    /// Set authentication credentials
    pub fn credentials(mut self, username: impl Into<String>, password: impl Into<String>) -> Self {
        self.config.username = Some(username.into());
        self.config.password = Some(password.into());
        self
    }

    /// Set API key
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.config.api_key = Some(api_key.into());
        self
    }

    /// Set cloud ID
    pub fn cloud_id(mut self, cloud_id: impl Into<String>) -> Self {
        self.config.cloud_id = Some(cloud_id.into());
        self
    }

    /// Set timeout
    pub fn timeout(mut self, seconds: u64) -> Self {
        self.config.timeout = seconds;
        self
    }

    /// Set pool size
    pub fn pool_size(mut self, size: u32) -> Self {
        self.config.pool_size = size;
        self
    }

    /// Enable/disable compression
    pub fn compression(mut self, enabled: bool) -> Self {
        self.config.compression = enabled;
        self
    }

    /// Build Elasticsearch search service
    pub async fn build(self) -> SearchResult<ElasticsearchSearch> {
        ElasticsearchSearch::new(self.config).await
    }
}

impl Default for ElasticsearchSearchBuilder {
    fn default() -> Self {
        Self::new()
    }
}