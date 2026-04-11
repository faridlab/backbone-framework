//! In-Memory Store - Generic Testing and Prototyping Utility
//!
//! Provides a thread-safe, async-compatible in-memory storage implementation
//! for use in testing, prototyping, and development environments.
//!
//! # Features
//!
//! - Generic storage for any `Clone + Send + Sync` type
//! - Soft delete with trash/restore functionality
//! - Pagination support
//! - Thread-safe using `RwLock`
//! - Async-compatible
//!
//! # Example
//!
//! ```rust,ignore
//! use backbone_orm::InMemoryStore;
//!
//! #[derive(Clone)]
//! struct User {
//!     id: String,
//!     name: String,
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let store = InMemoryStore::<User>::new();
//!
//!     // Insert
//!     let user = User { id: "1".to_string(), name: "Alice".to_string() };
//!     store.insert("1".to_string(), user.clone()).await;
//!
//!     // Get
//!     let found = store.get("1").await;
//!     assert!(found.is_some());
//!
//!     // Soft delete
//!     store.soft_delete("1").await;
//!     assert!(store.get("1").await.is_none());
//!
//!     // Restore
//!     store.restore("1").await;
//!     assert!(store.get("1").await.is_some());
//! }
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Generic in-memory store for entities
///
/// This store provides a simple key-value storage with soft delete support.
/// It's designed for testing and prototyping, not for production use.
///
/// # Type Parameters
///
/// - `T`: The entity type to store. Must be `Clone + Send + Sync`.
pub struct InMemoryStore<T: Clone + Send + Sync> {
    /// Active items
    items: RwLock<HashMap<String, T>>,
    /// Soft-deleted items (trash)
    deleted: RwLock<HashMap<String, T>>,
}

impl<T: Clone + Send + Sync> InMemoryStore<T> {
    /// Create a new empty in-memory store
    pub fn new() -> Self {
        Self {
            items: RwLock::new(HashMap::new()),
            deleted: RwLock::new(HashMap::new()),
        }
    }

    /// Create a new store wrapped in Arc for sharing across async tasks
    pub fn new_shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    // ========================================================================
    // Basic CRUD Operations
    // ========================================================================

    /// Insert or update an item
    pub async fn insert(&self, id: String, item: T) {
        self.items.write().await.insert(id, item);
    }

    /// Get an item by ID (returns None if not found or soft-deleted)
    pub async fn get(&self, id: &str) -> Option<T> {
        self.items.read().await.get(id).cloned()
    }

    /// Get a soft-deleted item by ID
    pub async fn get_deleted(&self, id: &str) -> Option<T> {
        self.deleted.read().await.get(id).cloned()
    }

    /// Check if an item exists (excluding soft-deleted)
    pub async fn exists(&self, id: &str) -> bool {
        self.items.read().await.contains_key(id)
    }

    /// Check if an item exists in trash
    pub async fn exists_in_trash(&self, id: &str) -> bool {
        self.deleted.read().await.contains_key(id)
    }

    /// Update an existing item (returns None if not found)
    pub async fn update(&self, id: &str, item: T) -> Option<T> {
        let mut items = self.items.write().await;
        if items.contains_key(id) {
            items.insert(id.to_string(), item.clone());
            Some(item)
        } else {
            None
        }
    }

    /// Remove an item permanently (hard delete)
    pub async fn remove(&self, id: &str) -> Option<T> {
        self.items.write().await.remove(id)
    }

    // ========================================================================
    // Soft Delete Operations
    // ========================================================================

    /// Soft delete an item (move to trash)
    pub async fn soft_delete(&self, id: &str) -> bool {
        let mut items = self.items.write().await;
        let mut deleted = self.deleted.write().await;
        if let Some(item) = items.remove(id) {
            deleted.insert(id.to_string(), item);
            true
        } else {
            false
        }
    }

    /// Restore a soft-deleted item from trash
    pub async fn restore(&self, id: &str) -> Option<T> {
        let mut items = self.items.write().await;
        let mut deleted = self.deleted.write().await;
        if let Some(item) = deleted.remove(id) {
            items.insert(id.to_string(), item.clone());
            Some(item)
        } else {
            None
        }
    }

    /// Permanently delete an item from trash
    pub async fn hard_delete(&self, id: &str) -> Option<T> {
        self.deleted.write().await.remove(id)
    }

    /// Empty the trash (permanently delete all soft-deleted items)
    pub async fn empty_trash(&self) -> u64 {
        let mut deleted = self.deleted.write().await;
        let count = deleted.len() as u64;
        deleted.clear();
        count
    }

    // ========================================================================
    // List Operations
    // ========================================================================

    /// List items with pagination
    ///
    /// # Arguments
    ///
    /// - `page`: Page number (1-indexed)
    /// - `limit`: Items per page
    ///
    /// # Returns
    ///
    /// Tuple of (items, total_count)
    pub async fn list(&self, page: u32, limit: u32) -> (Vec<T>, u64) {
        let items = self.items.read().await;
        let total = items.len() as u64;
        let skip = ((page.saturating_sub(1)) * limit) as usize;
        let items: Vec<T> = items.values().skip(skip).take(limit as usize).cloned().collect();
        (items, total)
    }

    /// List soft-deleted items with pagination
    pub async fn list_deleted(&self, page: u32, limit: u32) -> (Vec<T>, u64) {
        let deleted = self.deleted.read().await;
        let total = deleted.len() as u64;
        let skip = ((page.saturating_sub(1)) * limit) as usize;
        let items: Vec<T> = deleted.values().skip(skip).take(limit as usize).cloned().collect();
        (items, total)
    }

    /// Get all items (no pagination)
    pub async fn all(&self) -> Vec<T> {
        self.items.read().await.values().cloned().collect()
    }

    /// Get all soft-deleted items
    pub async fn all_deleted(&self) -> Vec<T> {
        self.deleted.read().await.values().cloned().collect()
    }

    // ========================================================================
    // Utility Operations
    // ========================================================================

    /// Get the count of active items
    pub async fn count(&self) -> u64 {
        self.items.read().await.len() as u64
    }

    /// Get the count of soft-deleted items
    pub async fn trash_count(&self) -> u64 {
        self.deleted.read().await.len() as u64
    }

    /// Clear all items (both active and deleted)
    pub async fn clear(&self) {
        self.items.write().await.clear();
        self.deleted.write().await.clear();
    }

    /// Clear only active items
    pub async fn clear_active(&self) {
        self.items.write().await.clear();
    }

    /// Get all IDs of active items
    pub async fn ids(&self) -> Vec<String> {
        self.items.read().await.keys().cloned().collect()
    }

    /// Get all IDs of soft-deleted items
    pub async fn trash_ids(&self) -> Vec<String> {
        self.deleted.read().await.keys().cloned().collect()
    }
}

impl<T: Clone + Send + Sync> Default for InMemoryStore<T> {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct TestEntity {
        id: String,
        name: String,
    }

    #[tokio::test]
    async fn test_insert_and_get() {
        let store = InMemoryStore::<TestEntity>::new();
        let entity = TestEntity {
            id: "1".to_string(),
            name: "Test".to_string(),
        };

        store.insert("1".to_string(), entity.clone()).await;

        let found = store.get("1").await;
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Test");
    }

    #[tokio::test]
    async fn test_update() {
        let store = InMemoryStore::<TestEntity>::new();
        let entity = TestEntity {
            id: "1".to_string(),
            name: "Original".to_string(),
        };

        store.insert("1".to_string(), entity).await;

        let updated = TestEntity {
            id: "1".to_string(),
            name: "Updated".to_string(),
        };

        let result = store.update("1", updated).await;
        assert!(result.is_some());

        let found = store.get("1").await;
        assert_eq!(found.unwrap().name, "Updated");
    }

    #[tokio::test]
    async fn test_soft_delete_and_restore() {
        let store = InMemoryStore::<TestEntity>::new();
        let entity = TestEntity {
            id: "1".to_string(),
            name: "Test".to_string(),
        };

        store.insert("1".to_string(), entity).await;

        // Soft delete
        let deleted = store.soft_delete("1").await;
        assert!(deleted);
        assert!(store.get("1").await.is_none());
        assert!(store.get_deleted("1").await.is_some());

        // Restore
        let restored = store.restore("1").await;
        assert!(restored.is_some());
        assert!(store.get("1").await.is_some());
        assert!(store.get_deleted("1").await.is_none());
    }

    #[tokio::test]
    async fn test_list_pagination() {
        let store = InMemoryStore::<TestEntity>::new();

        for i in 0..10 {
            let entity = TestEntity {
                id: i.to_string(),
                name: format!("Entity {}", i),
            };
            store.insert(i.to_string(), entity).await;
        }

        let (items, total) = store.list(1, 3).await;
        assert_eq!(items.len(), 3);
        assert_eq!(total, 10);

        let (items, total) = store.list(4, 3).await;
        assert_eq!(items.len(), 1);
        assert_eq!(total, 10);
    }

    #[tokio::test]
    async fn test_empty_trash() {
        let store = InMemoryStore::<TestEntity>::new();

        for i in 0..5 {
            let entity = TestEntity {
                id: i.to_string(),
                name: format!("Entity {}", i),
            };
            store.insert(i.to_string(), entity).await;
            store.soft_delete(&i.to_string()).await;
        }

        assert_eq!(store.trash_count().await, 5);

        let emptied = store.empty_trash().await;
        assert_eq!(emptied, 5);
        assert_eq!(store.trash_count().await, 0);
    }

    #[tokio::test]
    async fn test_exists() {
        let store = InMemoryStore::<TestEntity>::new();
        let entity = TestEntity {
            id: "1".to_string(),
            name: "Test".to_string(),
        };

        store.insert("1".to_string(), entity).await;

        assert!(store.exists("1").await);
        assert!(!store.exists("2").await);

        store.soft_delete("1").await;
        assert!(!store.exists("1").await);
        assert!(store.exists_in_trash("1").await);
    }
}
