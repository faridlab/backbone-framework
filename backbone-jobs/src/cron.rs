//! Cron expression parsing and scheduling utilities

use crate::error::{JobError, JobResult};
use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Cron expression parser and scheduler
#[derive(Debug, Clone)]
pub struct CronScheduler {
    expression: CronExpression,
    timezone: chrono_tz::Tz,
}

impl CronScheduler {
    /// Create a new cron scheduler with the given expression and timezone
    pub fn new(expression: &str, timezone: &str) -> JobResult<Self> {
        let cron_expr = CronExpression::parse(expression)?;
        let tz = timezone.parse::<chrono_tz::Tz>()
            .map_err(|_| JobError::time_zone(&format!("Invalid timezone: {}", timezone)))?;

        Ok(Self {
            expression: cron_expr,
            timezone: tz,
        })
    }

    /// Create a new cron scheduler with UTC timezone
    pub fn new_utc(expression: &str) -> JobResult<Self> {
        Self::new(expression, "UTC")
    }

    /// Get the next execution time after the given datetime
    pub fn next_after(&self, after: DateTime<Utc>) -> JobResult<DateTime<Utc>> {
        self.expression.next_after(after, &self.timezone)
    }

    /// Get the next n execution times after the given datetime
    pub fn next_n_after(&self, after: DateTime<Utc>, n: usize) -> JobResult<Vec<DateTime<Utc>>> {
        let mut times = Vec::with_capacity(n);
        let mut current = after;

        for _ in 0..n {
            let next = self.next_after(current)?;
            times.push(next);
            current = next + Duration::seconds(1);
        }

        Ok(times)
    }

    /// Check if the given datetime matches the cron expression
    pub fn matches(&self, datetime: DateTime<Utc>) -> bool {
        self.expression.matches(datetime, &self.timezone)
    }

    /// Get the cron expression
    pub fn expression(&self) -> &CronExpression {
        &self.expression
    }

    /// Get the timezone
    pub fn timezone(&self) -> &chrono_tz::Tz {
        &self.timezone
    }
}

/// Parsed cron expression with all fields
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CronExpression {
    /// Minute field (0-59)
    pub minute: CronField,
    /// Hour field (0-23)
    pub hour: CronField,
    /// Day of month field (1-31)
    pub day_of_month: CronField,
    /// Month field (1-12)
    pub month: CronField,
    /// Day of week field (0-6, 0=Sunday)
    pub day_of_week: CronField,
}

impl CronExpression {
    /// Parse a cron expression string
    /// Format: "minute hour day-of-month month day-of-week"
    /// Examples:
    /// - "0 12 * * *" - Every day at noon
    /// - "*/15 * * * *" - Every 15 minutes
    /// - "0 2 * * 0" - Every Sunday at 2 AM
    pub fn parse(expression: &str) -> JobResult<Self> {
        let parts: Vec<&str> = expression.split_whitespace().collect();

        if parts.len() != 5 {
            return Err(JobError::cron_parsing(
                &format!(
                    "Cron expression must have 5 fields, got {}: '{}'",
                    parts.len(),
                    expression
                )
            ));
        }

        Ok(Self {
            minute: CronField::parse(parts[0], 0, 59)?,
            hour: CronField::parse(parts[1], 0, 23)?,
            day_of_month: CronField::parse(parts[2], 1, 31)?,
            month: CronField::parse(parts[3], 1, 12)?,
            day_of_week: CronField::parse(parts[4], 0, 6)?,
        })
    }

    /// Get the next execution time after the given datetime
    pub fn next_after(&self, after: DateTime<Utc>, timezone: &chrono_tz::Tz) -> JobResult<DateTime<Utc>> {
        let mut current = after + Duration::seconds(1);
        let mut attempts = 0;
        const MAX_ATTEMPTS: u32 = 1000; // Prevent infinite loops

        while attempts < MAX_ATTEMPTS {
            if self.matches(current, timezone) {
                return Ok(current);
            }

            // Increment to next minute
            current = current + Duration::seconds(60 - current.second() as i64);
            current = current.with_nanosecond(0).unwrap();
            attempts += 1;
        }

        Err(JobError::cron_parsing("Could not find next execution time within reasonable bounds"))
    }

    /// Check if the given datetime matches the cron expression
    pub fn matches(&self, datetime: DateTime<Utc>, timezone: &chrono_tz::Tz) -> bool {
        let local = datetime.with_timezone(timezone);

        self.minute.matches(local.minute())
            && self.hour.matches(local.hour())
            && self.day_of_month.matches(local.day())
            && self.month.matches(local.month())
            && self.day_of_week.matches(local.weekday().num_days_from_sunday())
    }

}

impl std::fmt::Display for CronExpression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {} {} {} {}",
            self.minute, self.hour, self.day_of_month, self.month, self.day_of_week
        )
    }
}

/// Individual cron field (minute, hour, day, etc.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CronField {
    /// All values (*)
    All,
    /// Single value
    Single(u32),
    /// Range of values (e.g., 1-5)
    Range(u32, u32),
    /// List of values (e.g., 1,3,5)
    List(HashSet<u32>),
    /// Step values (e.g., */15 or 1-30/5)
    Step(Box<CronField>, u32),
}

impl CronField {
    /// Parse a cron field value
    pub fn parse(value: &str, min: u32, max: u32) -> JobResult<Self> {
        if value == "*" {
            return Ok(CronField::All);
        }

        // Handle step values
        if let Some((base, step)) = value.split_once('/') {
            let step_val = step.parse::<u32>()
                .map_err(|_| JobError::cron_parsing(&format!("Invalid step value: {}", step)))?;

            if step_val == 0 {
                return Err(JobError::cron_parsing("Step value cannot be zero"));
            }

            let base_field = CronField::parse(base, min, max)?;
            return Ok(CronField::Step(Box::new(base_field), step_val));
        }

        // Handle ranges
        if let Some((start, end)) = value.split_once('-') {
            let start_val = start.parse::<u32>()
                .map_err(|_| JobError::cron_parsing(&format!("Invalid range start: {}", start)))?;
            let end_val = end.parse::<u32>()
                .map_err(|_| JobError::cron_parsing(&format!("Invalid range end: {}", end)))?;

            if start_val < min || end_val > max || start_val > end_val {
                return Err(JobError::cron_parsing(&format!(
                    "Invalid range values: {}-{} (expected {}-{})",
                    start_val, end_val, min, max
                )));
            }

            return Ok(CronField::Range(start_val, end_val));
        }

        // Handle lists
        if value.contains(',') {
            let values: Result<HashSet<u32>, _> = value
                .split(',')
                .map(|v| {
                    v.parse::<u32>().map_err(|_| {
                        JobError::cron_parsing(&format!("Invalid list value: {}", v))
                    })
                })
                .collect();

            let values = values?;
            for &val in &values {
                if val < min || val > max {
                    return Err(JobError::cron_parsing(&format!(
                        "Value {} out of range (expected {}-{})",
                        val, min, max
                    )));
                }
            }
            return Ok(CronField::List(values));
        }

        // Single value
        let single_val = value.parse::<u32>()
            .map_err(|_| JobError::cron_parsing(&format!("Invalid value: {}", value)))?;

        if single_val < min || single_val > max {
            return Err(JobError::cron_parsing(&format!(
                "Value {} out of range (expected {}-{})",
                single_val, min, max
            )));
        }

        Ok(CronField::Single(single_val))
    }

    /// Check if the field matches a given value
    pub fn matches(&self, value: u32) -> bool {
        match self {
            CronField::All => true,
            CronField::Single(v) => *v == value,
            CronField::Range(start, end) => value >= *start && value <= *end,
            CronField::List(values) => values.contains(&value),
            CronField::Step(base, step) => {
                if let CronField::All = **base {
                    value.is_multiple_of(*step)
                } else if base.matches(value) {
                    // For step on specific values, we need to find the offset
                    match **base {
                        CronField::Single(base_val) => (value - base_val).is_multiple_of(*step),
                        CronField::Range(start, _) => (value - start).is_multiple_of(*step),
                        _ => false, // Complex cases handled differently
                    }
                } else {
                    false
                }
            }
        }
    }

}

impl std::fmt::Display for CronField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CronField::All => write!(f, "*"),
            CronField::Single(v) => write!(f, "{}", v),
            CronField::Range(start, end) => write!(f, "{}-{}", start, end),
            CronField::List(values) => {
                let mut sorted: Vec<_> = values.iter().collect();
                sorted.sort();
                write!(f, "{}", sorted.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(","))
            }
            CronField::Step(base, step) => write!(f, "{}/{}", base, step),
        }
    }
}

/// Common cron expression templates
pub mod templates {
    

    /// Every minute
    pub const EVERY_MINUTE: &str = "* * * * *";

    /// Every hour at minute 0
    pub const HOURLY: &str = "0 * * * *";

    /// Every day at midnight
    pub const DAILY_MIDNIGHT: &str = "0 0 * * *";

    /// Every day at noon
    pub const DAILY_NOON: &str = "0 12 * * *";

    /// Every Monday at 9 AM
    pub const WEEKLY_MONDAY_9AM: &str = "0 9 * * 1";

    /// Every Sunday at 2 AM
    pub const WEEKLY_SUNDAY_2AM: &str = "0 2 * * 0";

    /// First day of every month at midnight
    pub const MONTHLY_FIRST: &str = "0 0 1 * *";

    /// Last day of every month at midnight
    pub const MONTHLY_LAST: &str = "0 0 L * *";

    /// Every 5 minutes
    pub const EVERY_5_MINUTES: &str = "*/5 * * * *";

    /// Every 15 minutes
    pub const EVERY_15_MINUTES: &str = "*/15 * * * *";

    /// Every 30 minutes
    pub const EVERY_30_MINUTES: &str = "*/30 * * * *";

    /// Every 2 hours
    pub const EVERY_2_HOURS: &str = "0 */2 * * *";

    /// Every 6 hours
    pub const EVERY_6_HOURS: &str = "0 */6 * * *";

    /// Weekdays (Monday-Friday) at 9 AM
    pub const WEEKDAYS_9AM: &str = "0 9 * * 1-5";

    /// Weekends (Saturday-Sunday) at 10 AM
    pub const WEEKENDS_10AM: &str = "0 10 * * 6,0";

    /// Business hours (9 AM - 5 PM) every hour on weekdays
    pub const BUSINESS_HOURS: &str = "0 9-17 * * 1-5";
}

/// Helper functions for working with cron expressions
pub mod helpers {
    use super::*;
    use chrono::{DateTime, Utc};

    /// Calculate time until next execution
    pub fn time_until_next(
        expression: &str,
        timezone: &str,
        from: DateTime<Utc>,
    ) -> JobResult<Duration> {
        let scheduler = CronScheduler::new(expression, timezone)?;
        let next = scheduler.next_after(from)?;
        Ok(next - from)
    }

    /// Check if a cron expression is valid
    pub fn is_valid_expression(expression: &str) -> bool {
        CronExpression::parse(expression).is_ok()
    }

    /// Get a human-readable description of a cron expression
    pub fn describe_expression(expression: &str) -> JobResult<String> {
        let cron = CronExpression::parse(expression)?;

        let _minute_desc = describe_field(&cron.minute, "minute");
        let hour_desc = describe_field(&cron.hour, "hour");
        let day_desc = describe_field(&cron.day_of_month, "day");
        let month_desc = describe_field(&cron.month, "month");
        let weekday_desc = describe_field(&cron.day_of_week, "weekday");

        Ok(format!("Runs {} at {} on the {} of {}",
            weekday_desc, hour_desc, day_desc, month_desc))
    }

    fn describe_field(field: &CronField, field_name: &str) -> String {
        match field {
            CronField::All => match field_name {
                "minute" => "every minute".to_string(),
                "hour" => "every hour".to_string(),
                "day" => "every day".to_string(),
                "month" => "every month".to_string(),
                "weekday" => "every day".to_string(),
                _ => format!("every {}", field_name),
            },
            CronField::Single(v) => format!("at {}", v),
            CronField::Range(start, end) => format!("from {} to {}", start, end),
            CronField::List(values) => {
                let mut sorted: Vec<_> = values.iter().collect();
                sorted.sort();
                format!("at {}", sorted.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", "))
            }
            CronField::Step(base, step) => match **base {
                CronField::All => format!("every {} {}", step, field_name),
                _ => format!("every {} {}", step, field_name),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use chrono_tz::UTC;

    #[test]
    fn test_cron_parsing() {
        let cron = CronExpression::parse("0 12 * * *").unwrap();
        assert!(matches!(cron.minute, CronField::Single(0)));
        assert!(matches!(cron.hour, CronField::Single(12)));
        assert!(matches!(cron.day_of_month, CronField::All));
        assert!(matches!(cron.month, CronField::All));
        assert!(matches!(cron.day_of_week, CronField::All));
    }

    #[test]
    fn test_cron_ranges() {
        let cron = CronExpression::parse("0 9-17 * * 1-5").unwrap();
        assert!(matches!(cron.hour, CronField::Range(9, 17)));
        assert!(matches!(cron.day_of_week, CronField::Range(1, 5)));
    }

    #[test]
    fn test_cron_lists() {
        let cron = CronExpression::parse("0,30 8,12,18 * * 1,3,5").unwrap();
        assert!(matches!(cron.minute, CronField::List(_)));
        assert!(matches!(cron.hour, CronField::List(_)));
        assert!(matches!(cron.day_of_week, CronField::List(_)));
    }

    #[test]
    fn test_cron_steps() {
        let cron = CronExpression::parse("*/15 * * * *").unwrap();
        assert!(matches!(cron.minute, CronField::Step(_, 15)));
    }

    #[test]
    fn test_next_execution() {
        let scheduler = CronScheduler::new_utc("0 12 * * *").unwrap();
        let now = Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap();
        let next = scheduler.next_after(now).unwrap();
        assert_eq!(next.hour(), 12);
        assert_eq!(next.minute(), 0);
    }

    #[test]
    fn test_multiple_next_executions() {
        let scheduler = CronScheduler::new_utc("*/30 * * * *").unwrap();
        let now = Utc.with_ymd_and_hms(2024, 1, 1, 10, 15, 0).unwrap();
        let next_times = scheduler.next_n_after(now, 3).unwrap();

        assert_eq!(next_times[0].minute(), 30);
        assert_eq!(next_times[1].minute(), 0);
        assert_eq!(next_times[2].minute(), 30);
    }

    #[test]
    fn test_invalid_expressions() {
        assert!(CronExpression::parse("").is_err());
        assert!(CronExpression::parse("0 12 * *").is_err()); // Only 4 fields
        assert!(CronExpression::parse("60 * * * *").is_err()); // Invalid minute
        assert!(CronExpression::parse("0 25 * * *").is_err()); // Invalid hour
    }

    #[test]
    fn test_helpers() {
        assert!(helpers::is_valid_expression("0 12 * * *"));
        assert!(!helpers::is_valid_expression("invalid"));

        let desc = helpers::describe_expression("0 12 * * *").unwrap();
        assert!(desc.contains("12"));
    }

    #[test]
    fn test_templates() {
        assert!(helpers::is_valid_expression(templates::HOURLY));
        assert!(helpers::is_valid_expression(templates::DAILY_MIDNIGHT));
        assert!(helpers::is_valid_expression(templates::WEEKLY_MONDAY_9AM));
        assert!(helpers::is_valid_expression(templates::MONTHLY_FIRST));
        assert!(helpers::is_valid_expression(templates::BUSINESS_HOURS));
    }
}