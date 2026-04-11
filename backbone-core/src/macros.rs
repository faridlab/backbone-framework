//! Code-generation macros for backbone-schema generated repositories.
//!
//! These macros centralise the boilerplate `CrudRepository<E>` trait impl so
//! the generator can emit a single line instead of ~125 lines per entity.
//!
//! # Usage (in generated repository files)
//!
//! ```rust,ignore
//! // Entities that use JSONB-based soft delete (metadata.deleted_at):
//! backbone_core::impl_crud_repository!(UserRepository, User, soft_delete);
//!
//! // Entities that use hard deletes only:
//! backbone_core::impl_crud_repository!(RoleRepository, Role, no_soft_delete);
//! ```

/// Implement `backbone_core::CrudRepository<E>` for a generated repository struct.
///
/// This macro removes ~125 lines of boilerplate per entity by centralising the
/// `anyhow::Result → RepositoryError` mapping that is otherwise copy-pasted
/// identically for every entity.
///
/// # Method mapping — `soft_delete` variant
///
/// | CrudRepository method         | Inherent struct method called       |
/// |-------------------------------|-------------------------------------|
/// | `create`                      | `create(&entity)`                   |
/// | `find_by_id`                  | `find_by_id(id)`                    |
/// | `find_by_id_including_deleted`| `find_deleted_by_id(id)`            |
/// | `update`                      | `update(&id, &entity)`              |
/// | `soft_delete`                 | `soft_delete(id)`                   |
/// | `restore`                     | `restore(id)`                       |
/// | `hard_delete`                 | `permanent_delete(id)`              |
/// | `list`                        | `list_paginated(pagination)`        |
/// | `list_deleted`                | `list_deleted(pagination)`          |
/// | `count`                       | `count_active()`                    |
/// | `count_deleted`               | `count_deleted()`                   |
/// | `bulk_create`                 | `create(&entity)` (loop)            |
/// | `empty_trash`                 | `empty_trash()`                     |
///
/// # Method mapping — `no_soft_delete` variant
///
/// | CrudRepository method         | Inherent struct method called       |
/// |-------------------------------|-------------------------------------|
/// | `find_by_id_including_deleted`| `find_by_id(id)` (same as normal)   |
/// | `soft_delete`                 | `delete(id)`                        |
/// | `restore`                     | `find_by_id(id)` (no-op restore)    |
/// | `hard_delete`                 | `delete(id)`                        |
/// | `list_deleted`                | `Ok((vec![], 0))`                   |
/// | `count`                       | `count()`                           |
/// | `count_deleted`               | `Ok(0)`                             |
/// | `empty_trash`                 | `Ok(0)`                             |
#[macro_export]
macro_rules! impl_crud_repository {
    // ── Soft-delete variant ────────────────────────────────────────────────────
    //
    // For entities that store deleted_at inside the metadata JSONB column.
    // The generated struct exposes: soft_delete, restore, permanent_delete,
    // list_deleted, find_deleted_by_id, count_active, count_deleted, empty_trash.
    ($repo:ty, $entity:ty, soft_delete) => {
        #[async_trait::async_trait]
        impl backbone_core::CrudRepository<$entity> for $repo {
            // ── NOTE on method resolution ─────────────────────────────────────
            // This repo is a newtype wrapping `GenericCrudRepository<E, SoftDelete>`
            // and implements `Deref` to it.  Inside a trait impl, unqualified `self.foo()`
            // resolves to the *trait* method (recursive) when both the trait and the
            // inner type have a method with the same name.  We use `(&**self).foo()`
            // to force resolution through the `Deref` target's inherent methods.

            async fn create(
                &self,
                entity: $entity,
            ) -> Result<$entity, backbone_core::RepositoryError> {
                (&**self).create(&entity)
                    .await
                    .map_err(|e| backbone_core::RepositoryError::DatabaseError(e.to_string()))
            }

            async fn find_by_id(
                &self,
                id: &str,
            ) -> Result<Option<$entity>, backbone_core::RepositoryError> {
                (&**self).find_by_id(id)
                    .await
                    .map_err(|e| backbone_core::RepositoryError::DatabaseError(e.to_string()))
            }

            async fn find_by_id_including_deleted(
                &self,
                id: &str,
            ) -> Result<Option<$entity>, backbone_core::RepositoryError> {
                (&**self).find_deleted_by_id(id)
                    .await
                    .map_err(|e| backbone_core::RepositoryError::DatabaseError(e.to_string()))
            }

            async fn update(
                &self,
                entity: $entity,
            ) -> Result<$entity, backbone_core::RepositoryError> {
                let id = backbone_core::PersistentEntity::entity_id(&entity);
                (&**self).update(&id, &entity)
                    .await
                    .map_err(|e| backbone_core::RepositoryError::DatabaseError(e.to_string()))
                    .and_then(|opt| opt.ok_or(backbone_core::RepositoryError::NotFound))
            }

            async fn soft_delete(
                &self,
                id: &str,
            ) -> Result<bool, backbone_core::RepositoryError> {
                (&**self).soft_delete(id)
                    .await
                    .map_err(|e| backbone_core::RepositoryError::DatabaseError(e.to_string()))
            }

            async fn restore(
                &self,
                id: &str,
            ) -> Result<Option<$entity>, backbone_core::RepositoryError> {
                (&**self).restore(id)
                    .await
                    .map_err(|e| backbone_core::RepositoryError::DatabaseError(e.to_string()))
            }

            async fn hard_delete(
                &self,
                id: &str,
            ) -> Result<bool, backbone_core::RepositoryError> {
                (&**self).permanent_delete(id)
                    .await
                    .map_err(|e| backbone_core::RepositoryError::DatabaseError(e.to_string()))
            }

            async fn list(
                &self,
                page: u32,
                limit: u32,
            ) -> Result<(Vec<$entity>, u64), backbone_core::RepositoryError> {
                let pagination =
                    backbone_orm::repository::PaginationParams { page, per_page: limit };
                (&**self).list_paginated(pagination)
                    .await
                    .map(|r| (r.data, r.pagination.total))
                    .map_err(|e| backbone_core::RepositoryError::DatabaseError(e.to_string()))
            }

            async fn list_filtered(
                &self,
                page: u32,
                limit: u32,
                filters: std::collections::HashMap<String, String>,
            ) -> Result<(Vec<$entity>, u64), backbone_core::RepositoryError> {
                let pagination =
                    backbone_orm::repository::PaginationParams { page, per_page: limit };
                (&**self).list_paginated_filtered(pagination, Some(&filters))
                    .await
                    .map(|r| (r.data, r.pagination.total))
                    .map_err(|e| backbone_core::RepositoryError::DatabaseError(e.to_string()))
            }

            async fn list_deleted(
                &self,
                page: u32,
                limit: u32,
            ) -> Result<(Vec<$entity>, u64), backbone_core::RepositoryError> {
                let pagination =
                    backbone_orm::repository::PaginationParams { page, per_page: limit };
                (&**self).list_deleted(pagination)
                    .await
                    .map(|r| (r.data, r.pagination.total))
                    .map_err(|e| backbone_core::RepositoryError::DatabaseError(e.to_string()))
            }

            async fn count(&self) -> Result<u64, backbone_core::RepositoryError> {
                (&**self).count_active()
                    .await
                    .map_err(|e| backbone_core::RepositoryError::DatabaseError(e.to_string()))
            }

            async fn count_deleted(&self) -> Result<u64, backbone_core::RepositoryError> {
                (&**self).count_deleted()
                    .await
                    .map_err(|e| backbone_core::RepositoryError::DatabaseError(e.to_string()))
            }

            async fn bulk_create(
                &self,
                entities: Vec<$entity>,
            ) -> Result<Vec<$entity>, backbone_core::RepositoryError> {
                let mut results = Vec::with_capacity(entities.len());
                for entity in entities {
                    let created = (&**self)
                        .create(&entity)
                        .await
                        .map_err(|e| backbone_core::RepositoryError::DatabaseError(e.to_string()))?;
                    results.push(created);
                }
                Ok(results)
            }

            async fn empty_trash(&self) -> Result<u64, backbone_core::RepositoryError> {
                (&**self).empty_trash()
                    .await
                    .map_err(|e| backbone_core::RepositoryError::DatabaseError(e.to_string()))
            }
        }
    };

    // ── No-soft-delete variant ─────────────────────────────────────────────────
    //
    // For entities that use hard deletes only (no soft_delete, restore, trash).
    // Soft-delete-related trait methods return sensible no-op results.
    ($repo:ty, $entity:ty, no_soft_delete) => {
        #[async_trait::async_trait]
        impl backbone_core::CrudRepository<$entity> for $repo {
            async fn create(
                &self,
                entity: $entity,
            ) -> Result<$entity, backbone_core::RepositoryError> {
                (&**self).create(&entity)
                    .await
                    .map_err(|e| backbone_core::RepositoryError::DatabaseError(e.to_string()))
            }

            async fn find_by_id(
                &self,
                id: &str,
            ) -> Result<Option<$entity>, backbone_core::RepositoryError> {
                (&**self).find_by_id(id)
                    .await
                    .map_err(|e| backbone_core::RepositoryError::DatabaseError(e.to_string()))
            }

            async fn find_by_id_including_deleted(
                &self,
                id: &str,
            ) -> Result<Option<$entity>, backbone_core::RepositoryError> {
                // No soft delete: "including deleted" == normal lookup
                (&**self).find_by_id(id)
                    .await
                    .map_err(|e| backbone_core::RepositoryError::DatabaseError(e.to_string()))
            }

            async fn update(
                &self,
                entity: $entity,
            ) -> Result<$entity, backbone_core::RepositoryError> {
                let id = backbone_core::PersistentEntity::entity_id(&entity);
                (&**self).update(&id, &entity)
                    .await
                    .map_err(|e| backbone_core::RepositoryError::DatabaseError(e.to_string()))
                    .and_then(|opt| opt.ok_or(backbone_core::RepositoryError::NotFound))
            }

            async fn soft_delete(
                &self,
                id: &str,
            ) -> Result<bool, backbone_core::RepositoryError> {
                // No soft delete: fall back to hard delete
                (&**self).delete(id)
                    .await
                    .map_err(|e| backbone_core::RepositoryError::DatabaseError(e.to_string()))
            }

            async fn restore(
                &self,
                id: &str,
            ) -> Result<Option<$entity>, backbone_core::RepositoryError> {
                // No soft delete: "restore" is a no-op, return the current row
                (&**self).find_by_id(id)
                    .await
                    .map_err(|e| backbone_core::RepositoryError::DatabaseError(e.to_string()))
            }

            async fn hard_delete(
                &self,
                id: &str,
            ) -> Result<bool, backbone_core::RepositoryError> {
                (&**self).delete(id)
                    .await
                    .map_err(|e| backbone_core::RepositoryError::DatabaseError(e.to_string()))
            }

            async fn list(
                &self,
                page: u32,
                limit: u32,
            ) -> Result<(Vec<$entity>, u64), backbone_core::RepositoryError> {
                let pagination =
                    backbone_orm::repository::PaginationParams { page, per_page: limit };
                (&**self).list_paginated(pagination)
                    .await
                    .map(|r| (r.data, r.pagination.total))
                    .map_err(|e| backbone_core::RepositoryError::DatabaseError(e.to_string()))
            }

            async fn list_filtered(
                &self,
                page: u32,
                limit: u32,
                filters: std::collections::HashMap<String, String>,
            ) -> Result<(Vec<$entity>, u64), backbone_core::RepositoryError> {
                let pagination =
                    backbone_orm::repository::PaginationParams { page, per_page: limit };
                (&**self).list_paginated_filtered(pagination, Some(&filters))
                    .await
                    .map(|r| (r.data, r.pagination.total))
                    .map_err(|e| backbone_core::RepositoryError::DatabaseError(e.to_string()))
            }

            async fn list_deleted(
                &self,
                _page: u32,
                _limit: u32,
            ) -> Result<(Vec<$entity>, u64), backbone_core::RepositoryError> {
                // No soft delete: trash is always empty
                Ok((vec![], 0))
            }

            async fn count(&self) -> Result<u64, backbone_core::RepositoryError> {
                (&**self).count()
                    .await
                    .map_err(|e| backbone_core::RepositoryError::DatabaseError(e.to_string()))
            }

            async fn count_deleted(&self) -> Result<u64, backbone_core::RepositoryError> {
                Ok(0)
            }

            async fn bulk_create(
                &self,
                entities: Vec<$entity>,
            ) -> Result<Vec<$entity>, backbone_core::RepositoryError> {
                let mut results = Vec::with_capacity(entities.len());
                for entity in entities {
                    let created = (&**self)
                        .create(&entity)
                        .await
                        .map_err(|e| backbone_core::RepositoryError::DatabaseError(e.to_string()))?;
                    results.push(created);
                }
                Ok(results)
            }

            async fn empty_trash(&self) -> Result<u64, backbone_core::RepositoryError> {
                Ok(0)
            }
        }
    };
}
