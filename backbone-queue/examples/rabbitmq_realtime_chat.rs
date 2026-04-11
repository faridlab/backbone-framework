//! RabbitMQ Real-Time Chat Example
//!
//! This example demonstrates a real-time chat system using RabbitMQ
//! Features shown:
//! - Real-time message broadcasting
//! - Chat rooms and user management
//! - Message persistence
//! - Online user tracking
//! - Message history retrieval
//!
//! Run with: cargo run --example rabbitmq_realtime_chat

use backbone_queue::{
    rabbitmq_simple::{RabbitMQQueueSimple, RabbitMQConfig, ExchangeType},
    traits::QueueService,
    types::{QueueMessage, QueuePriority},
    utils::rabbitmq_simple::*,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMessage {
    id: String,
    room_id: String,
    user_id: String,
    username: String,
    content: String,
    timestamp: String,
    message_type: String, // "text", "join", "leave", "system"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatRoom {
    id: String,
    name: String,
    description: String,
    created_at: String,
    member_count: u32,
    max_members: u32,
    is_private: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserStatus {
    user_id: String,
    username: String,
    status: String, // "online", "offline", "away"
    last_seen: String,
    current_room: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    println!("💬 RabbitMQ Real-Time Chat System");
    println!("=================================");

    // Setup chat infrastructure
    setup_chat_infrastructure().await?;

    // Example 1: Create and join chat room
    println!("🏠 Example 1: Chat Room Management");
    chat_room_management().await?;
    println!();

    // Example 2: Real-time messaging
    println!("⚡ Example 2: Real-Time Messaging");
    real_time_messaging().await?;
    println!();

    // Example 3: User presence tracking
    println!("👥 Example 3: User Presence Tracking");
    user_presence_tracking().await?;
    println!();

    // Example 4: Message history and search
    println!("📚 Example 4: Message History & Search");
    message_history().await?;
    println!();

    // Example 5: Private messaging
    println!("🔒 Example 5: Private Messaging");
    private_messaging().await?;
    println!();

    // Example 6: System notifications
    println!("📢 Example 6: System Notifications");
    system_notifications().await?;
    println!();

    println!("✅ Chat system examples completed!");
    Ok(())
}

async fn setup_chat_infrastructure() -> Result<(), Box<dyn std::error::Error>> {
    println!("   🏗️  Setting up chat infrastructure...");

    // Main chat messages exchange (fanout for broadcasting)
    let messages_queue = RabbitMQQueueSimple::new(
        RabbitMQConfig {
            connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
            queue_name: "chat.messages".to_string(),
            exchange_name: "chat.messages.fanout".to_string(),
            exchange_type: ExchangeType::Fanout,
            routing_key: None,
        }
    ).await?;

    // Room management (topic exchange for room-specific messages)
    let room_queue = RabbitMQQueueSimple::new(
        RabbitMQConfig {
            connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
            queue_name: "chat.rooms".to_string(),
            exchange_name: "chat.rooms.topic".to_string(),
            exchange_type: ExchangeType::Topic,
            routing_key: Some("room.*".to_string()),
        }
    ).await?;

    // User presence tracking
    let presence_queue = RabbitMQQueueSimple::new(
        RabbitMQConfig {
            connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
            queue_name: "chat.presence".to_string(),
            exchange_name: "chat.presence.direct".to_string(),
            exchange_type: ExchangeType::Direct,
            routing_key: Some("presence.update".to_string()),
        }
    ).await?;

    // Private messages (direct exchange for 1-to-1 messaging)
    let private_queue = RabbitMQQueueSimple::new(
        RabbitMQConfig {
            connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
            queue_name: "chat.private".to_string(),
            exchange_name: "chat.private.direct".to_string(),
            exchange_type: ExchangeType::Direct,
            routing_key: Some("private.message".to_string()),
        }
    ).await?;

    // System notifications (high priority)
    let notifications_queue = RabbitMQQueueSimple::new(
        RabbitMQConfig {
            connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
            queue_name: "chat.notifications".to_string(),
            exchange_name: "chat.notifications.direct".to_string(),
            exchange_type: ExchangeType::Direct,
            routing_key: Some("notification.urgent".to_string()),
        }
    ).await?;

    println!("   ✓ Chat Messages Exchange: chat.messages.fanout");
    println!("   ✓ Room Management Exchange: chat.rooms.topic");
    println!("   ✓ Presence Tracking: chat.presence.direct");
    println!("   ✓ Private Messages: chat.private.direct");
    println!("   ✓ System Notifications: chat.notifications.direct");
    println!("   ✓ Chat infrastructure setup complete!\n");

    Ok(())
}

async fn chat_room_management() -> Result<(), Box<dyn std::error::Error>> {
    println!("   🏠 Managing chat rooms...");

    // Create public chat room
    let public_room = ChatRoom {
        id: "room_general".to_string(),
        name: "General Discussion".to_string(),
        description: "Open discussion room for all topics".to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        member_count: 0,
        max_members: 100,
        is_private: false,
    };

    let room_queue = RabbitMQQueueSimple::new(
        RabbitMQConfig {
            connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
            queue_name: "room_management".to_string(),
            exchange_name: "chat.rooms.topic".to_string(),
            exchange_type: ExchangeType::Topic,
            routing_key: Some("room.create".to_string()),
        }
    ).await?;

    let room_message = QueueMessage::builder()
        .payload(serde_json::to_value(public_room)?)
        .expect("Failed to serialize room data")
        .priority(QueuePriority::Normal)
        .routing_key("room.create")
        .build();

    let room_id = room_queue.enqueue(room_message).await?;
    println!("   ✓ Created public room: {}", public_room.name);

    // Create private chat room
    let private_room = ChatRoom {
        id: "room_project_alpha".to_string(),
        name: "Project Alpha".to_string(),
        description: "Private room for Project Alpha team members".to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        member_count: 0,
        max_members: 10,
        is_private: true,
    };

    let private_message = QueueMessage::builder()
        .payload(serde_json::to_value(private_room)?)
        .expect("Failed to serialize private room data")
        .priority(QueuePriority::High)
        .routing_key("room.create.private")
        .build();

    let private_id = room_queue.enqueue(private_message).await?;
    println!("   ✓ Created private room: {}", private_room.name);

    // Simulate room join
    simulate_room_join(&room_queue, &public_room.id).await?;
    simulate_room_join(&room_queue, &private_room.id).await?;

    Ok(())
}

async fn simulate_room_join(
    queue: &RabbitMQQueueSimple,
    room_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let join_message = ChatMessage {
        id: format!("join_{}", chrono::Utc::now().timestamp_millis()),
        room_id: room_id.to_string(),
        user_id: "user_12345".to_string(),
        username: "Alice".to_string(),
        content: "has joined the room".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        message_type: "join".to_string(),
    };

    let message = QueueMessage::builder()
        .payload(serde_json::to_value(join_message)?)
        .expect("Failed to serialize join message")
        .priority(QueuePriority::Normal)
        .routing_key(format!("room.{}", room_id))
        .build();

    let message_id = queue.enqueue(message).await?;
    println!("   👤 User joined room {}: {}", room_id, message_id);
    Ok(())
}

async fn real_time_messaging() -> Result<(), Box<dyn std::error::Error>> {
    println!("   ⚡ Demonstrating real-time messaging...");

    let messages_queue = RabbitMQQueueSimple::new(
        RabbitMQConfig {
            connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
            queue_name: "chat.messages.general".to_string(),
            exchange_name: "chat.messages.fanout".to_string(),
            exchange_type: ExchangeType::Fanout,
            routing_key: None,
        }
    ).await?;

    // Simulate multiple users sending messages
    let users = vec![
        ("user_12345", "Alice"),
        ("user_67890", "Bob"),
        ("user_11111", "Charlie"),
    ];

    let messages = vec![
        "Hey everyone! How's it going?",
        "Working on a new feature today 🚀",
        "Has anyone tried the new RabbitMQ implementation?",
        "It's really powerful! Great for microservices 🐰",
        "We're using it for our chat system!",
        "Love the topic exchange for routing!",
    ];

    for (i, ((user_id, username), content)) in users.iter().zip(&messages).enumerate() {
        let chat_message = ChatMessage {
            id: format!("msg_{}", chrono::Utc::now().timestamp_millis() + i as i64),
            room_id: "room_general".to_string(),
            user_id: user_id.to_string(),
            username: username.to_string(),
            content: content.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            message_type: "text".to_string(),
        };

        let message = QueueMessage::builder()
            .payload(serde_json::to_value(chat_message)?)
            .expect("Failed to serialize chat message")
            .priority(QueuePriority::Normal)
            .build();

        let message_id = messages_queue.enqueue(message).await?;
        println!("   💬 {}: {} ({})", username, content, message_id);

        // Small delay between messages
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    println!("   📢 All messages broadcast to connected users");
    Ok(())
}

async fn user_presence_tracking() -> Result<(), Box<dyn std::error::Error>> {
    println!("   👥 Tracking user presence...");

    let presence_queue = RabbitMQQueueSimple::new(
        RabbitMQConfig {
            connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
            queue_name: "presence.tracking".to_string(),
            exchange_name: "chat.presence.direct".to_string(),
            exchange_type: ExchangeType::Direct,
            routing_key: Some("presence.update".to_string()),
        }
    ).await?;

    // User comes online
    let user_online = UserStatus {
        user_id: "user_12345".to_string(),
        username: "Alice".to_string(),
        status: "online".to_string(),
        last_seen: chrono::Utc::now().to_rfc3339(),
        current_room: Some("room_general".to_string()),
    };

    let online_message = QueueMessage::builder()
        .payload(serde_json::to_value(user_online)?)
        .expect("Failed to serialize user status")
        .priority(QueuePriority::High)
        .routing_key("presence.update")
        .build();

    let online_id = presence_queue.enqueue(online_message).await?;
    println!("   👤 User Alice is now online: {}", online_id);

    // Simulate other users coming online
    let other_users = vec![
        ("user_67890", "Bob"),
        ("user_11111", "Charlie"),
        ("user_22222", "David"),
    ];

    for (user_id, username) in other_users {
        let status = UserStatus {
            user_id: user_id.to_string(),
            username: username.to_string(),
            status: "online".to_string(),
            last_seen: chrono::Utc::now().to_rfc3339(),
            current_room: Some("room_general".to_string()),
        };

        let message = QueueMessage::builder()
            .payload(serde_json::to_value(status)?)
            .expect("Failed to serialize user status")
            .priority(QueuePriority::High)
            .routing_key("presence.update")
            .build();

        let message_id = presence_queue.enqueue(message).await?;
        println!("   👤 User {} is now online: {}", username, message_id);
    }

    // User goes away
    tokio::time::sleep(Duration::from_millis(500)).await;

    let user_away = UserStatus {
        user_id: "user_12345".to_string(),
        username: "Alice".to_string(),
        status: "away".to_string(),
        last_seen: chrono::Utc::now().to_rfc3339(),
        current_room: Some("room_general".to_string()),
    };

    let away_message = QueueMessage::builder()
        .payload(serde_json::to_value(user_away)?)
        .expect("Failed to serialize user status")
        .priority(QueuePriority::Normal)
        .routing_key("presence.update")
        .build();

    let away_id = presence_queue.enqueue(away_message).await?;
    println!("   😴 User Alice is now away: {}", away_id);

    println!("   📊 Current online users: 4");

    Ok(())
}

async fn message_history() -> Result<(), Box<dyn std::error::Error>> {
    println!("   📚 Retrieving message history...");

    // In a real implementation, this would query a database
    // For this example, we'll simulate historical messages

    let historical_messages = vec![
        ("msg_001", "2023-10-01T10:00:00Z", "John", "Welcome to the chat!"),
        ("msg_002", "2023-10-01T10:01:15Z", "Jane", "Thanks! Excited to be here"),
        ("msg_003", "2023-10-01T10:02:30Z", "Bob", "What brings everyone here?"),
        ("msg_004", "2023-10-01T10:03:45Z", "John", "Working on a RabbitMQ project"),
        ("msg_005", "2023-10-01T10:05:00Z", "Jane", "That sounds interesting!"),
    ];

    println!("   📜 Message History (Last 20 minutes):");

    for (id, timestamp, username, content) in historical_messages {
        println!("     [{}] {}: {}", timestamp, username, content);
        println!("       Message ID: {}", id);
    }

    // Simulate message search
    println!("\n   🔍 Searching for 'RabbitMQ' in messages...");

    let search_results: Vec<_> = historical_messages
        .iter()
        .filter(|(_, _, _, content)| content.contains("RabbitMQ"))
        .collect();

    println!("   📊 Search Results:");
    if search_results.is_empty() {
        println!("     No messages found containing 'RabbitMQ'");
    } else {
        for (id, timestamp, username, content) in search_results {
            println!("     ✓ [{}] {}: {}", timestamp, username, content);
        }
    }

    Ok(())
}

async fn private_messaging() -> Result<(), Box<dyn std::error::Error>> {
    println!("   🔒 Setting up private messaging...");

    let private_queue = RabbitMQQueueSimple::new(
        RabbitMQConfig {
            connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
            queue_name: "private.messages.alice_bob".to_string(),
            exchange_name: "chat.private.direct".to_string(),
            exchange_type: ExchangeType::Direct,
            routing_key: Some("private.message.bob".to_string()), // Route to Bob's private queue
        }
    ).await?;

    // Alice sends a private message to Bob
    let private_message = ChatMessage {
        id: format!("private_{}", chrono::Utc::now().timestamp_millis()),
        room_id: "private_alice_bob".to_string(),
        user_id: "user_12345".to_string(),
        username: "Alice".to_string(),
        content: "Hey Bob, wanted to ask you about the RabbitMQ implementation...".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        message_type: "private".to_string(),
    };

    let message = QueueMessage::builder()
        .payload(serde_json::to_value(private_message)?)
        .expect("Failed to serialize private message")
        .priority(QueuePriority::Normal)
        .routing_key("private.message.bob")
        .build();

    let message_id = private_queue.enqueue(message).await?;
    println!("   💬 Alice sent private message to Bob: {}", message_id);

    // Bob's reply
    tokio::time::sleep(Duration::from_millis(300)).await;

    let bob_reply = ChatMessage {
        id: format!("private_{}", chrono::Utc::now().timestamp_millis()),
        room_id: "private_alice_bob".to_string(),
        user_id: "user_67890".to_string(),
        username: "Bob".to_string(),
        content: "Hey Alice! Sure, what would you like to know? 🐰".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        message_type: "private".to_string(),
    };

    let reply = QueueMessage::builder()
        .payload(serde_json::to_value(bob_reply)?)
        .expect("Failed to serialize reply")
        .priority(QueuePriority::Normal)
        .routing_key("private.message.alice") // Route back to Alice's queue
        .build();

    let reply_id = private_queue.enqueue(reply).await?;
    println!("   💬 Bob replied to Alice: {}", reply_id);

    println!("   🔒 Private conversation established between Alice and Bob");
    Ok(())
}

async fn system_notifications() -> Result<(), Box<dyn std::error::Error>> {
    println!("   📢 Sending system notifications...");

    let notifications_queue = RabbitMQQueueSimple::new(
        RabbitMQConfig {
            connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
            queue_name: "system.notifications".to_string(),
            exchange_name: "chat.notifications.direct".to_string(),
            exchange_type: ExchangeType::Direct,
            routing_key: Some("notification.urgent".to_string()),
        }
    ).await?;

    // System maintenance notification
    let maintenance_notification = ChatMessage {
        id: format!("sys_{}", chrono::Utc::now().timestamp_millis()),
        room_id: "system".to_string(),
        user_id: "system".to_string(),
        username: "System".to_string(),
        content: "🔧 Scheduled maintenance in 10 minutes. Chat services will be temporarily unavailable.".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        message_type: "system".to_string(),
    };

    let notification = QueueMessage::builder()
        .payload(serde_json::to_value(maintenance_notification)?)
        .expect("Failed to serialize system notification")
        .priority(QueuePriority::Critical)
        .routing_key("notification.urgent")
        .build();

    let notification_id = notifications_queue.enqueue(notification).await?;
    println!("   📢 System notification sent: {}", notification_id);

    // Welcome message for new users
    tokio::time::sleep(Duration::from_millis(200)).await;

    let welcome_notification = ChatMessage {
        id: format!("welcome_{}", chrono::Utc::now().timestamp_millis()),
        room_id: "system".to_string(),
        user_id: "system".to_string(),
        username: "System".to_string(),
        content: "👋 Welcome to the chat! Please read the community guidelines before posting.".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        message_type: "system".to_string(),
    };

    let welcome = QueueMessage::builder()
        .payload(serde_json::to_value(welcome_notification)?)
        .expect("Failed to serialize welcome notification")
        .priority(QueuePriority::Normal)
        .routing_key("notification.info")
        .build();

    let welcome_id = notifications_queue.enqueue(welcome).await?;
    println!("   👋 Welcome notification sent: {}", welcome_id);

    // User alert notification
    tokio::time::sleep(Duration::from_millis(200)).await();

    let alert_notification = ChatMessage {
        id: format!("alert_{}", chrono::Utc::now().timestamp_millis()),
        room_id: "system".to_string(),
        user_id: "moderator".to_string(),
        username: "Moderator".to_string(),
        content: "⚠️ Reminder: Please keep conversations respectful and on-topic.".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        message_type: "system".to_string(),
    };

    let alert = QueueMessage::builder()
        .payload(serde_json::to_value(alert_notification)?)
        .expect("Failed to serialize alert notification")
        .priority(QueuePriority::High)
        .routing_key("notification.warning")
        .build();

    let alert_id = notifications_queue.enqueue(alert).await?;
    println!("   ⚠️ Moderator alert sent: {}", alert_id);

    println!("   📊 All notifications queued successfully");
    Ok(())
}

// Add this to your Cargo.toml dependencies:
// chrono = { version = "0.4", features = ["serde"] }