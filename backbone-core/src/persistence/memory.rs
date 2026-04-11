//! In-Memory Repository Implementation
//!
//! Thread-safe in-memory storage for entities. Useful for:
//! - Unit testing
//! - Prototyping
//! - Development without database
//! - Entities that don't need persistence

use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::RwLock;

use super::traits::{CrudRepository, PersistentEntity, RepositoryError, SearchableRepository};

/// Generic in-memory repository
///
/// Provides thread-safe storage for any entity implementing `PersistentEntity`.
/// All data is stored in memory and lost when the application restarts.
///
/// # Example
///
/// ```ignore
/// use backbone_core::persistence::{InMemoryRepository, PersistentEntity};
///
/// #[derive(Clone, Debug, Serialize, Deserialize)]
/// struct User {
///     id: String,
///     name: String,
///     created_at: Option<DateTime<Utc>>,
///     updated_at: Option<DateTime<Utc>>,
///     deleted_at: Option<DateTime<Utc>>,
/// }
///
/// impl PersistentEntity for User { /* ... */ }
///
/// let repo = InMemoryRepository::<User>::new();
/// ```
pub struct InMemoryRepository<E>
where
    E: PersistentEntity,
{
    /// Active entities (not soft-deleted)
    store: RwLock<HashMap<String, E>>,
    /// Soft-deleted entities (trash)
    trash: RwLock<HashMap<String, E>>,
}

impl<E> InMemoryRepository<E>
where
    E: PersistentEntity,
{
    /// Create a new empty repository
    pub fn new() -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
            trash: RwLock::new(HashMap::new()),
        }
    }

    /// Create a repository with initial data
    pub fn with_data(entities: Vec<E>) -> Self {
        let store: HashMap<String, E> = entities
            .into_iter()
            .map(|e| (e.entity_id(), e))
            .collect();

        Self {
            store: RwLock::new(store),
            trash: RwLock::new(HashMap::new()),
        }
    }

    /// Get the number of active entities
    pub fn len(&self) -> usize {
        self.store.read().unwrap().len()
    }

    /// Check if store is empty
    pub fn is_empty(&self) -> bool {
        self.store.read().unwrap().is_empty()
    }

    /// Get the number of items in trash
    pub fn trash_len(&self) -> usize {
        self.trash.read().unwrap().len()
    }

    /// Clear all data (both active and trash)
    pub fn clear(&self) {
        self.store.write().unwrap().clear();
        self.trash.write().unwrap().clear();
    }
}

impl<E> Default for InMemoryRepository<E>
where
    E: PersistentEntity,
{
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<E> CrudRepository<E> for InMemoryRepository<E>
where
    E: PersistentEntity,
{
    async fn create(&self, mut entity: E) -> Result<E, RepositoryError> {
        let mut store = self.store.write().unwrap();

        // Generate ID if empty
        if entity.entity_id().is_empty() {
            entity.set_entity_id(E::generate_id());
        }

        // Check for duplicates
        if store.contains_key(&entity.entity_id()) {
            return Err(RepositoryError::AlreadyExists(format!(
                "Entity with ID {} already exists",
                entity.entity_id()
            )));
        }

        // Set timestamps
        let now = Utc::now();
        if entity.created_at().is_none() {
            entity.set_created_at(now);
        }
        entity.set_updated_at(now);

        let id = entity.entity_id();
        store.insert(id, entity.clone());
        Ok(entity)
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<E>, RepositoryError> {
        let store = self.store.read().unwrap();
        Ok(store.get(id).cloned())
    }

    async fn find_by_id_including_deleted(&self, id: &str) -> Result<Option<E>, RepositoryError> {
        // Check active store first
        {
            let store = self.store.read().unwrap();
            if let Some(entity) = store.get(id) {
                return Ok(Some(entity.clone()));
            }
        }

        // Check trash
        let trash = self.trash.read().unwrap();
        Ok(trash.get(id).cloned())
    }

    async fn update(&self, mut entity: E) -> Result<E, RepositoryError> {
        let mut store = self.store.write().unwrap();

        if !store.contains_key(&entity.entity_id()) {
            return Err(RepositoryError::NotFound);
        }

        entity.set_updated_at(Utc::now());
        let id = entity.entity_id();
        store.insert(id, entity.clone());
        Ok(entity)
    }

    async fn soft_delete(&self, id: &str) -> Result<bool, RepositoryError> {
        let mut store = self.store.write().unwrap();
        let mut trash = self.trash.write().unwrap();

        if let Some(mut entity) = store.remove(id) {
            entity.mark_deleted();
            trash.insert(id.to_string(), entity);
            return Ok(true);
        }

        Ok(false)
    }

    async fn restore(&self, id: &str) -> Result<Option<E>, RepositoryError> {
        let mut store = self.store.write().unwrap();
        let mut trash = self.trash.write().unwrap();

        if let Some(mut entity) = trash.remove(id) {
            entity.restore();
            store.insert(id.to_string(), entity.clone());
            return Ok(Some(entity));
        }

        Ok(None)
    }

    async fn hard_delete(&self, id: &str) -> Result<bool, RepositoryError> {
        let mut store = self.store.write().unwrap();
        let mut trash = self.trash.write().unwrap();

        // Try to remove from both stores
        let removed_active = store.remove(id).is_some();
        let removed_trash = trash.remove(id).is_some();

        Ok(removed_active || removed_trash)
    }

    async fn list(&self, page: u32, limit: u32) -> Result<(Vec<E>, u64), RepositoryError> {
        let store = self.store.read().unwrap();
        let total = store.len() as u64;

        let offset = ((page.saturating_sub(1)) * limit) as usize;
        let entities: Vec<E> = store
            .values()
            .skip(offset)
            .take(limit as usize)
            .cloned()
            .collect();

        Ok((entities, total))
    }

    async fn list_deleted(&self, page: u32, limit: u32) -> Result<(Vec<E>, u64), RepositoryError> {
        let trash = self.trash.read().unwrap();
        let total = trash.len() as u64;

        let offset = ((page.saturating_sub(1)) * limit) as usize;
        let entities: Vec<E> = trash
            .values()
            .skip(offset)
            .take(limit as usize)
            .cloned()
            .collect();

        Ok((entities, total))
    }

    async fn count(&self) -> Result<u64, RepositoryError> {
        Ok(self.store.read().unwrap().len() as u64)
    }

    async fn count_deleted(&self) -> Result<u64, RepositoryError> {
        Ok(self.trash.read().unwrap().len() as u64)
    }

    async fn bulk_create(&self, entities: Vec<E>) -> Result<Vec<E>, RepositoryError> {
        let mut results = Vec::with_capacity(entities.len());
        for entity in entities {
            results.push(self.create(entity).await?);
        }
        Ok(results)
    }

    async fn empty_trash(&self) -> Result<u64, RepositoryError> {
        let mut trash = self.trash.write().unwrap();
        let count = trash.len() as u64;
        trash.clear();
        Ok(count)
    }
}

#[async_trait]
impl<E> SearchableRepository<E> for InMemoryRepository<E>
where
    E: PersistentEntity,
{
    async fn search(
        &self,
        filters: HashMap<String, String>,
        page: u32,
        limit: u32,
    ) -> Result<(Vec<E>, u64), RepositoryError> {
        // For in-memory, we do basic JSON field matching
        let store = self.store.read().unwrap();

        let filtered: Vec<E> = store
            .values()
            .filter(|entity| {
                // Serialize entity to JSON for field matching
                if let Ok(json) = serde_json::to_value(entity) {
                    filters.iter().all(|(key, value)| {
                        if let Some(field_value) = json.get(key) {
                            match field_value {
                                serde_json::Value::String(s) => {
                                    s.to_lowercase().contains(&value.to_lowercase())
                                }
                                serde_json::Value::Number(n) => n.to_string() == *value,
                                serde_json::Value::Bool(b) => b.to_string() == *value,
                                _ => false,
                            }
                        } else {
                            false
                        }
                    })
                } else {
                    false
                }
            })
            .cloned()
            .collect();

        let total = filtered.len() as u64;
        let offset = ((page.saturating_sub(1)) * limit) as usize;
        let paginated: Vec<E> = filtered
            .into_iter()
            .skip(offset)
            .take(limit as usize)
            .collect();

        Ok((paginated, total))
    }

    async fn find_by_field(&self, field: &str, value: &str) -> Result<Option<E>, RepositoryError> {
        let store = self.store.read().unwrap();

        for entity in store.values() {
            if let Ok(json) = serde_json::to_value(entity) {
                if let Some(field_value) = json.get(field) {
                    let matches = match field_value {
                        serde_json::Value::String(s) => s == value,
                        serde_json::Value::Number(n) => n.to_string() == value,
                        serde_json::Value::Bool(b) => b.to_string() == value,
                        _ => false,
                    };
                    if matches {
                        return Ok(Some(entity.clone()));
                    }
                }
            }
        }

        Ok(None)
    }

    async fn find_all_by_field(
        &self,
        field: &str,
        value: &str,
        page: u32,
        limit: u32,
    ) -> Result<(Vec<E>, u64), RepositoryError> {
        let mut filters = HashMap::new();
        filters.insert(field.to_string(), value.to_string());
        self.search(filters, page, limit).await
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    struct TestEntity {
        id: String,
        name: String,
        value: i32,
        created_at: Option<chrono::DateTime<Utc>>,
        updated_at: Option<chrono::DateTime<Utc>>,
        deleted_at: Option<chrono::DateTime<Utc>>,
    }

    impl TestEntity {
        fn new(name: &str, value: i32) -> Self {
            Self {
                id: String::new(),
                name: name.to_string(),
                value,
                created_at: None,
                updated_at: None,
                deleted_at: None,
            }
        }
    }

    impl PersistentEntity for TestEntity {
        fn entity_id(&self) -> String {
            self.id.clone()
        }

        fn set_entity_id(&mut self, id: String) {
            self.id = id;
        }

        fn created_at(&self) -> Option<chrono::DateTime<Utc>> {
            self.created_at
        }

        fn set_created_at(&mut self, ts: chrono::DateTime<Utc>) {
            self.created_at = Some(ts);
        }

        fn updated_at(&self) -> Option<chrono::DateTime<Utc>> {
            self.updated_at
        }

        fn set_updated_at(&mut self, ts: chrono::DateTime<Utc>) {
            self.updated_at = Some(ts);
        }

        fn deleted_at(&self) -> Option<chrono::DateTime<Utc>> {
            self.deleted_at
        }

        fn set_deleted_at(&mut self, ts: Option<chrono::DateTime<Utc>>) {
            self.deleted_at = ts;
        }
    }

    #[tokio::test]
    async fn test_create_and_find() {
        let repo = InMemoryRepository::<TestEntity>::new();
        let entity = TestEntity::new("test", 42);

        let created = repo.create(entity).await.unwrap();
        assert!(!created.id.is_empty());
        assert_eq!(created.name, "test");
        assert!(created.created_at.is_some());

        let found = repo.find_by_id(&created.id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "test");
    }

    #[tokio::test]
    async fn test_soft_delete_and_restore() {
        let repo = InMemoryRepository::<TestEntity>::new();
        let entity = TestEntity::new("test", 42);

        let created = repo.create(entity).await.unwrap();
        let id = created.id.clone();

        // Soft delete
        assert!(repo.soft_delete(&id).await.unwrap());
        assert!(repo.find_by_id(&id).await.unwrap().is_none());
        assert!(repo.find_by_id_including_deleted(&id).await.unwrap().is_some());

        // Restore
        let restored = repo.restore(&id).await.unwrap();
        assert!(restored.is_some());
        assert!(repo.find_by_id(&id).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_bulk_create() {
        let repo = InMemoryRepository::<TestEntity>::new();
        let entities = vec![
            TestEntity::new("one", 1),
            TestEntity::new("two", 2),
            TestEntity::new("three", 3),
        ];

        let created = repo.bulk_create(entities).await.unwrap();
        assert_eq!(created.len(), 3);
        assert_eq!(repo.count().await.unwrap(), 3);
    }

    #[tokio::test]
    async fn test_pagination() {
        let repo = InMemoryRepository::<TestEntity>::new();

        // Create 10 entities
        for i in 0..10 {
            repo.create(TestEntity::new(&format!("entity_{}", i), i))
                .await
                .unwrap();
        }

        // Test pagination
        let (page1, total) = repo.list(1, 3).await.unwrap();
        assert_eq!(total, 10);
        assert_eq!(page1.len(), 3);

        let (page2, _) = repo.list(2, 3).await.unwrap();
        assert_eq!(page2.len(), 3);
    }

    #[tokio::test]
    async fn test_search() {
        let repo = InMemoryRepository::<TestEntity>::new();

        repo.create(TestEntity::new("apple", 1)).await.unwrap();
        repo.create(TestEntity::new("banana", 2)).await.unwrap();
        repo.create(TestEntity::new("apple pie", 3)).await.unwrap();

        let mut filters = HashMap::new();
        filters.insert("name".to_string(), "apple".to_string());

        let (results, total) = repo.search(filters, 1, 10).await.unwrap();
        assert_eq!(total, 2);
        assert_eq!(results.len(), 2);
    }
}
