//! AWS SQS queue implementation

use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_sdk_sqs::{Client as SqsClient, types::{Message, QueueAttributeName, SendMessageBatchRequestEntry, DeleteMessageBatchRequestEntry}};
use crate::{
    QueueResult, QueueError, QueueService, QueueMessage, QueueStats,
    QueueBackend, BatchReceiveResult, QueueHealthCheck, QueueHealth
};
use std::collections::HashMap;
use chrono::Utc;
use uuid::Uuid;

/// AWS SQS queue configuration
#[derive(Debug, Clone)]
pub struct SqsQueueConfig {
    /// AWS region
    pub region: String,

    /// AWS access key ID (optional if using instance profile)
    pub access_key_id: Option<String>,

    /// AWS secret access key (optional if using instance profile)
    pub secret_access_key: Option<String>,

    /// Queue URL or name
    pub queue_url: String,

    /// Queue name (if queue_url is not provided)
    pub queue_name: Option<String>,

    /// Visibility timeout in seconds
    pub visibility_timeout: i32,

    /// Message retention period in seconds
    pub message_retention_period: i32,

    /// Maximum message size in bytes
    pub max_message_size: i32,

    /// Delay seconds for new messages
    pub delay_seconds: i32,

    /// Receive message wait time in seconds
    pub wait_time_seconds: i32,

    /// Maximum number of messages to receive
    pub max_number_of_messages: i32,

    /// Enable long polling
    pub enable_long_polling: bool,

    /// Dead letter queue URL
    pub dead_letter_queue_url: Option<String>,

    /// AWS account ID (for queue URL construction)
    pub account_id: Option<String>,
}

impl Default for SqsQueueConfig {
    fn default() -> Self {
        Self {
            region: "us-east-1".to_string(),
            access_key_id: None,
            secret_access_key: None,
            queue_url: String::new(),
            queue_name: None,
            visibility_timeout: 30,
            message_retention_period: 345600, // 4 days
            max_message_size: 262144, // 256KB
            delay_seconds: 0,
            wait_time_seconds: 20,
            max_number_of_messages: 10,
            enable_long_polling: true,
            dead_letter_queue_url: None,
            account_id: None,
        }
    }
}

/// AWS SQS queue implementation
pub struct SqsQueue {
    config: SqsQueueConfig,
    client: SqsClient,
    queue_url: String,
}

impl SqsQueue {
    /// Create new SQS queue
    pub async fn new(mut config: SqsQueueConfig) -> QueueResult<Self> {
        // Set up AWS config
        let mut aws_config = aws_config::defaults(BehaviorVersion::latest())
            .region(aws_sdk_sqs::config::Region::new(config.region.clone()));

        // Set credentials if provided
        if let (Some(access_key), Some(secret_key)) = (&config.access_key_id, &config.secret_access_key) {
            aws_config = aws_config.credentials_provider(
                aws_sdk_sqs::config::Credentials::new(
                    access_key.clone(),
                    secret_key.clone(),
                    None,
                    None,
                    "backbone-queue",
                )
            );
        }

        let sdk_config = aws_config.load().await;
        let client = aws_sdk_sqs::Client::new(&sdk_config);

        // Get queue URL if not provided
        let queue_url = if config.queue_url.is_empty() {
            if let Some(queue_name) = &config.queue_name {
                if let Some(account_id) = &config.account_id {
                    format!(
                        "https://sqs.{}.amazonaws.com/{}/{}",
                        config.region, account_id, queue_name
                    )
                } else {
                    // Try to get queue URL by name
                    match client.get_queue_url()
                        .queue_name(queue_name)
                        .send()
                        .await
                    {
                        Ok(response) => response.queue_url()
                            .ok_or_else(|| QueueError::ConfigError("Queue URL not found".to_string()))?
                            .to_string(),
                        Err(e) => return Err(QueueError::SqsError(format!("Failed to get queue URL: {}", e))),
                    }
                }
            } else {
                return Err(QueueError::ConfigError("Either queue_url or queue_name must be provided".to_string()));
            }
        } else {
            config.queue_url.clone()
        };

        config.queue_url = queue_url.clone();

        Ok(Self {
            config,
            client,
            queue_url,
        })
    }

    /// Builder for SQS queue
    pub fn builder() -> SqsQueueBuilder {
        SqsQueueBuilder::new()
    }

    /// Convert QueueMessage to SQS message body and attributes
    fn convert_to_sqs_message_data(&self, message: &QueueMessage) -> QueueResult<(String, HashMap<String, aws_sdk_sqs::types::MessageAttributeValue>)> {
        let serialized = serde_json::to_string(message)
            .map_err(|e| QueueError::Serialization(e.to_string()))?;

        // Check message size
        if serialized.len() > self.config.max_message_size as usize {
            return Err(QueueError::MessageTooLarge {
                size: serialized.len(),
                max: self.config.max_message_size as usize,
            });
        }

        // Set message attributes
        let mut attributes = HashMap::new();
        attributes.insert(
            "Priority".to_string(),
            aws_sdk_sqs::types::MessageAttributeValue::builder()
                .data_type("Number")
                .string_value(message.priority.to_string())
                .build()
                .map_err(|e| QueueError::SqsError(format!("Failed to build priority attribute: {}", e)))?,
        );

        if let Some(group_id) = &message.message_group_id {
            attributes.insert(
                "MessageGroupId".to_string(),
                aws_sdk_sqs::types::MessageAttributeValue::builder()
                    .data_type("String")
                    .string_value(group_id.clone())
                    .build()
                    .map_err(|e| QueueError::SqsError(format!("Failed to build group ID attribute: {}", e)))?,
            );
        }

        if let Some(dedup_id) = &message.message_deduplication_id {
            attributes.insert(
                "MessageDeduplicationId".to_string(),
                aws_sdk_sqs::types::MessageAttributeValue::builder()
                    .data_type("String")
                    .string_value(dedup_id.clone())
                    .build()
                    .map_err(|e| QueueError::SqsError(format!("Failed to build dedup ID attribute: {}", e)))?,
            );
        }

        Ok((serialized, attributes))
    }

    /// Convert SQS message to QueueMessage
    fn convert_from_sqs_message(&self, sqs_message: &Message) -> QueueResult<QueueMessage> {
        let body = sqs_message.body()
            .ok_or_else(|| QueueError::Deserialization("Empty message body".to_string()))?;

        let mut message: QueueMessage = serde_json::from_str(body)
            .map_err(|e| QueueError::Deserialization(format!("Failed to parse message: {}", e)))?;

        // Update with SQS-specific fields
        if let Some(receipt_handle) = sqs_message.receipt_handle() {
            message.attributes.insert("receipt_handle".to_string(), receipt_handle.to_string());
        }

        if let Some(message_id) = sqs_message.message_id() {
            message.attributes.insert("sqs_message_id".to_string(), message_id.to_string());
        }

        // Extract message attributes
        if let Some(attributes) = sqs_message.message_attributes() {
            for (key, value) in attributes {
                if let Some(string_value) = value.string_value() {
                    message.attributes.insert(key.clone(), string_value.to_string());
                }
            }
        }

        Ok(message)
    }

    /// Get queue attributes
    async fn get_queue_attributes(&self) -> QueueResult<HashMap<String, String>> {
        let response = self.client.get_queue_attributes()
            .queue_url(&self.queue_url)
            .attribute_names(QueueAttributeName::All)
            .send()
            .await
            .map_err(|e| QueueError::SqsError(format!("Failed to get queue attributes: {}", e)))?;

        let mut attributes = HashMap::new();
        if let Some(attrs) = response.attributes() {
            for (key, value) in attrs {
                attributes.insert(key.to_string(), value.clone());
            }
        }

        Ok(attributes)
    }
}

#[async_trait]
impl QueueService for SqsQueue {
    async fn enqueue(&self, mut message: QueueMessage) -> QueueResult<String> {
        // Validate message
        message.validate()
            .map_err(QueueError::Other)?;

        // Generate ID if not set
        if message.id.is_empty() {
            message.id = Uuid::new_v4().to_string();
        }

        // Convert to SQS format
        let (message_body, message_attributes) = self.convert_to_sqs_message_data(&message)?;

        let mut request = self.client.send_message()
            .queue_url(&self.queue_url)
            .message_body(message_body)
            .set_message_attributes(Some(message_attributes));

        // Set delay seconds
        if let Some(delay) = message.delay_seconds {
            request = request.delay_seconds(delay as i32);
        } else if self.config.delay_seconds > 0 {
            request = request.delay_seconds(self.config.delay_seconds);
        }

        // Set message group ID if present
        if let Some(group_id) = &message.message_group_id {
            request = request.message_group_id(group_id);
        }

        // Set message deduplication ID if present
        if let Some(dedup_id) = &message.message_deduplication_id {
            request = request.message_deduplication_id(dedup_id);
        }

        // Send message
        let response = request.send()
            .await
            .map_err(|e| QueueError::SqsError(format!("Failed to send message: {}", e)))?;

        let message_id = response.message_id()
            .ok_or_else(|| QueueError::SqsError("No message ID returned".to_string()))?;

        Ok(message_id.to_string())
    }

    async fn enqueue_batch(&self, messages: Vec<QueueMessage>) -> QueueResult<Vec<String>> {
        if messages.is_empty() {
            return Ok(Vec::new());
        }

        let mut entries = Vec::with_capacity(messages.len());
        let mut ids = Vec::with_capacity(messages.len());

        for (index, mut message) in messages.into_iter().enumerate() {
            // Validate and prepare message
            message.validate()
                .map_err(QueueError::Other)?;

            if message.id.is_empty() {
                message.id = Uuid::new_v4().to_string();
            }

            let (message_body, message_attributes) = self.convert_to_sqs_message_data(&message)?;
            let entry_id = format!("msg-{}", index);
            ids.push(message.id.clone());

            let mut entry = SendMessageBatchRequestEntry::builder()
                .id(&entry_id)
                .message_body(message_body)
                .set_message_attributes(Some(message_attributes));

            if let Some(delay) = message.delay_seconds {
                entry = entry.delay_seconds(delay as i32);
            }

            if let Some(group_id) = &message.message_group_id {
                entry = entry.message_group_id(group_id);
            }

            if let Some(dedup_id) = &message.message_deduplication_id {
                entry = entry.message_deduplication_id(dedup_id);
            }

            entries.push(entry.build()
                .map_err(|e| QueueError::SqsError(format!("Failed to build batch entry: {}", e)))?);
        }

        // Send batch
        let response = self.client.send_message_batch()
            .queue_url(&self.queue_url)
            .set_entries(Some(entries))
            .send()
            .await
            .map_err(|e| QueueError::SqsError(format!("Failed to send batch messages: {}", e)))?;

        // Check for failed messages
        let failed = response.failed();
        if !failed.is_empty() {
            return Err(QueueError::SqsError(format!("{} messages failed to send", failed.len())));
        }

        Ok(ids)
    }

    async fn dequeue(&self) -> QueueResult<Option<QueueMessage>> {
        let response = self.client.receive_message()
            .queue_url(&self.queue_url)
            .max_number_of_messages(1)
            .wait_time_seconds(if self.config.enable_long_polling { self.config.wait_time_seconds } else { 0 })
            .visibility_timeout(self.config.visibility_timeout)
            .send()
            .await
            .map_err(|e| QueueError::SqsError(format!("Failed to receive message: {}", e)))?;

        let messages = response.messages();
        if !messages.is_empty() {
            let sqs_message = &messages[0];
            let message = self.convert_from_sqs_message(sqs_message)?;
            return Ok(Some(message));
        }

        Ok(None)
    }

    async fn dequeue_batch(&self, max_messages: usize) -> QueueResult<BatchReceiveResult> {
        let start_time = std::time::Instant::now();
        let batch_size = max_messages.min(self.config.max_number_of_messages as usize);

        let response = self.client.receive_message()
            .queue_url(&self.queue_url)
            .max_number_of_messages(batch_size as i32)
            .wait_time_seconds(if self.config.enable_long_polling { self.config.wait_time_seconds } else { 0 })
            .visibility_timeout(self.config.visibility_timeout)
            .send()
            .await
            .map_err(|e| QueueError::SqsError(format!("Failed to receive batch messages: {}", e)))?;

        let mut queue_messages = Vec::new();
        let messages = response.messages();
        for sqs_message in messages {
            if let Ok(message) = self.convert_from_sqs_message(sqs_message) {
                queue_messages.push(message);
            }
        }

        // Get queue size
        let attributes = self.get_queue_attributes().await?;
        let total_in_queue = attributes.get("ApproximateNumberOfMessages")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let available_count = queue_messages.len();
        Ok(BatchReceiveResult {
            messages: queue_messages,
            requested: max_messages,
            available: available_count,
            total_in_queue,
            processing_time_ms: start_time.elapsed().as_millis() as u64,
        })
    }

    async fn ack(&self, _message_id: &str) -> QueueResult<bool> {
        // SQS uses receipt handles for deletion, not message IDs
        // This would require storing the receipt handle when receiving messages
        Err(QueueError::Other(
            "SQS requires receipt handle for message acknowledgment. Use delete() instead.".to_string()
        ))
    }

    async fn ack_batch(&self, _message_ids: Vec<String>) -> QueueResult<u64> {
        Err(QueueError::Other(
            "SQS requires receipt handles for message acknowledgment. Use delete_batch() instead.".to_string()
        ))
    }

    async fn nack(&self, _message_id: &str, _delay_seconds: Option<u64>) -> QueueResult<bool> {
        // SQS doesn't have a direct NACK operation
        // Messages become visible again after visibility timeout
        Err(QueueError::Other(
            "SQS doesn't support NACK. Messages will become visible after visibility timeout.".to_string()
        ))
    }

    async fn delete(&self, _message_id: &str) -> QueueResult<bool> {
        // SQS requires receipt handle for deletion, not message ID
        // This method needs to find the message first to get the receipt handle
        Err(QueueError::Other(
            "SQS requires receipt handle for message deletion. Use delete_by_message() instead.".to_string()
        ))
    }

    async fn delete_batch(&self, messages: Vec<QueueMessage>) -> QueueResult<u64> {
        if messages.is_empty() {
            return Ok(0);
        }

        let mut entries = Vec::with_capacity(messages.len());
        for (index, message) in messages.into_iter().enumerate() {
            if let Some(receipt_handle) = message.attributes.get("receipt_handle") {
                let entry = DeleteMessageBatchRequestEntry::builder()
                    .id(format!("msg-{}", index))
                    .receipt_handle(receipt_handle.clone())
                    .build()
                    .map_err(|e| QueueError::SqsError(format!("Failed to build delete entry: {}", e)))?;
                entries.push(entry);
            }
        }

        if entries.is_empty() {
            return Ok(0);
        }

        let response = self.client.delete_message_batch()
            .queue_url(&self.queue_url)
            .set_entries(Some(entries))
            .send()
            .await
            .map_err(|e| QueueError::SqsError(format!("Failed to delete batch messages: {}", e)))?;

        let successful = response.successful()
            .len() as u64;

        Ok(successful)
    }

    async fn get_message(&self, __message_id: &str) -> QueueResult<Option<QueueMessage>> {
        // SQS doesn't support getting messages by ID directly
        Err(QueueError::Other(
            "SQS doesn't support getting messages by ID directly".to_string()
        ))
    }

    async fn get_stats(&self) -> QueueResult<QueueStats> {
        let attributes = self.get_queue_attributes().await?;
        let mut stats = QueueStats::default();

        stats.visible_messages = attributes.get("ApproximateNumberOfMessages")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        stats.invisible_messages = attributes.get("ApproximateNumberOfMessagesNotVisible")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        stats.total_messages = stats.visible_messages + stats.invisible_messages;

        Ok(stats)
    }

    async fn purge(&self) -> QueueResult<u64> {
        let _response = self.client.purge_queue()
            .queue_url(&self.queue_url)
            .send()
            .await
            .map_err(|e| QueueError::SqsError(format!("Failed to purge queue: {}", e)))?;

        Ok(0) // SQS doesn't return the number of purged messages
    }

    async fn size(&self) -> QueueResult<u64> {
        let attributes = self.get_queue_attributes().await?;
        Ok(attributes.get("ApproximateNumberOfMessages")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0))
    }

    async fn is_empty(&self) -> QueueResult<bool> {
        let size = self.size().await?;
        Ok(size == 0)
    }

    async fn health_check(&self) -> QueueResult<QueueHealthCheck> {
        let stats = self.get_stats().await?;
        let is_connected = self.test_connection().await.unwrap_or(false);

        let status = if is_connected && stats.total_messages < 10000 {
            QueueHealth::Healthy
        } else if is_connected && stats.total_messages < 50000 {
            QueueHealth::Degraded
        } else {
            QueueHealth::Unhealthy
        };

        Ok(QueueHealthCheck {
            status,
            queue_size: stats.total_messages,
            avg_processing_time_ms: None,
            messages_per_second: None,
            error_rate: 0.0,
            last_activity: Some(Utc::now()),
            checked_at: Utc::now(),
            details: {
                let mut details = HashMap::new();
                details.insert("region".to_string(), self.config.region.clone());
                details.insert("queue_url".to_string(), self.queue_url.clone());
                details
            },
        })
    }

    async fn validate_config(&self) -> QueueResult<bool> {
        // Test by getting queue attributes
        match self.get_queue_attributes().await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    async fn test_connection(&self) -> QueueResult<bool> {
        match self.get_queue_attributes().await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    fn backend_type(&self) -> QueueBackend {
        QueueBackend::Sqs
    }
}

/// SQS queue builder
pub struct SqsQueueBuilder {
    config: SqsQueueConfig,
}

impl SqsQueueBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            config: SqsQueueConfig::default(),
        }
    }

    /// Set AWS region
    pub fn region(mut self, region: impl Into<String>) -> Self {
        self.config.region = region.into();
        self
    }

    /// Set AWS credentials
    pub fn credentials(mut self, access_key_id: impl Into<String>, secret_access_key: impl Into<String>) -> Self {
        self.config.access_key_id = Some(access_key_id.into());
        self.config.secret_access_key = Some(secret_access_key.into());
        self
    }

    /// Set queue URL
    pub fn queue_url(mut self, url: impl Into<String>) -> Self {
        self.config.queue_url = url.into();
        self
    }

    /// Set queue name and account ID
    pub fn queue_name(mut self, name: impl Into<String>, account_id: impl Into<String>) -> Self {
        self.config.queue_name = Some(name.into());
        self.config.account_id = Some(account_id.into());
        self
    }

    /// Set visibility timeout
    pub fn visibility_timeout(mut self, timeout: i32) -> Self {
        self.config.visibility_timeout = timeout;
        self
    }

    /// Set wait time seconds
    pub fn wait_time_seconds(mut self, seconds: i32) -> Self {
        self.config.wait_time_seconds = seconds;
        self
    }

    /// Enable/disable long polling
    pub fn long_polling(mut self, enabled: bool) -> Self {
        self.config.enable_long_polling = enabled;
        self
    }

    /// Build SQS queue
    pub async fn build(self) -> QueueResult<SqsQueue> {
        SqsQueue::new(self.config).await
    }
}

impl Default for SqsQueueBuilder {
    fn default() -> Self {
        Self::new()
    }
}

