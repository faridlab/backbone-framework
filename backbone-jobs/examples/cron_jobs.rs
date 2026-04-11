//! Advanced cron job examples
//!
//! This example demonstrates various cron expression patterns
//! and real-world job scheduling scenarios.

use backbone_jobs::{JobScheduler, JobBuilder, JobSchedulerBuilder, job_types};
use backbone_jobs::job_storage::InMemoryJobStorage;
use backbone_jobs::job_executor::MockQueueService;
use backbone_jobs::cron::CronScheduler;
use chrono::{Utc, TimeZone};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    println!("🕐 Advanced Cron Job Examples");
    println!("============================");

    // Create scheduler
    let storage = Arc::new(InMemoryJobStorage::new());
    let scheduler = JobSchedulerBuilder::new()
        .with_storage(storage)
        .build()?;

    scheduler.start().await?;

    // Demonstrate different cron patterns
    demonstrate_cron_patterns().await?;

    // Schedule real-world cron jobs
    schedule_real_world_jobs(&scheduler).await?;

    // Show next execution times
    show_next_executions(&scheduler).await?;

    println!("\n⏰ Scheduler running with advanced cron jobs...");
    println!("Press Ctrl+C to stop");

    // Keep running
    tokio::signal::ctrl_c().await?;
    scheduler.stop().await?;

    Ok(())
}

async fn demonstrate_cron_patterns() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📅 Cron Expression Patterns:");
    println!("============================");

    let patterns = vec![
        ("Every minute", "* * * * *"),
        ("Every 15 minutes", "*/15 * * * *"),
        ("Every hour at minute 0", "0 * * * *"),
        ("Every 6 hours", "0 */6 * * *"),
        ("Daily at 2 AM", "0 2 * * *"),
        ("Daily at noon (12 PM)", "0 12 * * *"),
        ("Weekly on Sunday at 2 AM", "0 2 * * 0"),
        ("Weekly on Monday at 9 AM", "0 9 * * 1"),
        ("Monthly on 1st at midnight", "0 0 1 * *"),
        ("Weekdays at 9 AM", "0 9 * * 1-5"),
        ("Weekends at 10 AM", "0 10 * * 6,0"),
        ("Business hours (9 AM - 5 PM) hourly", "0 9-17 * * 1-5"),
        ("Quarterly on 1st at 3 AM", "0 3 1 1,4,7,10 *"),
    ];

    for (description, cron_expr) in patterns {
        let scheduler = CronScheduler::new(cron_expr, "UTC")?;
        let now = Utc::now();
        let next = scheduler.next_after(now)?;

        println!("{:<35} {:<20} → Next: {}",
            description,
            cron_expr,
            next.format("%Y-%m-%d %H:%M:%S UTC")
        );
    }

    Ok(())
}

async fn schedule_real_world_jobs(scheduler: &JobScheduler) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🏢 Real-World Job Scheduling:");
    println!("============================");

    // 1. Database backup job using predefined job type
    let backup_job = job_types::daily_backup()?;
    scheduler.schedule_job(backup_job).await?;
    println!("✅ Database backup: Daily at 2 AM");

    // 2. Log rotation and cleanup using predefined job type
    let log_cleanup_job = job_types::weekly_log_cleanup()?;
    scheduler.schedule_job(log_cleanup_job).await?;
    println!("✅ Log cleanup: Weekly on Sunday at 3 AM");

    // 3. Email campaign scheduler using predefined job type
    let email_job = job_types::email_campaign_schedule()?;
    scheduler.schedule_job(email_job).await?;
    println!("✅ Email campaigns: Weekdays at 9 AM");

    // 4. User data sync using predefined job type
    let sync_job = job_types::hourly_data_sync()?;
    scheduler.schedule_job(sync_job).await?;
    println!("✅ User sync: Every hour");

    // 5. Cache warming using predefined job type
    let cache_job = job_types::cache_warming()?;
    scheduler.schedule_job(cache_job).await?;
    println!("✅ Cache warming: Every 30 minutes");

    // 6. Security scan
    let security_job = JobBuilder::new()
        .id("security_scan".to_string())
        .name("Security Vulnerability Scan")
        .description("Scan for security vulnerabilities")
        .cron("0 3 1 * *") // Monthly on 1st at 3 AM
        .queue("security_queue".to_string())
        .payload(json!({
            "type": "security_scan",
            "scan_types": ["dependencies", "configurations", "permissions"],
            "severity_threshold": "medium",
            "notify_admins": true,
            "create_tickets": true
        }))
        .timeout(3600) // 1 hour
        .build()?;

    scheduler.schedule_job(security_job).await?;
    println!("✅ Security scan: Monthly on 1st at 3 AM");

    // 7. Monthly report generation using predefined job type
    let analytics_job = job_types::monthly_report()?;
    scheduler.schedule_job(analytics_job).await?;
    println!("✅ Analytics reports: Monthly on 1st at 6 AM");

    // 8. Session cleanup using predefined job type
    let session_job = job_types::session_cleanup()?;
    scheduler.schedule_job(session_job).await?;
    println!("✅ Session cleanup: Every 6 hours");

    // 9. API usage report
    let api_job = JobBuilder::new()
        .id("api_usage_report".to_string())
        .name("API Usage Report")
        .description("Generate API usage statistics")
        .cron("0 8 * * 1") // Every Monday at 8 AM
        .queue("reports_queue".to_string())
        .payload(json!({
            "type": "api_usage_report",
            "period": "weekly",
            "metrics": ["requests", "errors", "response_time", "users"],
            "group_by": ["endpoint", "user", "status_code"],
            "export_format": "json"
        }))
        .build()?;

    scheduler.schedule_job(api_job).await?;
    println!("✅ API usage report: Every Monday at 8 AM");

    // 10. Database maintenance using predefined job type
    let db_maintenance_job = job_types::database_maintenance()?;
    scheduler.schedule_job(db_maintenance_job).await?;
    println!("✅ DB maintenance: Weekly on Sunday at 1 AM");

    Ok(())
}

async fn show_next_executions(scheduler: &JobScheduler) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n⏰ Next 5 Executions for Each Job:");
    println!("==================================");

    let jobs = scheduler.list_jobs().await?;
    let now = Utc::now();

    for job in jobs {
        println!("\n📋 {} ({})", job.name, job.cron_expression);

        if let Ok(cron_scheduler) = CronScheduler::new(&job.cron_expression, "UTC") {
            let mut next_times = Vec::new();
            let mut current_time = now;

            for _ in 0..5 {
                if let Ok(next_time) = cron_scheduler.next_after(current_time) {
                    next_times.push(next_time);
                    current_time = next_time;
                } else {
                    break;
                }
            }

            for (i, next_time) in next_times.iter().enumerate() {
                let duration = *next_time - now;
                println!("   {}. {} (in {})",
                    i + 1,
                    next_time.format("%Y-%m-%d %H:%M:%S UTC"),
                    format_duration(duration)
                );
            }
        }
    }

    Ok(())
}

fn format_duration(duration: chrono::Duration) -> String {
    let total_seconds = duration.num_seconds();

    if total_seconds < 60 {
        format!("{} seconds", total_seconds)
    } else if total_seconds < 3600 {
        let minutes = total_seconds / 60;
        let seconds = total_seconds % 60;
        if seconds == 0 {
            format!("{} minutes", minutes)
        } else {
            format!("{} minutes {} seconds", minutes, seconds)
        }
    } else if total_seconds < 86400 {
        let hours = total_seconds / 3600;
        let minutes = (total_seconds % 3600) / 60;
        if minutes == 0 {
            format!("{} hours", hours)
        } else {
            format!("{} hours {} minutes", hours, minutes)
        }
    } else {
        let days = total_seconds / 86400;
        let hours = (total_seconds % 86400) / 3600;
        if hours == 0 {
            format!("{} days", days)
        } else {
            format!("{} days {} hours", days, hours)
        }
    }
}