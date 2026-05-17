//! Simplified RabbitMQ Queue Implementation
//!
//! A basic implementation of RabbitMQ queue service focusing on core functionality.
//! This version provides a working foundation for RabbitMQ integration.

use crate::{
    traits::QueueService,
    types::{QueueMessage, BatchReceiveResult, QueueHealthCheck, QueueHealth},
    QueueResult, QueueBackend, QueueStats, QueueError,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use lapin::{
    options::*, types::FieldTable, BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind,
    Consumer,
};
use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;

/// Basic RabbitMQ exchange types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExchangeType {
    Direct,
    Fanout,
    Topic,
}

/// Simple RabbitMQ configuration
#[derive(Debug, Clone)]
pub struct RabbitMQConfig {
    pub connection_url: String,
    pub queue_name: String,
    pub exchange_name: String,
    pub exchange_type: ExchangeType,
    pub routing_key: Option<String>,
}

/// Build a development-friendly `RabbitMQConfig` pointing at `localhost:5672`.
pub fn dev_config(
    queue_name: impl Into<String>,
    exchange_name: impl Into<String>,
    exchange_type: ExchangeType,
) -> RabbitMQConfig {
    let queue_name = queue_name.into();
    RabbitMQConfig {
        connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
        routing_key: Some(queue_name.clone()),
        queue_name,
        exchange_name: exchange_name.into(),
        exchange_type,
    }
}

/// Build a `RabbitMQConfig` for production use, using the given connection URL.
pub fn prod_config(
    connection_url: impl Into<String>,
    queue_name: impl Into<String>,
    exchange_name: impl Into<String>,
    exchange_type: ExchangeType,
) -> RabbitMQConfig {
    let queue_name = queue_name.into();
    RabbitMQConfig {
        connection_url: connection_url.into(),
        routing_key: Some(queue_name.clone()),
        queue_name,
        exchange_name: exchange_name.into(),
        exchange_type,
    }
}

impl Default for RabbitMQConfig {
    fn default() -> Self {
        Self {
            connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
            queue_name: "default_queue".to_string(),
            exchange_name: "default_exchange".to_string(),
            exchange_type: ExchangeType::Direct,
            routing_key: None,
        }
    }
}

/// Simple RabbitMQ queue service
#[derive(Debug)]
pub struct RabbitMQQueueSimple {
    pub config: RabbitMQConfig,
    #[allow(dead_code)] // Kept for potential reconnection logic
    connection: Option<Arc<Connection>>,
    channel: Option<Arc<Mutex<Channel>>>,
    consumer: Option<Arc<Mutex<Consumer>>>,
    delivery_tags: Arc<Mutex<HashMap<String, u64>>>, // message_id -> delivery_tag
}

impl RabbitMQQueueSimple {
    /// Create new RabbitMQ queue service
    pub async fn new(config: RabbitMQConfig) -> QueueResult<Self> {
        // Validate config
        Self::validate_connection_url(&config.connection_url)?;

        // Establish actual RabbitMQ connection
        let connection = Self::connect_to_rabbitmq(&config.connection_url).await?;

        // Create channel
        let channel = connection
            .create_channel()
            .await
            .map_err(|e| QueueError::ConnectionError(format!("Failed to create channel: {}", e)))?;

        let channel = Arc::new(Mutex::new(channel));

        // Declare exchange
        Self::declare_exchange(&channel, &config).await?;

        // Declare queue
        Self::declare_queue(&channel, &config).await?;

        // Bind queue to exchange
        Self::bind_queue(&channel, &config).await?;

        let queue = Self {
            config: config.clone(),
            connection: Some(Arc::new(connection)),
            channel: Some(channel),
            consumer: None,
            delivery_tags: Arc::new(Mutex::new(HashMap::new())),
        };

        tracing::info!(
            "RabbitMQ queue service initialized - Exchange: {}, Queue: {}",
            queue.config.exchange_name,
            queue.config.queue_name
        );

        Ok(queue)
    }

    /// Connect to RabbitMQ
    async fn connect_to_rabbitmq(connection_url: &str) -> QueueResult<Connection> {
        Connection::connect(connection_url, ConnectionProperties::default())
            .await
            .map_err(|e| QueueError::ConnectionError(format!("Failed to connect to RabbitMQ: {}", e)))
    }

    /// Declare exchange
    async fn declare_exchange(channel: &Arc<Mutex<Channel>>, config: &RabbitMQConfig) -> QueueResult<()> {
        let channel_guard = channel.lock().await;
        let exchange_kind = match config.exchange_type {
            ExchangeType::Direct => ExchangeKind::Direct,
            ExchangeType::Fanout => ExchangeKind::Fanout,
            ExchangeType::Topic => ExchangeKind::Topic,
        };

        channel_guard
            .exchange_declare(
                config.exchange_name.as_str(),
                exchange_kind,
                ExchangeDeclareOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|e| QueueError::ConfigError(format!("Failed to declare exchange: {}", e)))?;

        tracing::debug!("Declared exchange: {}", config.exchange_name);
        Ok(())
    }

    /// Declare queue
    async fn declare_queue(channel: &Arc<Mutex<Channel>>, config: &RabbitMQConfig) -> QueueResult<()> {
        let channel_guard = channel.lock().await;
        let options = QueueDeclareOptions {
            durable: true,
            ..Default::default()
        };

        channel_guard
            .queue_declare(
                config.queue_name.as_str(),
                options,
                FieldTable::default(),
            )
            .await
            .map_err(|e| QueueError::ConfigError(format!("Failed to declare queue: {}", e)))?;

        tracing::debug!("Declared queue: {}", config.queue_name);
        Ok(())
    }

    /// Bind queue to exchange
    async fn bind_queue(channel: &Arc<Mutex<Channel>>, config: &RabbitMQConfig) -> QueueResult<()> {
        let channel_guard = channel.lock().await;
        let routing_key = config.routing_key.as_deref().unwrap_or(&config.queue_name);

        channel_guard
            .queue_bind(
                config.queue_name.as_str(),
                config.exchange_name.as_str(),
                routing_key,
                QueueBindOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|e| QueueError::ConfigError(format!("Failed to bind queue: {}", e)))?;

        tracing::debug!(
            "Bound queue {} to exchange {} with routing key {}",
            config.queue_name,
            config.exchange_name,
            routing_key
        );
        Ok(())
    }

    /// Get channel, ensuring connection is active
    async fn get_channel(&self) -> QueueResult<Arc<Mutex<Channel>>> {
        self.channel
            .clone()
            .ok_or_else(|| QueueError::ConnectionError("No active channel".to_string()))
    }

    /// Validate connection URL format
    fn validate_connection_url(url: &str) -> QueueResult<()> {
        if url.is_empty() {
            return Err(QueueError::ConfigError("Connection URL cannot be empty".to_string()));
        }

        if !url.starts_with("amqp://") && !url.starts_with("amqps://") {
            return Err(QueueError::ConfigError(
                "Connection URL must start with amqp:// or amqps://".to_string(),
            ));
        }

        Ok(())
    }
}

#[async_trait]
impl QueueService for RabbitMQQueueSimple {
    async fn enqueue(&self, message: QueueMessage) -> QueueResult<String> {
        let channel = self.get_channel().await?;
        let channel_guard = channel.lock().await;

        // Serialize message payload
        let payload = serde_json::to_vec(&message.payload)
            .map_err(|e| QueueError::Serialization(format!("Failed to serialize message: {}", e)))?;

        // Create basic properties
        let props = BasicProperties::default()
            .with_delivery_mode(2) // Persistent
            .with_message_id(lapin::types::ShortString::from(message.id.clone()))
            .with_timestamp(chrono::Utc::now().timestamp() as u64);

        // Get routing key
        let routing_key = self.config.routing_key.as_deref().unwrap_or(&self.config.queue_name);

        // Publish to exchange
        let publish_confirm = channel_guard
            .basic_publish(
                self.config.exchange_name.as_str(),
                routing_key,
                BasicPublishOptions::default(),
                &payload,
                props,
            )
            .await
            .map_err(|e| QueueError::PublishError(format!("Failed to publish message: {}", e)))?
            .await
            .map_err(|e| QueueError::PublishError(format!("Publisher confirm failed: {}", e)))?;

        if !publish_confirm.is_ack() {
            return Err(QueueError::PublishError("Message not acknowledged by broker".to_string()));
        }

        tracing::debug!("Published RabbitMQ message: {}", message.id);
        Ok(format!("rabbitmq-{}", message.id))
    }

    async fn enqueue_batch(&self, messages: Vec<QueueMessage>) -> QueueResult<Vec<String>> {
        let mut ids = Vec::with_capacity(messages.len());
        for message in messages {
            ids.push(self.enqueue(message).await?);
        }
        Ok(ids)
    }

    async fn dequeue(&self) -> QueueResult<Option<QueueMessage>> {
        let channel = self.get_channel().await?;

        // Create consumer if it doesn't exist
        if self.consumer.is_none() {
            let channel_guard = channel.lock().await;
            let _consumer = channel_guard
                .basic_consume(
                    self.config.queue_name.as_str(),
                    "", // Consumer tag
                    BasicConsumeOptions::default(),
                    FieldTable::default(),
                )
                .await
                .map_err(|e| QueueError::ConsumerError(format!("Failed to create consumer: {}", e)))?;

            drop(channel_guard);
            // Note: We would need to store this consumer, but for simplicity in this implementation
            // we'll create a new consumer each time. In production, you'd want to reuse it.
        }

        // For a proper implementation, we'd need to store the consumer and poll it
        // For now, we'll use basic_get which is simpler but less efficient
        let channel_guard = channel.lock().await;
        let opt = channel_guard
            .basic_get(
                self.config.queue_name.as_str(),
                BasicGetOptions::default(),
            )
            .await
            .map_err(|e| QueueError::ConsumerError(format!("Failed to get message: {}", e)))?;

        match opt {
            Some(delivery) => {
                // Deserialize message
                let payload = serde_json::from_slice::<serde_json::Value>(&delivery.data)
                    .map_err(|e| QueueError::Deserialization(format!("Failed to deserialize message: {}", e)))?;

                // Store delivery tag for ack/nack
                let message_id = if let Some(id) = delivery.properties.message_id() {
                    id.to_string()
                } else {
                    uuid::Uuid::new_v4().to_string()
                };
                let delivery_tag = delivery.delivery_tag;
                {
                    let mut tags = self.delivery_tags.lock().await;
                    tags.insert(message_id.clone(), delivery_tag);
                }

                // Ack immediately for now (in production, you'd ack after processing)
                channel_guard
                    .basic_ack(delivery_tag, BasicAckOptions::default())
                    .await
                    .map_err(|e| QueueError::ConsumerError(format!("Failed to ack message: {}", e)))?;

                // Remove from tracking after ack
                {
                    let mut tags = self.delivery_tags.lock().await;
                    tags.remove(&message_id);
                }

                let message = QueueMessage {
                    id: message_id,
                    payload,
                    priority: crate::types::QueuePriority::Normal,
                    receive_count: 0,
                    max_receive_count: 3,
                    enqueued_at: chrono::Utc::now(),
                    created_at: chrono::Utc::now(),
                    visible_at: chrono::Utc::now(),
                    expires_at: None,
                    visibility_timeout: 30,
                    status: crate::types::MessageStatus::Pending,
                    delay_seconds: None,
                    attributes: std::collections::HashMap::new(),
                    headers: std::collections::HashMap::new(),
                    message_group_id: None,
                    message_deduplication_id: None,
                    compressed: false,
                    original_size: None,
                    routing_key: self.config.routing_key.clone().or_else(|| Some(self.config.queue_name.clone())),
                };

                tracing::debug!("Consumed RabbitMQ message: {}", message.id);
                Ok(Some(message))
            }
            None => {
                // No messages available
                Ok(None)
            }
        }
    }

    async fn dequeue_batch(&self, max_messages: usize) -> QueueResult<BatchReceiveResult> {
        let start = std::time::Instant::now();
        let mut messages = Vec::new();

        for _ in 0..max_messages {
            if let Some(msg) = self.dequeue().await? {
                messages.push(msg);
            } else {
                break;
            }
        }

        // Get actual queue size
        let queue_size = self.size().await? as usize;

        Ok(BatchReceiveResult {
            messages,
            requested: max_messages,
            available: queue_size,
            total_in_queue: queue_size as u64,
            processing_time_ms: start.elapsed().as_millis() as u64,
        })
    }

    async fn ack(&self, message_id: &str) -> QueueResult<bool> {
        let channel = self.get_channel().await?;

        // Get delivery tag
        let delivery_tag = {
            let mut tags = self.delivery_tags.lock().await;
            tags.remove(message_id)
        };

        if let Some(delivery_tag) = delivery_tag {
            let channel_guard = channel.lock().await;
            channel_guard
                .basic_ack(delivery_tag, BasicAckOptions::default())
                .await
                .map_err(|e| QueueError::ConsumerError(format!("Failed to ack message: {}", e)))?;

            tracing::debug!("Acknowledged RabbitMQ message: {}", message_id);
            Ok(true)
        } else {
            // Message not in tracking (already acked or never existed)
            tracing::warn!("Attempted to ack unknown message: {}", message_id);
            Ok(false)
        }
    }

    async fn ack_batch(&self, message_ids: Vec<String>) -> QueueResult<u64> {
        let mut acked = 0;
        for message_id in message_ids {
            if self.ack(&message_id).await? {
                acked += 1;
            }
        }
        Ok(acked)
    }

    async fn nack(&self, message_id: &str, _delay_seconds: Option<u64>) -> QueueResult<bool> {
        let channel = self.get_channel().await?;

        // Get delivery tag
        let delivery_tag = {
            let mut tags = self.delivery_tags.lock().await;
            tags.remove(message_id)
        };

        if let Some(delivery_tag) = delivery_tag {
            let channel_guard = channel.lock().await;
            channel_guard
                .basic_nack(delivery_tag, BasicNackOptions::default())
                .await
                .map_err(|e| QueueError::ConsumerError(format!("Failed to nack message: {}", e)))?;

            tracing::debug!("Negative acknowledged RabbitMQ message: {}", message_id);
            Ok(true)
        } else {
            // Message not in tracking
            tracing::warn!("Attempted to nack unknown message: {}", message_id);
            Ok(false)
        }
    }

    async fn delete(&self, message_id: &str) -> QueueResult<bool> {
        // RabbitMQ doesn't support deleting messages directly
        // The closest equivalent is ack (which removes it from the queue)
        self.ack(message_id).await
    }

    async fn get_message(&self, message_id: &str) -> QueueResult<Option<QueueMessage>> {
        // RabbitMQ doesn't support getting messages by ID directly
        // This would require a separate message store
        tracing::warn!("Getting RabbitMQ message by ID not supported: {}", message_id);
        Ok(None)
    }

    async fn get_stats(&self) -> QueueResult<QueueStats> {
        let channel = self.get_channel().await?;
        let channel_guard = channel.lock().await;

        // Get queue message count
        let queue_declare = channel_guard
            .queue_declare(
                self.config.queue_name.as_str(),
                QueueDeclareOptions {
                    passive: true, // Don't create, just check
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(|e| QueueError::ConnectionError(format!("Failed to get queue stats: {}", e)))?;

        let message_count = queue_declare.message_count();
        let consumer_count = queue_declare.consumer_count();

        Ok(QueueStats {
            total_messages: message_count as u64,
            visible_messages: message_count as u64,
            invisible_messages: 0,
            delayed_messages: 0,
            dead_letter_messages: 0,
            avg_processing_time_ms: None,
            messages_per_second: None,
            total_processed: 0,
            total_failed: 0,
            queue_age_seconds: Some(0),
            backend_stats: {
                let mut stats = std::collections::HashMap::new();
                stats.insert("consumers".to_string(), serde_json::json!(consumer_count));
                stats.insert("queue_name".to_string(), serde_json::json!(self.config.queue_name));
                stats.insert("exchange".to_string(), serde_json::json!(self.config.exchange_name));
                stats
            },
        })
    }

    async fn purge(&self) -> QueueResult<u64> {
        let channel = self.get_channel().await?;
        let channel_guard = channel.lock().await;

        let message_count = channel_guard
            .queue_purge(self.config.queue_name.as_str(), QueuePurgeOptions::default())
            .await
            .map_err(|e| QueueError::ConnectionError(format!("Failed to purge queue: {}", e)))?;

        tracing::info!("Purged {} messages from RabbitMQ queue: {}", message_count, self.config.queue_name);
        Ok(message_count as u64)
    }

    async fn size(&self) -> QueueResult<u64> {
        let channel = self.get_channel().await?;
        let channel_guard = channel.lock().await;

        let queue_declare = channel_guard
            .queue_declare(
                self.config.queue_name.as_str(),
                QueueDeclareOptions {
                    passive: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(|e| QueueError::ConnectionError(format!("Failed to get queue size: {}", e)))?;

        Ok(queue_declare.message_count() as u64)
    }

    async fn is_empty(&self) -> QueueResult<bool> {
        let size = self.size().await?;
        Ok(size == 0)
    }

    async fn health_check(&self) -> QueueResult<QueueHealthCheck> {
        let start = std::time::Instant::now();

        // Test connection by checking if we can get channel
        let _channel = self.get_channel().await?;

        // Get queue stats
        let stats = self.get_stats().await?;

        // Calculate health status
        let status = if stats.total_messages > 10000 {
            QueueHealth::Degraded
        } else {
            QueueHealth::Healthy
        };

        Ok(QueueHealthCheck {
            status,
            queue_size: stats.total_messages,
            avg_processing_time_ms: None,
            messages_per_second: None,
            error_rate: 0.0,
            last_activity: Some(chrono::Utc::now()),
            checked_at: chrono::Utc::now(),
            details: {
                let mut details = std::collections::HashMap::new();
                details.insert("backend".to_string(), "rabbitmq".to_string());
                details.insert("exchange".to_string(), self.config.exchange_name.clone());
                details.insert("queue".to_string(), self.config.queue_name.clone());
                details.insert("response_time_ms".to_string(), start.elapsed().as_millis().to_string());
                details
            },
        })
    }

    async fn validate_config(&self) -> QueueResult<bool> {
        Self::validate_connection_url(&self.config.connection_url)?;
        Ok(true)
    }

    async fn test_connection(&self) -> QueueResult<bool> {
        // Validate URL
        Self::validate_connection_url(&self.config.connection_url)?;

        // Test actual connection by trying to use the channel
        let channel = self.get_channel().await?;

        // Try to get queue info (this will fail if connection is down)
        let channel_guard = channel.lock().await;
        channel_guard
            .queue_declare(
                self.config.queue_name.as_str(),
                QueueDeclareOptions {
                    passive: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(|e| QueueError::ConnectionError(format!("Connection test failed: {}", e)))?;

        tracing::debug!("RabbitMQ connection test successful");
        Ok(true)
    }

    fn backend_type(&self) -> QueueBackend {
        QueueBackend::RabbitMQ
    }
}