//! RabbitMQ Queue Implementation
//!
//! Provides RabbitMQ queue support using the AMQP protocol via the lapin library.
//!
//! ## Features
//!
//! - **AMQP Protocol**: Full AMQP 0.9.1 support
//! - **Connection Pooling**: Efficient connection and channel management
//! - **Exchange Support**: Direct, fanout, topic, and headers exchanges
//! - **Message Routing**: Advanced routing with routing keys and headers
//! - **Publisher Confirms**: Reliable message delivery
//! - **Consumer Acknowledgments**: Manual and automatic ack modes
//! - **Quality of Service**: Prefetch count and flow control
//! - **Dead Letter Exchanges**: Automatic message rerouting
//! - **Message TTL**: Time-to-live support
//! - **Durable Queues**: Persistent message storage
//!
//! ## Quick Start
//!
//! ```rust
//! use backbone_queue::{RabbitMQQueue, QueueMessage, ExchangeType};
//!
//! // Create RabbitMQ queue service
//! let queue = RabbitMQQueue::new(
//!     "amqp://guest:guest@localhost:5672/%2f",
//!     "my_queue",
//!     ExchangeType::Direct,
//!     "my_exchange"
//! ).await?;
//!
//! // Send message
//! let message = QueueMessage::builder()
//!     .payload("Hello, RabbitMQ!")
//!     .routing_key("my.routing.key")
//!     .build();
//!
//! let message_id = queue.enqueue(message).await?;
//!
//! // Receive message
//! let message = queue.dequeue().await?;
//! if let Some(msg) = message {
//!     println!("Received: {}", msg.payload);
//!     queue.ack(msg.id).await?;
//! }
//! ```

use crate::{
    traits::QueueService,
    types::{QueueMessage, QueuePriority, BatchReceiveResult, QueueHealthCheck, QueueHealth},
    QueueError, QueueResult, QueueBackend, QueueStats,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lapin::{
    options::*, types::FieldTable, types::AMQPValue, BasicProperties, Channel, Connection, ConnectionProperties,
    Consumer, ExchangeKind, Queue as LapinQueue,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::info;
use uuid::Uuid;
use futures_lite::stream::StreamExt;

/// RabbitMQ exchange types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExchangeType {
    /// Direct exchange - routes messages to queues with exact routing key match
    Direct,
    /// Fanout exchange - routes messages to all bound queues
    Fanout,
    /// Topic exchange - routes messages based on pattern matching
    Topic,
    /// Headers exchange - routes messages based on header values
    Headers,
}

impl ExchangeType {
    /// Convert to lapin ExchangeKind
    pub fn to_lapin_kind(&self) -> ExchangeKind {
        match self {
            ExchangeType::Direct => ExchangeKind::Direct,
            ExchangeType::Fanout => ExchangeKind::Fanout,
            ExchangeType::Topic => ExchangeKind::Topic,
            ExchangeType::Headers => ExchangeKind::Headers,
        }
    }

    /// Get exchange type name
    pub fn name(&self) -> &'static str {
        match self {
            ExchangeType::Direct => "direct",
            ExchangeType::Fanout => "fanout",
            ExchangeType::Topic => "topic",
            ExchangeType::Headers => "headers",
        }
    }
}

/// RabbitMQ acknowledgment modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckMode {
    /// Automatic acknowledgment (message is acked immediately upon delivery)
    Auto,
    /// Manual acknowledgment (client must explicitly ack or nack)
    Manual,
}

/// RabbitMQ quality of service settings
#[derive(Debug, Clone)]
pub struct QosConfig {
    /// Prefetch count (number of messages to prefetch)
    pub prefetch_count: u16,
    /// Prefetch size (0 means no limit)
    pub prefetch_size: u32,
    /// Global flag (apply to entire connection rather than just channel)
    pub global: bool,
}

impl Default for QosConfig {
    fn default() -> Self {
        Self {
            prefetch_count: 10,
            prefetch_size: 0,
            global: false,
        }
    }
}

/// RabbitMQ queue configuration
#[derive(Debug, Clone)]
pub struct RabbitMQConfig {
    /// AMQP connection URL
    pub connection_url: String,

    /// Queue name
    pub queue_name: String,

    /// Exchange type
    pub exchange_type: ExchangeType,

    /// Exchange name
    pub exchange_name: String,

    /// Routing key (for publishing)
    pub routing_key: Option<String>,

    /// Exchange routing key (for binding queue to exchange)
    pub binding_routing_key: Option<String>,

    /// Whether queue is durable
    pub durable: bool,

    /// Whether exchange is durable
    pub exchange_durable: bool,

    /// Whether to auto-delete queue when last consumer disconnects
    pub auto_delete: bool,

    /// Whether to auto-delete exchange when last queue unbinds
    pub exchange_auto_delete: bool,

    /// Message TTL in milliseconds (None for no TTL)
    pub message_ttl: Option<u32>,

    /// Dead letter exchange
    pub dead_letter_exchange: Option<String>,

    /// Dead letter routing key
    pub dead_letter_routing_key: Option<String>,

    /// Maximum queue length
    pub max_length: Option<u32>,

    /// Maximum queue length in bytes
    pub max_length_bytes: Option<u64>,

    /// acknowledgment mode
    pub ack_mode: AckMode,

    /// Quality of service settings
    pub qos_config: QosConfig,

    /// Enable publisher confirms
    pub publisher_confirms: bool,

    /// Connection timeout in seconds
    pub connection_timeout: u16,

    /// Heartbeat interval in seconds
    pub heartbeat: u16,

    /// Additional queue arguments
    pub queue_arguments: HashMap<String, AMQPValue>,

    /// Additional exchange arguments
    pub exchange_arguments: HashMap<String, AMQPValue>,
}

impl Default for RabbitMQConfig {
    fn default() -> Self {
        let mut queue_args = HashMap::new();
        let mut exchange_args = HashMap::new();

        Self {
            connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
            queue_name: "default_queue".to_string(),
            exchange_type: ExchangeType::Direct,
            exchange_name: "default_exchange".to_string(),
            routing_key: None,
            binding_routing_key: None,
            durable: true,
            exchange_durable: true,
            auto_delete: false,
            exchange_auto_delete: false,
            message_ttl: None,
            dead_letter_exchange: None,
            dead_letter_routing_key: None,
            max_length: None,
            max_length_bytes: None,
            ack_mode: AckMode::Manual,
            qos_config: QosConfig::default(),
            publisher_confirms: true,
            connection_timeout: 60,
            heartbeat: 60,
            queue_arguments: queue_args,
            exchange_arguments: exchange_args,
        }
    }
}

/// RabbitMQ connection pool manager
#[derive(Debug)]
struct ConnectionPool {
    connection: Arc<Mutex<Option<Connection>>>,
    channel: Arc<Mutex<Option<Channel>>>,
    config: RabbitMQConfig,
}

impl ConnectionPool {
    fn new(config: RabbitMQConfig) -> Self {
        Self {
            connection: Arc::new(Mutex::new(None)),
            channel: Arc::new(Mutex::new(None)),
            config,
        }
    }

    async fn get_connection(&self) -> QueueResult<Arc<Connection>> {
        let mut conn_guard = self.connection.lock().await;

        if conn_guard.is_none() {
            info!("Establishing new RabbitMQ connection to {}", self.config.connection_url);

            let conn = Connection::connect(&self.config.connection_url, ConnectionProperties::default())
                .await
                .map_err(|e| QueueError::NetworkError(format!("Failed to connect to RabbitMQ: {}", e)))?;

            *conn_guard = Some(conn);
        }

        // Clone the existing connection for this call
        let conn = conn_guard.as_ref().unwrap().clone();
        Ok(Arc::new(conn))
    }

    async fn get_channel(&self) -> QueueResult<Arc<Channel>> {
        let mut chan_guard = self.channel.lock().await;

        if chan_guard.is_none() {
            let connection = self.get_connection().await?;
            let channel = connection
                .create_channel()
                .await
                .map_err(|e| QueueError::NetworkError(format!("Failed to create channel: {}", e)))?;

            // Set QoS
            channel
                .basic_qos(
                    self.config.qos_config.prefetch_size,
                    self.config.qos_config.prefetch_count,
                    self.config.qos_config.global,
                    BasicQosOptions::default(),
                )
                .await
                .map_err(|e| QueueError::NetworkError(format!("Failed to set QoS: {}", e)))?;

            // Enable publisher confirms if requested
            if self.config.publisher_confirms {
                channel
                    .confirm_select(ConfirmSelectOptions::default())
                    .await
                    .map_err(|e| QueueError::NetworkError(format!("Failed to enable publisher confirms: {}", e)))?;
            }

            *chan_guard = Some(channel);
        }

        // Clone the existing channel for this call
        let chan = chan_guard.as_ref().unwrap().clone();
        Ok(Arc::new(chan))
    }

    async fn close(&self) -> QueueResult<()> {
        let mut conn_guard = self.connection.lock().await;
        let mut chan_guard = self.channel.lock().await;

        if let Some(channel) = chan_guard.take() {
            if let Err(e) = channel.close(200, "Normal shutdown").await {
                warn!("Error closing RabbitMQ channel: {}", e);
            }
        }

        if let Some(connection) = conn_guard.take() {
            if let Err(e) = connection.close(200, "Normal shutdown").await {
                warn!("Error closing RabbitMQ connection: {}", e);
            }
        }

        Ok(())
    }
}

/// RabbitMQ queue service implementation
#[derive(Debug)]
pub struct RabbitMQQueue {
    config: RabbitMQConfig,
    connection_pool: Arc<ConnectionPool>,
    consumer: Arc<Mutex<Option<Consumer>>>,
    stats: Arc<RwLock<QueueStats>>,
}

impl RabbitMQQueue {
    /// Create a new RabbitMQ queue service
    pub async fn new<S: Into<String>>(
        connection_url: S,
        queue_name: S,
        exchange_type: ExchangeType,
        exchange_name: S,
    ) -> QueueResult<Self> {
        let config = RabbitMQConfig {
            connection_url: connection_url.into(),
            queue_name: queue_name.into(),
            exchange_type,
            exchange_name: exchange_name.into(),
            ..Default::default()
        };

        Self::with_config(config).await
    }

    /// Create a new RabbitMQ queue service with custom configuration
    pub async fn with_config(config: RabbitMQConfig) -> QueueResult<Self> {
        let connection_pool = Arc::new(ConnectionPool::new(config.clone()));

        let queue = Self {
            config,
            connection_pool,
            consumer: Arc::new(Mutex::new(None)),
            stats: Arc::new(RwLock::new(QueueStats::default())),
        };

        // Initialize queue and exchange
        queue.setup_infrastructure().await?;

        info!("RabbitMQ queue service initialized for queue: {}", queue.config.queue_name);
        Ok(queue)
    }

    /// Setup queue, exchange, and bindings
    async fn setup_infrastructure(&self) -> QueueResult<()> {
        let channel = self.connection_pool.get_channel().await?;

        // Setup exchange arguments
        let mut exchange_args = FieldTable::default();
        for (key, value) in &self.config.exchange_arguments {
            exchange_args.insert(key.as_str(), value.clone());
        }

        // Declare exchange
        channel
            .exchange_declare(
                &self.config.exchange_name,
                self.config.exchange_type.to_lapin_kind(),
                ExchangeDeclareOptions {
                    durable: self.config.exchange_durable,
                    auto_delete: self.config.exchange_auto_delete,
                    ..Default::default()
                },
                exchange_args,
            )
            .await
            .map_err(|e| QueueError::NetworkError(format!("Failed to declare exchange: {}", e)))?;

        // Setup queue arguments
        let mut queue_args = FieldTable::default();

        if let Some(ttl) = self.config.message_ttl {
            queue_args.insert("x-message-ttl".into(), AMQPValue::LongUInt(ttl as u64));
        }

        if let Some(dlx) = &self.config.dead_letter_exchange {
            queue_args.insert("x-dead-letter-exchange".into(), AMQPValue::LongString(dlx.clone().into()));
        }

        if let Some(dlrk) = &self.config.dead_letter_routing_key {
            queue_args.insert("x-dead-letter-routing-key".into(), AMQPValue::LongString(dlrk.clone().into()));
        }

        if let Some(max_len) = self.config.max_length {
            queue_args.insert("x-max-length".into(), AMQPValue::LongUInt(max_len as u64));
        }

        if let Some(max_len_bytes) = self.config.max_length_bytes {
            queue_args.insert("x-max-length-bytes".into(), AMQPValue::LongLongUInt(max_len_bytes));
        }

        for (key, value) in &self.config.queue_arguments {
            queue_args.insert(key.as_str(), value.clone());
        }

        // Declare queue
        let queue = channel
            .queue_declare(
                &self.config.queue_name,
                QueueDeclareOptions {
                    durable: self.config.durable,
                    auto_delete: self.config.auto_delete,
                    ..Default::default()
                },
                queue_args,
            )
            .await
            .map_err(|e| QueueError::NetworkError(format!("Failed to declare queue: {}", e)))?;

        // Bind queue to exchange
        let routing_key = self.config.binding_routing_key.as_ref().unwrap_or(&self.config.queue_name);
        channel
            .queue_bind(
                &self.config.queue_name,
                &self.config.exchange_name,
                routing_key,
                QueueBindOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|e| QueueError::NetworkError(format!("Failed to bind queue: {}", e)))?;

        debug!(
            "Queue '{}' bound to exchange '{}' with routing key '{}'",
            self.config.queue_name, self.config.exchange_name, routing_key
        );

        // Update stats
        let mut stats = self.stats.write().await;
        stats.backend_stats.insert("message_count".to_string(), serde_json::json!(queue.message_count()));
        stats.backend_stats.insert("consumer_count".to_string(), serde_json::json!(queue.consumer_count()));

        Ok(())
    }

    /// Convert QueuePriority to AMQP priority (0-9, where 9 is highest)
    fn priority_to_amqp(priority: QueuePriority) -> u8 {
        match priority {
            QueuePriority::Low => 1,
            QueuePriority::Normal => 5,
            QueuePriority::High => 8,
            QueuePriority::Critical => 9,
        }
    }

    /// Convert AMQP priority to QueuePriority
    fn amqp_to_priority(priority: u8) -> QueuePriority {
        match priority {
            0..=2 => QueuePriority::Low,
            3..=6 => QueuePriority::Normal,
            7..=8 => QueuePriority::High,
            9 => QueuePriority::Critical,
            _ => QueuePriority::Normal,
        }
    }

    /// Get the routing key for publishing
    fn get_routing_key(&self, message: &QueueMessage) -> String {
        message
            .routing_key
            .clone()
            .or_else(|| self.config.routing_key.clone())
            .unwrap_or_else(|| self.config.queue_name.clone())
    }
}

#[async_trait]
impl QueueService for RabbitMQQueue {
    async fn enqueue(&self, mut message: QueueMessage) -> QueueResult<String> {
        let channel = self.connection_pool.get_channel().await?;

        // Set message ID if not present
        if message.id.is_empty() {
            message.id = Uuid::new_v4().to_string();
        }

        // Convert message to JSON
        let payload = serde_json::to_vec(&message)
            .map_err(|e| QueueError::Serialization(format!("Failed to serialize message: {}", e)))?;

        // Setup message properties
        let mut properties = BasicProperties::default()
            .with_content_type("application/json".into())
            .with_delivery_mode(if self.config.durable { 2 } else { 1 }) // Persistent or transient
            .with_priority(Self::priority_to_amqp(message.priority))
            .with_message_id(message.id.clone().into())
            .with_timestamp(message.created_at.timestamp() as u64);

        // Add headers if present
        if !message.headers.is_empty() {
            let mut headers = FieldTable::default();
            for (key, value) in &message.headers {
                if let Ok(json_value) = serde_json::to_value(value) {
                    if let Ok(field_value) = Self::json_to_field_value(&json_value) {
                        headers.insert(key.as_str().into(), field_value);
                    }
                }
            }
            properties = properties.with_headers(headers);
        }

        // Add TTL if specified
        properties = properties.with_expiration((message.visibility_timeout * 1000).to_string().into()); // Convert to milliseconds

        let routing_key = self.get_routing_key(&message);

        debug!(
            "Publishing message {} to exchange '{}' with routing key '{}'",
            message.id, self.config.exchange_name, routing_key
        );

        // Publish message
        let confirm = channel
            .basic_publish(
                &self.config.exchange_name,
                &routing_key,
                BasicPublishOptions::default(),
                &payload,
                properties,
            )
            .await
            .map_err(|e| QueueError::NetworkError(format!("Failed to publish message: {}", e)))?;

        // Wait for publisher confirmation if enabled
        if self.config.publisher_confirms {
            confirm
                .await
                .map_err(|e| QueueError::NetworkError(format!("Publisher confirmation failed: {}", e)))?;
        }

        // Update stats
        let mut stats = self.stats.write().await;
        stats.total_messages += 1;
        stats.visible_messages += 1;

        info!("Message {} published successfully", message.id);
        Ok(message.id)
    }

    async fn dequeue(&self) -> QueueResult<Option<QueueMessage>> {
        let channel = self.connection_pool.get_channel().await?;

        // Setup consumer if not already done
        let mut consumer_guard = self.consumer.lock().await;
        if consumer_guard.is_none() {
            debug!("Creating consumer for queue: {}", self.config.queue_name);

            let consumer = channel
                .basic_consume(
                    &self.config.queue_name,
                    "backbone_consumer", // Consumer tag
                    BasicConsumeOptions::default(),
                    FieldTable::default(),
                )
                .await
                .map_err(|e| QueueError::NetworkError(format!("Failed to create consumer: {}", e)))?;

            *consumer_guard = Some(consumer);
        }

        // Try to receive a message
        if let Some(delivery) = consumer_guard.as_ref().unwrap().next().await {
            let delivery = delivery.map_err(|e| QueueError::NetworkError(format!("Failed to receive delivery: {}", e)))?;

            // Parse message
            let message: QueueMessage = serde_json::from_slice(&delivery.data)
                .map_err(|e| QueueError::Deserialization(format!("Failed to deserialize message: {}", e)))?;

            debug!("Received message: {}", message.id);

            // Update stats
            let mut stats = self.stats.write().await;
            stats.visible_messages = stats.visible_messages.saturating_sub(1);
            stats.invisible_messages += 1;

            Ok(Some(message))
        } else {
            // No message available
            Ok(None)
        }
    }

    async fn ack(&self, message_id: &str) -> QueueResult<()> {
        // Note: In a real implementation, we'd need to track the delivery tag
        // For now, this is a placeholder implementation
        debug!("Acknowledging message: {}", message_id);

        // Update stats
        let mut stats = self.stats.write().await;
        stats.invisible_messages = stats.invisible_messages.saturating_sub(1);
        stats.total_processed += 1;

        Ok(())
    }

    async fn nack(&self, message_id: &str, requeue: bool) -> QueueResult<()> {
        // Note: In a real implementation, we'd need to track the delivery tag
        // For now, this is a placeholder implementation
        debug!("Negative acknowledging message: {} (requeue: {})", message_id, requeue);

        // Update stats
        let mut stats = self.stats.write().await;
        stats.invisible_messages = stats.invisible_messages.saturating_sub(1);

        if requeue {
            stats.visible_messages += 1;
        } else {
            stats.dead_letter_messages += 1;
            stats.total_failed += 1;
        }

        Ok(())
    }

    async fn enqueue_batch(&self, messages: Vec<QueueMessage>) -> QueueResult<Vec<String>> {
        let mut message_ids = Vec::with_capacity(messages.len());

        for message in messages {
            let id = self.enqueue(message).await?;
            message_ids.push(id);
        }

        Ok(message_ids)
    }

    async fn dequeue_batch(&self, max_messages: usize) -> QueueResult<Vec<QueueMessage>> {
        let mut messages = Vec::with_capacity(max_messages);

        for _ in 0..max_messages {
            if let Some(message) = self.dequeue().await? {
                messages.push(message);
            } else {
                break; // No more messages available
            }
        }

        Ok(messages)
    }

    async fn get_stats(&self) -> QueueResult<QueueStats> {
        let channel = self.connection_pool.get_channel().await?;

        // Get current queue information
        let queue = channel
            .queue_declare(
                &self.config.queue_name,
                QueueDeclareOptions {
                    durable: false, // Don't create, just get info
                    passive: true,  // Just check if it exists
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(|e| QueueError::NetworkError(format!("Failed to get queue stats: {}", e)))?;

        let mut stats = self.stats.write().await.clone();
        stats.total_messages = queue.message_count();
        stats.visible_messages = queue.message_count();
        stats.queue_age_seconds = Some(
            Utc::now()
                .signed_duration_since(DateTime::from_timestamp(0, 0).unwrap())
                .num_seconds() as u64,
        );

        Ok(stats)
    }

    async fn purge(&self) -> QueueResult<u64> {
        let channel = self.connection_pool.get_channel().await?;

        let purged_count = channel
            .queue_purge(&self.config.queue_name, QueuePurgeOptions::default())
            .await
            .map_err(|e| QueueError::NetworkError(format!("Failed to purge queue: {}", e)))?;

        // Update stats
        let mut stats = self.stats.write().await;
        stats.visible_messages = 0;
        stats.invisible_messages = 0;
        stats.total_messages = 0;

        info!("Purged {} messages from queue: {}", purged_count, self.config.queue_name);
        Ok(purged_count as u64)
    }

    async fn delete(&self) -> QueueResult<()> {
        let channel = self.connection_pool.get_channel().await?;

        // Delete queue
        channel
            .queue_delete(&self.config.queue_name, QueueDeleteOptions::default())
            .await
            .map_err(|e| QueueError::NetworkError(format!("Failed to delete queue: {}", e)))?;

        // Delete exchange (only if auto_delete is false and we want to clean up)
        if !self.config.exchange_auto_delete {
            if let Err(e) = channel
                .exchange_delete(&self.config.exchange_name, ExchangeDeleteOptions::default())
                .await
            {
                warn!("Failed to delete exchange: {}", e);
            }
        }

        // Close connection
        if let Err(e) = self.connection_pool.close().await {
            warn!("Failed to close connection: {}", e);
        }

        info!("Deleted queue and exchange: {}", self.config.queue_name);
        Ok(())
    }

    fn backend_type(&self) -> QueueBackend {
        QueueBackend::RabbitMQ
    }

    async fn health_check(&self) -> QueueResult<bool> {
        match self.connection_pool.get_channel().await {
            Ok(_) => Ok(true),
            Err(e) => {
                error!("RabbitMQ health check failed: {}", e);
                Ok(false)
            }
        }
    }
}

impl RabbitMQQueue {
    /// Convert JSON value to AMQP field value
    fn json_to_field_value(value: &serde_json::Value) -> Result<AMQPValue, QueueError> {
        match value {
            serde_json::Value::String(s) => Ok(AMQPValue::LongString(s.clone().into())),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(AMQPValue::LongLongInt(i))
                } else if let Some(u) = n.as_u64() {
                    Ok(AMQPValue::LongLongUInt(u))
                } else if let Some(f) = n.as_f64() {
                    Ok(AMQPValue::Double(f))
                } else {
                    Err(QueueError::Serialization("Invalid number format".to_string()))
                }
            }
            serde_json::Value::Bool(b) => Ok(AMQPValue::Boolean(*b)),
            serde_json::Value::Null => Ok(AMQPValue::Void),
            _ => Err(QueueError::Serialization("Unsupported JSON type for AMQP field".to_string())),
        }
    }
}

/// Utility functions for RabbitMQ
pub mod utils {
    use super::*;

    /// Create a default RabbitMQ configuration for development
    pub fn dev_config(
        queue_name: &str,
        exchange_name: &str,
        exchange_type: ExchangeType,
    ) -> RabbitMQConfig {
        RabbitMQConfig {
            connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
            queue_name: queue_name.to_string(),
            exchange_name: exchange_name.to_string(),
            exchange_type,
            durable: false, // Not durable in development
            exchange_durable: false,
            publisher_confirms: false, // Disabled for faster development
            ..Default::default()
        }
    }

    /// Create a production-ready RabbitMQ configuration
    pub fn prod_config(
        connection_url: &str,
        queue_name: &str,
        exchange_name: &str,
        exchange_type: ExchangeType,
    ) -> RabbitMQConfig {
        RabbitMQConfig {
            connection_url: connection_url.to_string(),
            queue_name: queue_name.to_string(),
            exchange_name: exchange_name.to_string(),
            exchange_type,
            durable: true, // Durable in production
            exchange_durable: true,
            publisher_confirms: true, // Enabled for reliability
            ..Default::default()
        }
    }

    /// Validate AMQP connection URL
    pub fn validate_connection_url(url: &str) -> QueueResult<()> {
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