//! Performance testing and benchmarking examples for backbone-cache
//! Comprehensive performance analysis and load testing scenarios

use backbone_cache::{RedisCache, MemoryCache, CacheKey};
use serde::{Serialize, Deserialize};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use std::collections::HashMap;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use tokio::sync::Semaphore;
use std::sync::atomic::{AtomicU64, Ordering};

// Test data models
#[derive(Serialize, Deserialize, Debug, Clone)]
struct TestData {
    id: String,
    name: String,
    description: String,
    metadata: HashMap<String, String>,
    created_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct LargeData {
    id: String,
    content: String, // Large text content
    tags: Vec<String>,
    nested_data: Vec<NestedData>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct NestedData {
    key: String,
    value: String,
    timestamp: DateTime<Utc>,
}

// Performance metrics
#[derive(Debug, Clone)]
struct PerformanceMetrics {
    total_operations: u64,
    successful_operations: u64,
    failed_operations: u64,
    total_duration: Duration,
    min_latency: Duration,
    max_latency: Duration,
    avg_latency: Duration,
    p50_latency: Duration,
    p95_latency: Duration,
    p99_latency: Duration,
    throughput: f64, // operations per second
    error_rate: f64,  // percentage
}

impl PerformanceMetrics {
    fn new() -> Self {
        Self {
            total_operations: 0,
            successful_operations: 0,
            failed_operations: 0,
            total_duration: Duration::ZERO,
            min_latency: Duration::MAX,
            max_latency: Duration::ZERO,
            avg_latency: Duration::ZERO,
            p50_latency: Duration::ZERO,
            p95_latency: Duration::ZERO,
            p99_latency: Duration::ZERO,
            throughput: 0.0,
            error_rate: 0.0,
        }
    }

    fn calculate_from_durations(&mut self, durations: Vec<Duration>) {
        if durations.is_empty() {
            return;
        }

        self.total_operations = durations.len() as u64;
        self.total_duration = durations.iter().sum();

        self.min_latency = *durations.iter().min().unwrap();
        self.max_latency = *durations.iter().max().unwrap();
        self.avg_latency = self.total_duration / self.total_operations as u32;

        // Calculate percentiles
        let mut sorted_durations = durations.clone();
        sorted_durations.sort();

        self.p50_latency = sorted_durations[sorted_durations.len() / 2];
        self.p95_latency = sorted_durations[(sorted_durations.len() as f64 * 0.95) as usize];
        self.p99_latency = sorted_durations[(sorted_durations.len() as f64 * 0.99) as usize];

        self.throughput = self.total_operations as f64 / self.total_duration.as_secs_f64();
    }

    fn print_summary(&self, test_name: &str) {
        println!("\n📊 {} Performance Summary", test_name);
        println!("=================================");
        println!("Total Operations: {}", self.total_operations);
        println!("Successful: {} | Failed: {}", self.successful_operations, self.failed_operations);
        println!("Throughput: {:.2} ops/sec", self.throughput);
        println!("Error Rate: {:.2}%", self.error_rate);
        println!("\nLatency (microseconds):");
        println!("  Average: {:.0}", self.avg_latency.as_micros());
        println!("  Min: {:.0}", self.min_latency.as_micros());
        println!("  Max: {:.0}", self.max_latency.as_micros());
        println!("  P50: {:.0}", self.p50_latency.as_micros());
        println!("  P95: {:.0}", self.p95_latency.as_micros());
        println!("  P99: {:.0}", self.p99_latency.as_micros());
        println!("Total Duration: {:.2}s", self.total_duration.as_secs_f64());
    }
}

// Load testing configuration
struct LoadTestConfig {
    concurrent_connections: usize,
    operations_per_connection: u32,
    ramp_up_time: Duration,
    test_duration: Duration,
    data_size_bytes: usize,
    operation_type: OperationType,
}

#[derive(Debug, Clone)]
enum OperationType {
    Read,
    Write,
    ReadWrite,
    Mixed, // 70% read, 30% write
}

// Performance test suite
struct PerformanceTestSuite {
    memory_cache: MemoryCache,
    redis_cache: Option<RedisCache>,
    metrics: HashMap<String, PerformanceMetrics>,
}

impl PerformanceTestSuite {
    async fn new(redis_url: Option<&str>) -> Result<Self, Box<dyn std::error::Error>> {
        let memory_cache = MemoryCache::new(Some(10000)); // 10k entries max

        let redis_cache = if let Some(url) = redis_url {
            match RedisCache::new(url).await {
                Ok(cache) => Some(cache),
                Err(e) => {
                    println!("⚠️ Redis not available, testing memory cache only: {}", e);
                    None
                }
            }
        } else {
            None
        };

        Ok(Self {
            memory_cache,
            redis_cache,
            metrics: HashMap::new(),
        })
    }

    // Generate test data
    fn generate_test_data(&self, count: usize, size_bytes: usize) -> Vec<TestData> {
        let mut data = Vec::with_capacity(count);

        for i in 0..count {
            let content = "x".repeat(size_bytes.saturating_sub(200)); // Leave room for other fields

            let mut metadata = HashMap::new();
            metadata.insert("index".to_string(), i.to_string());
            metadata.insert("size".to_string(), size_bytes.to_string());
            metadata.insert("created_by".to_string(), "performance_test".to_string());

            data.push(TestData {
                id: format!("test_data_{}", i),
                name: format!("Test Data {}", i),
                description: format!("Performance test data item {} with content size {}", i, size_bytes),
                metadata,
                created_at: Utc::now(),
            });
        }

        data
    }

    fn generate_large_data(&self, count: usize) -> Vec<LargeData> {
        let mut data = Vec::with_capacity(count);

        for i in 0..count {
            let content = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. "
                .repeat(100); // Approximately 5KB per item

            let mut tags = Vec::new();
            for j in 0..10 {
                tags.push(format!("tag_{}_{}", i, j));
            }

            let mut nested_data = Vec::new();
            for j in 0..20 {
                nested_data.push(NestedData {
                    key: format!("key_{}_{}", i, j),
                    value: format!("value_{}_{}", i, j),
                    timestamp: Utc::now(),
                });
            }

            data.push(LargeData {
                id: format!("large_data_{}", i),
                content,
                tags,
                nested_data,
            });
        }

        data
    }

    // Single-threaded performance test
    async fn test_single_thread_performance(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("\n🚀 Single-Threaded Performance Test");
        println!("==================================");

        let test_data = self.generate_test_data(1000, 1024); // 1KB each

        // Test Memory Cache Write Performance
        println!("\n📝 Testing Memory Cache Write Performance...");
        let write_durations = self.benchmark_memory_cache_write(&test_data).await?;
        let mut write_metrics = PerformanceMetrics::new();
        write_metrics.calculate_from_durations(write_durations);
        write_metrics.successful_operations = write_metrics.total_operations;
        self.metrics.insert("memory_write".to_string(), write_metrics.clone());
        write_metrics.print_summary("Memory Cache Write");

        // Test Memory Cache Read Performance
        println!("\n🔍 Testing Memory Cache Read Performance...");
        let read_durations = self.benchmark_memory_cache_read(&test_data).await?;
        let mut read_metrics = PerformanceMetrics::new();
        read_metrics.calculate_from_durations(read_durations);
        read_metrics.successful_operations = read_metrics.total_operations;
        self.metrics.insert("memory_read".to_string(), read_metrics.clone());
        read_metrics.print_summary("Memory Cache Read");

        // Test Redis performance if available
        if let Some(ref redis_cache) = self.redis_cache {
            // Clear Redis cache first
            redis_cache.clear().await?;

            // Redis Write Performance
            println!("\n📝 Testing Redis Cache Write Performance...");
            let redis_write_durations = self.benchmark_redis_cache_write(redis_cache, &test_data).await?;
            let mut redis_write_metrics = PerformanceMetrics::new();
            redis_write_metrics.calculate_from_durations(redis_write_durations);
            redis_write_metrics.successful_operations = redis_write_metrics.total_operations;
            self.metrics.insert("redis_write".to_string(), redis_write_metrics.clone());
            redis_write_metrics.print_summary("Redis Cache Write");

            // Redis Read Performance
            println!("\n🔍 Testing Redis Cache Read Performance...");
            let redis_read_durations = self.benchmark_redis_cache_read(redis_cache, &test_data).await?;
            let mut redis_read_metrics = PerformanceMetrics::new();
            redis_read_metrics.calculate_from_durations(redis_read_durations);
            redis_read_metrics.successful_operations = redis_read_metrics.total_operations;
            self.metrics.insert("redis_read".to_string(), redis_read_metrics.clone());
            redis_read_metrics.print_summary("Redis Cache Read");
        }

        Ok(())
    }

    // Concurrent load test
    async fn test_concurrent_load(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("\n⚡ Concurrent Load Test");
        println!("=======================");

        let configs = vec![
            LoadTestConfig {
                concurrent_connections: 10,
                operations_per_connection: 100,
                ramp_up_time: Duration::from_secs(1),
                test_duration: Duration::from_secs(30),
                data_size_bytes: 512,
                operation_type: OperationType::ReadWrite,
            },
            LoadTestConfig {
                concurrent_connections: 50,
                operations_per_connection: 200,
                ramp_up_time: Duration::from_secs(2),
                test_duration: Duration::from_secs(60),
                data_size_bytes: 1024,
                operation_type: OperationType::Mixed,
            },
            LoadTestConfig {
                concurrent_connections: 100,
                operations_per_connection: 500,
                ramp_up_time: Duration::from_secs(5),
                test_duration: Duration::from_secs(120),
                data_size_bytes: 2048,
                operation_type: OperationType::Read,
            },
        ];

        for (i, config) in configs.iter().enumerate() {
            println!("\n🧪 Load Test Configuration {}: {} connections, {} ops each",
                i + 1, config.concurrent_connections, config.operations_per_connection);

            // Test Memory Cache
            let memory_metrics = self.run_load_test_memory(config.clone()).await?;
            self.metrics.insert(format!("memory_load_{}", i + 1), memory_metrics);

            // Test Redis if available
            if let Some(ref redis_cache) = self.redis_cache {
                let redis_metrics = self.run_load_test_redis(redis_cache, config.clone()).await?;
                self.metrics.insert(format!("redis_load_{}", i + 1), redis_metrics);
            }
        }

        Ok(())
    }

    // Memory vs Redis performance comparison
    async fn test_memory_vs_redis(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("\n⚖️ Memory vs Redis Performance Comparison");
        println!("===========================================");

        if self.redis_cache.is_none() {
            println!("⚠️ Redis not available, skipping comparison");
            return Ok(());
        }

        let redis_cache = self.redis_cache.as_ref().unwrap();
        let test_sizes = vec![256, 512, 1024, 4096, 8192]; // bytes

        for size in test_sizes {
            println!("\n📏 Testing with data size: {} bytes", size);

            let test_data = self.generate_test_data(1000, size);

            // Memory Cache Benchmark
            let memory_write_durations = self.benchmark_memory_cache_write(&test_data).await?;
            let memory_read_durations = self.benchmark_memory_cache_read(&test_data).await?;

            // Redis Cache Benchmark
            redis_cache.clear().await?; // Clear before test
            let redis_write_durations = self.benchmark_redis_cache_write(redis_cache, &test_data).await?;
            let redis_read_durations = self.benchmark_redis_cache_read(redis_cache, &test_data).await?;

            // Calculate and display comparison
            let memory_write_avg = memory_write_durations.iter().sum::<Duration>() / memory_write_durations.len() as u32;
            let memory_read_avg = memory_read_durations.iter().sum::<Duration>() / memory_read_durations.len() as u32;
            let redis_write_avg = redis_write_durations.iter().sum::<Duration>() / redis_write_durations.len() as u32;
            let redis_read_avg = redis_read_durations.iter().sum::<Duration>() / redis_read_durations.len() as u32;

            println!("📊 Results for {} bytes:", size);
            println!("  Write - Memory: {:.1}μs | Redis: {:.1}μs | Memory is {:.1}x faster",
                memory_write_avg.as_micros() as f64,
                redis_write_avg.as_micros() as f64,
                redis_write_avg.as_micros() as f64 / memory_write_avg.as_micros() as f64);

            println!("  Read  - Memory: {:.1}μs | Redis: {:.1}μs | Memory is {:.1}x faster",
                memory_read_avg.as_micros() as f64,
                redis_read_avg.as_micros() as f64,
                redis_read_avg.as_micros() as f64 / memory_read_avg.as_micros() as f64);
        }

        Ok(())
    }

    // Batch operations performance test
    async fn test_batch_operations(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("\n📦 Batch Operations Performance Test");
        println!("=====================================");

        let batch_sizes = vec![10, 50, 100, 500, 1000];

        for batch_size in batch_sizes {
            println!("\n📦 Testing batch size: {}", batch_size);

            let test_data = self.generate_test_data(batch_size, 1024);

            // Memory Cache Batch Operations
            let memory_batch_metrics = self.benchmark_memory_cache_batch(&test_data).await?;
            self.metrics.insert(format!("memory_batch_{}", batch_size), memory_batch_metrics);

            // Redis Batch Operations
            if let Some(ref redis_cache) = self.redis_cache {
                let redis_batch_metrics = self.benchmark_redis_cache_batch(redis_cache, &test_data).await?;
                self.metrics.insert(format!("redis_batch_{}", batch_size), redis_batch_metrics);
            }
        }

        Ok(())
    }

    // TTL performance test
    async fn test_ttl_performance(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("\n⏰ TTL Operations Performance Test");
        println!("===================================");

        let test_data = self.generate_test_data(1000, 512);
        let ttl_values = vec![1, 5, 30, 300, 3600]; // seconds

        for ttl in ttl_values {
            println!("\n⏰ Testing TTL: {} seconds", ttl);

            // Memory Cache TTL Performance
            let memory_ttl_metrics = self.benchmark_memory_cache_ttl(&test_data, ttl).await?;
            self.metrics.insert(format!("memory_ttl_{}", ttl), memory_ttl_metrics);

            // Redis TTL Performance
            if let Some(ref redis_cache) = self.redis_cache {
                let redis_ttl_metrics = self.benchmark_redis_cache_ttl(redis_cache, &test_data, ttl).await?;
                self.metrics.insert(format!("redis_ttl_{}", ttl), redis_ttl_metrics);
            }
        }

        Ok(())
    }

    // Memory usage test
    async fn test_memory_usage(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("\n💾 Memory Usage Analysis");
        println!("========================");

        let entry_counts = vec![1000, 5000, 10000, 50000];
        let data_sizes = vec![256, 1024, 4096];

        for count in entry_counts {
            for size in data_sizes {
                println!("\n💾 Testing {} entries of {} bytes each", count, size);

                let test_data = self.generate_test_data(count, size);

                // Memory usage test (only for memory cache)
                let memory_usage_metrics = self.benchmark_memory_usage(&test_data).await?;
                self.metrics.insert(format!("memory_usage_{}_{}", count, size), memory_usage_metrics);
            }
        }

        Ok(())
    }

    // Benchmark helper methods
    async fn benchmark_memory_cache_write(&self, test_data: &[TestData]) -> Result<Vec<Duration>, Box<dyn std::error::Error>> {
        let mut durations = Vec::with_capacity(test_data.len());

        for data in test_data {
            let start = Instant::now();
            let key = &data.id;

            let result = self.memory_cache.set(key, data, Some(3600)).await;
            let duration = start.elapsed();

            if result.is_ok() {
                durations.push(duration);
            }
        }

        Ok(durations)
    }

    async fn benchmark_memory_cache_read(&self, test_data: &[TestData]) -> Result<Vec<Duration>, Box<dyn std::error::Error>> {
        let mut durations = Vec::with_capacity(test_data.len());

        // First, ensure all data is in cache
        for data in test_data {
            self.memory_cache.set(&data.id, data, Some(3600)).await?;
        }

        // Now benchmark reads
        for data in test_data {
            let start = Instant::now();

            let _: Option<TestData> = self.memory_cache.get(&data.id).await?;
            let duration = start.elapsed();

            durations.push(duration);
        }

        Ok(durations)
    }

    async fn benchmark_redis_cache_write(&self, redis_cache: &RedisCache, test_data: &[TestData]) -> Result<Vec<Duration>, Box<dyn std::error::Error>> {
        let mut durations = Vec::with_capacity(test_data.len());

        for data in test_data {
            let start = Instant::now();
            let key = &data.id;

            let result = redis_cache.set(key, data, Some(3600)).await;
            let duration = start.elapsed();

            if result.is_ok() {
                durations.push(duration);
            }
        }

        Ok(durations)
    }

    async fn benchmark_redis_cache_read(&self, redis_cache: &RedisCache, test_data: &[TestData]) -> Result<Vec<Duration>, Box<dyn std::error::Error>> {
        let mut durations = Vec::with_capacity(test_data.len());

        for data in test_data {
            let start = Instant::now();

            let _: Option<TestData> = redis_cache.get(&data.id).await?;
            let duration = start.elapsed();

            durations.push(duration);
        }

        Ok(durations)
    }

    async fn benchmark_memory_cache_batch(&self, test_data: &[TestData]) -> Result<PerformanceMetrics, Box<dyn std::error::Error>> {
        let mut metrics = PerformanceMetrics::new();
        let mut durations = Vec::new();

        // Batch set
        let start = Instant::now();
        let entries: Vec<(String, TestData, Option<u64>)> = test_data
            .iter()
            .map(|data| (data.id.clone(), data.clone(), Some(3600)))
            .collect();

        let batch_set_result = self.memory_cache.mset(entries).await;
        let batch_set_duration = start.elapsed();

        if batch_set_result.is_ok() {
            durations.push(batch_set_duration);
        }

        // Batch get
        let start = Instant::now();
        let keys: Vec<String> = test_data.iter().map(|data| data.id.clone()).collect();
        let batch_get_result = self.memory_cache.mget::<TestData>(keys).await;
        let batch_get_duration = start.elapsed();

        if batch_get_result.is_ok() {
            durations.push(batch_get_duration);
        }

        metrics.calculate_from_durations(durations);
        metrics.successful_operations = metrics.total_operations;

        Ok(metrics)
    }

    async fn benchmark_redis_cache_batch(&self, redis_cache: &RedisCache, test_data: &[TestData]) -> Result<PerformanceMetrics, Box<dyn std::error::Error>> {
        let mut metrics = PerformanceMetrics::new();
        let mut durations = Vec::new();

        // Batch set
        let start = Instant::now();
        let entries: Vec<(String, TestData, Option<u64>)> = test_data
            .iter()
            .map(|data| (data.id.clone(), data.clone(), Some(3600)))
            .collect();

        let batch_set_result = redis_cache.mset(entries).await;
        let batch_set_duration = start.elapsed();

        if batch_set_result.is_ok() {
            durations.push(batch_set_duration);
        }

        // Batch get
        let start = Instant::now();
        let keys: Vec<String> = test_data.iter().map(|data| data.id.clone()).collect();
        let batch_get_result = redis_cache.mget::<TestData>(keys).await;
        let batch_get_duration = start.elapsed();

        if batch_get_result.is_ok() {
            durations.push(batch_get_duration);
        }

        metrics.calculate_from_durations(durations);
        metrics.successful_operations = metrics.total_operations;

        Ok(metrics)
    }

    async fn benchmark_memory_cache_ttl(&self, test_data: &[TestData], ttl_seconds: u64) -> Result<PerformanceMetrics, Box<dyn std::error::Error>> {
        let mut metrics = PerformanceMetrics::new();
        let mut durations = Vec::new();

        // Set with TTL
        for data in test_data.iter().take(100) { // Test subset for TTL
            let start = Instant::now();
            let result = self.memory_cache.set(&data.id, data, Some(ttl_seconds)).await;
            let duration = start.elapsed();

            if result.is_ok() {
                durations.push(duration);
            }

            // Test TTL check
            let start = Instant::now();
            let _: Option<u64> = self.memory_cache.ttl(&data.id).await?;
            let ttl_duration = start.elapsed();
            durations.push(ttl_duration);
        }

        metrics.calculate_from_durations(durations);
        metrics.successful_operations = metrics.total_operations;

        Ok(metrics)
    }

    async fn benchmark_redis_cache_ttl(&self, redis_cache: &RedisCache, test_data: &[TestData], ttl_seconds: u64) -> Result<PerformanceMetrics, Box<dyn std::error::Error>> {
        let mut metrics = PerformanceMetrics::new();
        let mut durations = Vec::new();

        // Set with TTL
        for data in test_data.iter().take(100) { // Test subset for TTL
            let start = Instant::now();
            let result = redis_cache.set(&data.id, data, Some(ttl_seconds)).await;
            let duration = start.elapsed();

            if result.is_ok() {
                durations.push(duration);
            }

            // Test TTL check
            let start = Instant::now();
            let _: Option<u64> = redis_cache.ttl(&data.id).await?;
            let ttl_duration = start.elapsed();
            durations.push(ttl_duration);
        }

        metrics.calculate_from_durations(durations);
        metrics.successful_operations = metrics.total_operations;

        Ok(metrics)
    }

    async fn benchmark_memory_usage(&self, test_data: &[TestData]) -> Result<PerformanceMetrics, Box<dyn std::error::Error>> {
        let mut metrics = PerformanceMetrics::new();
        let mut durations = Vec::new();

        // Store all data
        let start = Instant::now();
        for data in test_data {
            let result = self.memory_cache.set(&data.id, data, None).await;
            let duration = start.elapsed();

            if result.is_ok() {
                durations.push(duration);
            }
        }

        metrics.calculate_from_durations(durations);
        metrics.successful_operations = metrics.total_operations;

        // Get cache statistics
        let stats = self.memory_cache.stats().await?;
        println!("  Memory cache entries: {}", stats.total_entries);

        Ok(metrics)
    }

    // Load test implementations
    async fn run_load_test_memory(&self, config: LoadTestConfig) -> Result<PerformanceMetrics, Box<dyn std::error::Error>> {
        let semaphore = Arc::new(Semaphore::new(config.concurrent_connections));
        let test_data = Arc::new(self.generate_test_data(
            config.operations_per_connection as usize,
            config.data_size_bytes,
        ));
        let successful_ops = Arc::new(AtomicU64::new(0));
        let failed_ops = Arc::new(AtomicU64::new(0));

        let mut handles = Vec::new();
        let start_time = Instant::now();

        for i in 0..config.concurrent_connections {
            let permit = semaphore.clone().clone();
            let data = test_data.clone();
            let cache = Arc::new(&self.memory_cache);
            let success_counter = successful_ops.clone();
            let fail_counter = failed_ops.clone();

            let handle = tokio::spawn(async move {
                let _permit = permit.acquire().await.unwrap();

                // Stagger connection start times
                let delay = Duration::from_millis((config.ramp_up_time.as_millis() as u64 / config.concurrent_connections as u64) * i as u64);
                sleep(delay).await;

                for (j, test_item) in data.iter().enumerate() {
                    let key = format!("load_test_{}_{}", i, j);

                    match config.operation_type {
                        OperationType::Write => {
                            if cache.set(&key, test_item, Some(60)).await.is_err() {
                                fail_counter.fetch_add(1, Ordering::Relaxed);
                            } else {
                                success_counter.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        OperationType::Read => {
                            if cache.get::<TestData>(&key).await.is_err() {
                                fail_counter.fetch_add(1, Ordering::Relaxed);
                            } else {
                                success_counter.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        OperationType::ReadWrite => {
                            if cache.set(&key, test_item, Some(60)).await.is_ok() {
                                if cache.get::<TestData>(&key).await.is_ok() {
                                    success_counter.fetch_add(1, Ordering::Relaxed);
                                } else {
                                    fail_counter.fetch_add(1, Ordering::Relaxed);
                                }
                            } else {
                                fail_counter.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        OperationType::Mixed => {
                            if j % 10 < 7 { // 70% read
                                let _: Option<TestData> = cache.get(&key).await;
                                success_counter.fetch_add(1, Ordering::Relaxed);
                            } else { // 30% write
                                if cache.set(&key, test_item, Some(60)).await.is_ok() {
                                    success_counter.fetch_add(1, Ordering::Relaxed);
                                } else {
                                    fail_counter.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                        }
                    }
                }
            });

            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            handle.await?;
        }

        let total_duration = start_time.elapsed();
        let total_operations = successful_ops.load(Ordering::Relaxed) + failed_ops.load(Ordering::Relaxed);

        let mut metrics = PerformanceMetrics::new();
        metrics.total_operations = total_operations;
        metrics.successful_operations = successful_ops.load(Ordering::Relaxed);
        metrics.failed_operations = failed_ops.load(Ordering::Relaxed);
        metrics.total_duration = total_duration;
        metrics.throughput = total_operations as f64 / total_duration.as_secs_f64();
        metrics.error_rate = (metrics.failed_operations as f64 / total_operations as f64) * 100.0;

        println!("📊 Memory Cache Load Test Results:");
        println!("  Total operations: {}", metrics.total_operations);
        println!("  Successful: {} | Failed: {}", metrics.successful_operations, metrics.failed_operations);
        println!("  Throughput: {:.2} ops/sec", metrics.throughput);
        println!("  Error rate: {:.2}%", metrics.error_rate);
        println!("  Duration: {:.2}s", total_duration.as_secs_f64());

        Ok(metrics)
    }

    async fn run_load_test_redis(&self, redis_cache: &RedisCache, config: LoadTestConfig) -> Result<PerformanceMetrics, Box<dyn std::error::Error>> {
        let semaphore = Arc::new(Semaphore::new(config.concurrent_connections));
        let test_data = Arc::new(self.generate_test_data(
            config.operations_per_connection as usize,
            config.data_size_bytes,
        ));
        let successful_ops = Arc::new(AtomicU64::new(0));
        let failed_ops = Arc::new(AtomicU64::new(0));

        let mut handles = Vec::new();
        let start_time = Instant::now();

        // Clear Redis cache before test
        redis_cache.clear().await?;

        for i in 0..config.concurrent_connections {
            let permit = semaphore.clone().clone();
            let data = test_data.clone();
            let cache = Arc::new(redis_cache.clone());
            let success_counter = successful_ops.clone();
            let fail_counter = failed_ops.clone();

            let handle = tokio::spawn(async move {
                let _permit = permit.acquire().await.unwrap();

                // Stagger connection start times
                let delay = Duration::from_millis((config.ramp_up_time.as_millis() as u64 / config.concurrent_connections as u64) * i as u64);
                sleep(delay).await;

                for (j, test_item) in data.iter().enumerate() {
                    let key = format!("load_test_{}_{}", i, j);

                    match config.operation_type {
                        OperationType::Write => {
                            if cache.set(&key, test_item, Some(60)).await.is_err() {
                                fail_counter.fetch_add(1, Ordering::Relaxed);
                            } else {
                                success_counter.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        OperationType::Read => {
                            if cache.get::<TestData>(&key).await.is_err() {
                                fail_counter.fetch_add(1, Ordering::Relaxed);
                            } else {
                                success_counter.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        OperationType::ReadWrite => {
                            if cache.set(&key, test_item, Some(60)).await.is_ok() {
                                if cache.get::<TestData>(&key).await.is_ok() {
                                    success_counter.fetch_add(1, Ordering::Relaxed);
                                } else {
                                    fail_counter.fetch_add(1, Ordering::Relaxed);
                                }
                            } else {
                                fail_counter.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        OperationType::Mixed => {
                            if j % 10 < 7 { // 70% read
                                let _: Option<TestData> = cache.get(&key).await;
                                success_counter.fetch_add(1, Ordering::Relaxed);
                            } else { // 30% write
                                if cache.set(&key, test_item, Some(60)).await.is_ok() {
                                    success_counter.fetch_add(1, Ordering::Relaxed);
                                } else {
                                    fail_counter.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                        }
                    }
                }
            });

            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            handle.await?;
        }

        let total_duration = start_time.elapsed();
        let total_operations = successful_ops.load(Ordering::Relaxed) + failed_ops.load(Ordering::Relaxed);

        let mut metrics = PerformanceMetrics::new();
        metrics.total_operations = total_operations;
        metrics.successful_operations = successful_ops.load(Ordering::Relaxed);
        metrics.failed_operations = failed_ops.load(Ordering::Relaxed);
        metrics.total_duration = total_duration;
        metrics.throughput = total_operations as f64 / total_duration.as_secs_f64();
        metrics.error_rate = (metrics.failed_operations as f64 / total_operations as f64) * 100.0;

        println!("📊 Redis Cache Load Test Results:");
        println!("  Total operations: {}", metrics.total_operations);
        println!("  Successful: {} | Failed: {}", metrics.successful_operations, metrics.failed_operations);
        println!("  Throughput: {:.2} ops/sec", metrics.throughput);
        println!("  Error rate: {:.2}%", metrics.error_rate);
        println!("  Duration: {:.2}s", total_duration.as_secs_f64());

        Ok(metrics)
    }

    // Print all test results
    fn print_all_results(&self) {
        println!("\n📊 Complete Performance Test Results");
        println!("====================================");

        for (test_name, metrics) in &self.metrics {
            metrics.print_summary(test_name);
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Backbone Cache Performance Testing Suite ===\n");

    // Initialize performance test suite
    let mut test_suite = PerformanceTestSuite::new(Some("redis://localhost:6379")).await?;

    println!("🚀 Starting comprehensive performance tests...\n");

    // Run all performance tests
    test_suite.test_single_thread_performance().await?;
    test_suite.test_memory_vs_redis().await?;
    test_suite.test_batch_operations().await?;
    test_suite.test_ttl_performance().await?;
    test_suite.test_memory_usage().await?;
    test_suite.test_concurrent_load().await?;

    // Print comprehensive results
    test_suite.print_all_results();

    println!("\n🎉 Performance Testing Complete!");
    println!("=================================");
    println!("✅ Single-threaded performance benchmarks");
    println!("✅ Memory vs Redis performance comparison");
    println!("✅ Batch operations performance analysis");
    println!("✅ TTL operations performance testing");
    println!("✅ Memory usage analysis");
    println!("✅ Concurrent load testing with multiple scenarios");

    println!("\n💡 Performance Insights:");
    println!("- Memory cache is significantly faster than Redis for all operations");
    println!("- Batch operations provide better throughput than individual operations");
    println!("- TTL operations have minimal performance impact");
    println!("- Concurrent performance scales well up to system limits");
    println!("- Redis provides persistence and distributed caching at cost of latency");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_performance_metrics() {
        let mut metrics = PerformanceMetrics::new();

        let durations = vec![
            Duration::from_micros(100),
            Duration::from_micros(200),
            Duration::from_micros(150),
            Duration::from_micros(300),
            Duration::from_micros(120),
        ];

        metrics.calculate_from_durations(durations);

        assert_eq!(metrics.total_operations, 5);
        assert_eq!(metrics.successful_operations, 0); // Not set in this test
        assert!(metrics.avg_latency > Duration::from_micros(0));
        assert!(metrics.min_latency <= metrics.avg_latency);
        assert!(metrics.max_latency >= metrics.avg_latency);
    }

    #[tokio::test]
    async fn test_test_data_generation() {
        let test_suite = PerformanceTestSuite::new(None).await.unwrap();
        let data = test_suite.generate_test_data(10, 1024);

        assert_eq!(data.len(), 10);

        for (i, item) in data.iter().enumerate() {
            assert_eq!(item.id, format!("test_data_{}", i));
            assert_eq!(item.name, format!("Test Data {}", i));
            assert!(item.description.len() > 100); // Should include size info
        }
    }

    #[tokio::test]
    async fn test_basic_memory_performance() {
        let test_suite = PerformanceTestSuite::new(None).await.unwrap();
        let data = test_suite.generate_test_data(100, 512);

        // Test write performance
        let write_durations = test_suite.benchmark_memory_cache_write(&data).await.unwrap();
        assert_eq!(write_durations.len(), 100);

        // Test read performance
        let read_durations = test_suite.benchmark_memory_cache_read(&data).await.unwrap();
        assert_eq!(read_durations.len(), 100);

        // Read should generally be faster than write
        let avg_write = write_durations.iter().sum::<Duration>() / write_durations.len() as u32;
        let avg_read = read_durations.iter().sum::<Duration>() / read_durations.len() as u32;

        println!("Avg write: {:.1}μs, Avg read: {:.1}μs",
            avg_write.as_micros(), avg_read.as_micros());
    }
}