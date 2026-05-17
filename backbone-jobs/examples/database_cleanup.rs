//! Database cleanup automation example
//!
//! This example demonstrates automated database maintenance tasks
//! including session cleanup, log rotation, data archiving, and optimization.

use backbone_jobs::{JobScheduler, JobBuilder, JobSchedulerBuilder};
use backbone_jobs::job_storage::InMemoryJobStorage;
use backbone_jobs::job_executor::MockQueueService;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    println!("🗄️  Database Cleanup Automation");
    println!("==============================");

    // Create scheduler
    let storage = Arc::new(InMemoryJobStorage::new());
    let scheduler = JobSchedulerBuilder::new()
        .storage(storage)
        .build()
        .await?;

    scheduler.start().await?;

    // Schedule various database cleanup jobs
    schedule_cleanup_jobs(&scheduler).await?;

    // Schedule optimization jobs
    schedule_optimization_jobs(&scheduler).await?;

    // Schedule archiving jobs
    schedule_archiving_jobs(&scheduler).await?;

    // Show job summary
    show_job_summary(&scheduler).await?;

    println!("\n⏰ Database cleanup scheduler running...");
    println!("🔧 Jobs will execute according to their schedules");
    println!("Press Ctrl+C to stop");

    tokio::signal::ctrl_c().await?;
    scheduler.stop().await?;

    Ok(())
}

async fn schedule_cleanup_jobs(scheduler: &JobScheduler) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🧹 Database Cleanup Jobs:");
    println!("==========================");

    // 1. Expired session cleanup (every 6 hours)
    let session_cleanup = JobBuilder::new()
        .id("expired_sessions_cleanup".to_string())
        .name("Expired Sessions Cleanup")
        .description("Remove expired user sessions from database")
        .cron("0 */6 * * *") // Every 6 hours
        .queue("database_maintenance".to_string())
        .payload(json!({
            "type": "session_cleanup",
            "tables": ["user_sessions", "api_tokens", "refresh_tokens"],
            "conditions": {
                "expires_at": {
                    "operator": "<",
                    "value": "now()"
                },
                "last_activity": {
                    "operator": "<",
                    "value": "now() - interval '7 days'"
                }
            },
            "batch_size": 10000,
            "log_statistics": true,
            "notify_admin": false
        }))
        .timeout(1800) // 30 minutes
        .build()?;

    scheduler.schedule_job(session_cleanup).await?;
    println!("✅ Expired sessions: Every 6 hours");

    // 2. Temporary files cleanup (daily)
    let temp_files_cleanup = JobBuilder::new()
        .id("temp_files_cleanup".to_string())
        .name("Temporary Files Cleanup")
        .description("Remove temporary files and upload artifacts")
        .cron("0 3 * * *") // Daily at 3 AM
        .queue("file_maintenance".to_string())
        .payload(json!({
            "type": "temp_files_cleanup",
            "directories": [
                "/tmp/uploads",
                "/var/www/tmp",
                "/tmp/exports",
                "/tmp/reports"
            ],
            "older_than_hours": 24,
            "file_patterns": ["*.tmp", "*.temp", "*.upload", "*.partial"],
            "max_size_mb": 1024,
            "log_deleted_files": true
        }))
        .timeout(3600) // 1 hour
        .build()?;

    scheduler.schedule_job(temp_files_cleanup).await?;
    println!("✅ Temporary files: Daily at 3 AM");

    // 3. Audit log cleanup (weekly)
    let audit_log_cleanup = JobBuilder::new()
        .id("audit_log_cleanup".to_string())
        .name("Audit Log Cleanup")
        .description("Archive and clean old audit log entries")
        .cron("0 2 * * 0") // Sunday at 2 AM
        .queue("database_maintenance".to_string())
        .payload(json!({
            "type": "audit_log_cleanup",
            "tables": ["audit_logs", "access_logs", "error_logs"],
            "archive_policy": {
                "keep_days": 90,
                "archive_table_suffix": "_archive",
                "compression": true
            },
            "conditions": {
                "created_at": {
                    "operator": "<",
                    "value": "now() - interval '90 days'"
                }
            },
            "batch_size": 50000,
            "create_backup": true
        }))
        .timeout(7200) // 2 hours
        .build()?;

    scheduler.schedule_job(audit_log_cleanup).await?;
    println!("✅ Audit logs: Weekly on Sunday at 2 AM");

    // 4. Failed job cleanup (daily)
    let failed_jobs_cleanup = JobBuilder::new()
        .id("failed_jobs_cleanup".to_string())
        .name("Failed Jobs Cleanup")
        .description("Clean up old failed job records")
        .cron("30 1 * * *") // Daily at 1:30 AM
        .queue("job_maintenance".to_string())
        .payload(json!({
            "type": "failed_jobs_cleanup",
            "tables": ["job_executions", "job_attempts", "job_logs"],
            "conditions": {
                "status": "failed",
                "created_at": {
                    "operator": "<",
                    "value": "now() - interval '7 days'"
                }
            },
            "archive_successful": true,
            "keep_recent_failures": 100
        }))
        .timeout(1800) // 30 minutes
        .build()?;

    scheduler.schedule_job(failed_jobs_cleanup).await?;
    println!("✅ Failed jobs: Daily at 1:30 AM");

    // 5. Cache invalidation cleanup (every 4 hours)
    let cache_cleanup = JobBuilder::new()
        .id("cache_invalidation_cleanup".to_string())
        .name("Cache Invalidation Cleanup")
        .description("Remove stale cache invalidation entries")
        .cron("0 */4 * * *") // Every 4 hours
        .queue("cache_maintenance".to_string())
        .payload(json!({
            "type": "cache_invalidation_cleanup",
            "tables": ["cache_invalidation", "cache_tags"],
            "conditions": {
                "created_at": {
                    "operator": "<",
                    "value": "now() - interval '24 hours'"
                },
                "processed": true
            },
            "cleanup_orphaned_tags": true,
            "rebuild_tag_index": false
        }))
        .timeout(900) // 15 minutes
        .build()?;

    scheduler.schedule_job(cache_cleanup).await?;
    println!("✅ Cache invalidation: Every 4 hours");

    Ok(())
}

async fn schedule_optimization_jobs(scheduler: &JobScheduler) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n⚡ Database Optimization Jobs:");
    println!("==============================");

    // 1. Database statistics update (daily)
    let update_stats = JobBuilder::new()
        .id("update_database_stats".to_string())
        .name("Update Database Statistics")
        .description("Update table statistics for query optimizer")
        .cron("0 5 * * *") // Daily at 5 AM
        .queue("database_optimization".to_string())
        .payload(json!({
            "type": "update_statistics",
            "tables": [
                "users", "orders", "products", "sessions",
                "audit_logs", "cache_entries", "job_queue"
            ],
            "sample_percent": 10,
            "parallel_workers": 2,
            "force_update": false,
            "analyze_specific_columns": [
                "users.status", "orders.created_at", "products.category"
            ]
        }))
        .timeout(3600) // 1 hour
        .build()?;

    scheduler.schedule_job(update_stats).await?;
    println!("✅ Update statistics: Daily at 5 AM");

    // 2. Index rebuilding (weekly)
    let rebuild_indexes = JobBuilder::new()
        .id("rebuild_indexes".to_string())
        .name("Rebuild Database Indexes")
        .description("Rebuild fragmented indexes for better performance")
        .cron("0 4 * * 6") // Saturday at 4 AM
        .queue("database_optimization".to_string())
        .payload(json!({
            "type": "rebuild_indexes",
            "indexes": [
                "idx_users_email",
                "idx_orders_user_id_created_at",
                "idx_products_category_status",
                "idx_sessions_user_id",
                "idx_audit_logs_created_at"
            ],
            "rebuild_condition": {
                "fragmentation_percent": "> 30"
            },
            "parallel_rebuild": true,
            "max_parallel_workers": 4
        }))
        .timeout(7200) // 2 hours
        .build()?;

    scheduler.schedule_job(rebuild_indexes).await?;
    println!("✅ Rebuild indexes: Weekly on Saturday at 4 AM");

    // 3. Table partitioning maintenance (monthly)
    let partition_maintenance = JobBuilder::new()
        .id("partition_maintenance".to_string())
        .name("Table Partitioning Maintenance")
        .description("Create new partitions and drop old ones")
        .cron("0 6 1 * *") // Monthly on 1st at 6 AM
        .queue("database_optimization".to_string())
        .payload(json!({
            "type": "partition_maintenance",
            "tables": [
                "audit_logs", "access_logs", "metrics",
                "user_activities", "api_requests"
            ],
            "partition_column": "created_at",
            "partition_type": "monthly",
            "keep_partitions": 12,
            "create_future_partitions": 2,
            "archive_old_partitions": true
        }))
        .timeout(10800) // 3 hours
        .build()?;

    scheduler.schedule_job(partition_maintenance).await?;
    println!("✅ Partition maintenance: Monthly on 1st at 6 AM");

    // 4. Query performance analysis (weekly)
    let query_analysis = JobBuilder::new()
        .id("query_performance_analysis".to_string())
        .name("Query Performance Analysis")
        .description("Analyze slow queries and suggest optimizations")
        .cron("0 3 * * 1") // Monday at 3 AM
        .queue("performance_analysis".to_string())
        .payload(json!({
            "type": "query_analysis",
            "min_duration_ms": 1000,
            "analysis_period_days": 7,
            "top_queries_count": 50,
            "generate_recommendations": true,
            "check_missing_indexes": true,
            "notify_db_team": true,
            "export_to_file": true
        }))
        .timeout(3600) // 1 hour
        .build()?;

    scheduler.schedule_job(query_analysis).await?;
    println!("✅ Query analysis: Weekly on Monday at 3 AM");

    Ok(())
}

async fn schedule_archiving_jobs(scheduler: &JobScheduler) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📦 Data Archiving Jobs:");
    println!("========================");

    // 1. Order data archiving (monthly)
    let order_archiving = JobBuilder::new()
        .id("order_data_archiving".to_string())
        .name("Order Data Archiving")
        .description("Archive old order data to long-term storage")
        .cron("0 2 1 * *") // Monthly on 1st at 2 AM
        .queue("data_archiving".to_string())
        .payload(json!({
            "type": "order_archiving",
            "tables": ["orders", "order_items", "order_payments"],
            "conditions": {
                "order_date": {
                    "operator": "<",
                    "value": "now() - interval '2 years'"
                },
                "status": ["completed", "cancelled"]
            },
            "archive_storage": {
                "type": "s3",
                "bucket": "order-archive",
                "compression": "gzip",
                "encryption": true
            },
            "keep_index_table": true,
            "batch_size": 10000
        }))
        .timeout(14400) // 4 hours
        .build()?;

    scheduler.schedule_job(order_archiving).await?;
    println!("✅ Order archiving: Monthly on 1st at 2 AM");

    // 2. User activity archiving (quarterly)
    let activity_archiving = JobBuilder::new()
        .id("user_activity_archiving".to_string())
        .name("User Activity Archiving")
        .description("Archive old user activity logs")
        .cron("0 3 1 1,4,7,10 *") // Quarterly on 1st at 3 AM
        .queue("data_archiving".to_string())
        .payload(json!({
            "type": "activity_archiving",
            "tables": ["user_activities", "page_views", "click_events"],
            "conditions": {
                "created_at": {
                    "operator": "<",
                    "value": "now() - interval '1 year'"
                }
            },
            "archive_storage": {
                "type": "s3",
                "bucket": "activity-archive",
                "compression": "gzip",
                "partition_by": "year_month"
            },
            "sample_recent_data": {
                "keep_percentage": 10,
                "random_sample": true
            }
        }))
        .timeout(10800) // 3 hours
        .build()?;

    scheduler.schedule_job(activity_archiving).await?;
    println!("✅ Activity archiving: Quarterly on 1st at 3 AM");

    // 3. Analytics data summarization (weekly)
    let analytics_summarization = JobBuilder::new()
        .id("analytics_summarization".to_string())
        .name("Analytics Data Summarization")
        .description("Summarize detailed analytics into aggregated tables")
        .cron("0 1 * * 0") // Sunday at 1 AM
        .queue("analytics_processing".to_string())
        .payload(json!({
            "type": "analytics_summarization",
            "source_tables": ["page_views", "events", "user_actions"],
            "target_tables": ["daily_analytics", "weekly_analytics"],
            "summarization_period": "daily",
            "aggregations": [
                "page_views_count",
                "unique_users_count",
                "session_duration_avg",
                "conversion_rate",
                "revenue_total"
            ],
            "group_by": ["date", "user_type", "source"],
            "drop_source_data": false
        }))
        .timeout(7200) // 2 hours
        .build()?;

    scheduler.schedule_job(analytics_summarization).await?;
    println!("✅ Analytics summarization: Weekly on Sunday at 1 AM");

    Ok(())
}

async fn show_job_summary(scheduler: &JobScheduler) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📊 Scheduled Jobs Summary:");
    println!("========================");

    let jobs = scheduler.list_jobs().await?;
    let mut cleanup_count = 0;
    let mut optimization_count = 0;
    let mut archiving_count = 0;

    for job in &jobs {
        match job.queue.as_str() {
            "database_maintenance" | "file_maintenance" | "cache_maintenance" | "job_maintenance" => {
                cleanup_count += 1;
            }
            "database_optimization" | "performance_analysis" => {
                optimization_count += 1;
            }
            "data_archiving" | "analytics_processing" => {
                archiving_count += 1;
            }
            _ => {}
        }
    }

    println!("🧹 Cleanup jobs: {}", cleanup_count);
    println!("⚡ Optimization jobs: {}", optimization_count);
    println!("📦 Archiving jobs: {}", archiving_count);
    println!("📋 Total jobs: {}", jobs.len());

    println!("\n🔄 Execution Schedule:");
    println!("   Every hour: Session cleanup, Cache cleanup");
    println!("   Daily: Temp files, Failed jobs, Statistics update");
    println!("   Weekly: Audit logs, Index rebuild, Query analysis, Analytics");
    println!("   Monthly: Partitions, Order archiving");
    println!("   Quarterly: Activity archiving");

    Ok(())
}