//! Basic job scheduler example
//!
//! This example demonstrates how to set up a basic job scheduler
//! with different types of cron jobs and their execution.

use backbone_jobs::{JobScheduler, JobBuilder, JobSchedulerBuilder};
use backbone_jobs::job_storage::InMemoryJobStorage;
use backbone_jobs::job_executor::MockQueueService;
use chrono::Utc;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Create in-memory storage for demo purposes
    let storage = Arc::new(InMemoryJobStorage::new());

    // Create mock queue service
    let queue_service = Arc::new(MockQueueService::new());

    // Create a scheduler with in-memory storage
    let scheduler = JobSchedulerBuilder::new()
        .with_storage(storage)
        .build()?;

    // Schedule different types of jobs

    // Start the scheduler
    scheduler.start().await?;

    println!("🚀 Job Scheduler started successfully!");

    // Schedule different types of jobs

    // 1. A job that runs every minute for demo purposes
    let frequent_job = JobBuilder::new()
        .id("frequent_job".to_string())
        .name("Frequent Demo Job")
        .description("Runs every minute for demonstration")
        .cron("*/1 * * * *") // Every minute (5-field cron format)
        .queue("demo_queue".to_string())
        .payload(json!({
            "type": "demo",
            "message": "This job runs every minute",
            "timestamp": Utc::now().to_rfc3339()
        }))
        .build()?;

    scheduler.schedule_job(frequent_job).await?;
    println!("✅ Scheduled frequent job (every minute)");

    // 2. A data cleanup job that runs every 5 minutes
    let cleanup_job = JobBuilder::new()
        .id("cleanup_job".to_string())
        .name("Data Cleanup")
        .description("Clean up old demo data")
        .cron("*/5 * * * *") // Every 5 minutes
        .queue("cleanup_queue".to_string())
        .payload(json!({
            "type": "cleanup",
            "target": "demo_data",
            "older_than_minutes": 60
        }))
        .build()?;

    scheduler.schedule_job(cleanup_job).await?;
    println!("✅ Scheduled cleanup job (every 5 minutes)");

    // 3. A health check job that runs every 2 minutes
    let health_job = JobBuilder::new()
        .id("health_check".to_string())
        .name("System Health Check")
        .description("Check system health and status")
        .cron("*/2 * * * *") // Every 2 minutes
        .queue("health_queue".to_string())
        .payload(json!({
            "type": "health_check",
            "services": ["database", "queue", "cache"]
        }))
        .build()?;

    scheduler.schedule_job(health_job).await?;
    println!("✅ Scheduled health check job (every 2 minutes)");

    // 4. A report generation job that runs every 10 minutes
    let report_job = JobBuilder::new()
        .id("report_job".to_string())
        .name("Demo Report Generation")
        .description("Generate demo analytics report")
        .cron("*/10 * * * *") // Every 10 minutes
        .queue("reports_queue".to_string())
        .payload(json!({
            "type": "report",
            "report_type": "demo_analytics",
            "format": "json"
        }))
        .build()?;

    scheduler.schedule_job(report_job).await?;
    println!("✅ Scheduled report job (every 10 minutes)");

    // Display scheduler statistics
    let stats = scheduler.get_statistics().await?;
    println!("\n📊 Initial Scheduler Statistics:");
    println!("   Total jobs: {}", stats.total_jobs);
    println!("   Active workers: {}", stats.active_workers);
    println!("   Queued jobs: {}", stats.queued_jobs);

    // Get list of all scheduled jobs
    let jobs = scheduler.list_jobs().await?;
    println!("\n📋 Scheduled Jobs:");
    for job in jobs {
        println!("   - {} ({})", job.name, job.status);
        println!("     Cron: {}", job.cron_expression);
        println!("     Queue: {}", job.queue);
        println!("     Next run: {}", job.next_run_time.unwrap_or_default());
    }

    println!("\n⏰ Scheduler is now running... Press Ctrl+C to stop");
    println!("💡 Jobs will be executed according to their cron schedules");

    // Simulate job execution by listening to queue messages
    // In a real application, you would have workers processing these messages
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(10));

        loop {
            interval.tick().await;

            // Update statistics every 10 seconds
            match scheduler.get_statistics().await {
                Ok(stats) => {
                    println!("📈 Live Stats - Total: {}, Workers: {}, Queued: {}",
                        stats.total_jobs, stats.active_workers, stats.queued_jobs);
                }
                Err(e) => {
                    eprintln!("❌ Error getting statistics: {}", e);
                }
            }
        }
    });

    // Run for 5 minutes then shutdown gracefully
    println!("⏱️  Running for 5 minutes demonstration...");
    sleep(Duration::from_secs(300)).await;

    // Graceful shutdown
    println!("\n🛑 Shutting down scheduler...");
    scheduler.stop().await?;
    println!("✅ Scheduler shutdown complete");

    Ok(())
}