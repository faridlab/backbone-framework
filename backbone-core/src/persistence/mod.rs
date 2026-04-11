//! Persistence Layer - Reusable Repository Implementations
//!
//! This module provides generic, reusable repository implementations that can be
//! used across all modules in the framework. Instead of writing custom repositories
//! for each entity, modules can use these generic implementations.
//!
//! # Available Implementations
//!
//! - `InMemoryRepository` - Thread-safe in-memory storage (testing, prototyping)
//! - `PostgresRepository` - PostgreSQL-backed storage (production, requires "postgres" feature)
//!
//! # Usage Pattern
//!
//! ## In-Memory (testing/prototyping)
//!
//! ```ignore
//! use backbone_core::persistence::{InMemoryRepository, PersistentEntity, CrudRepository};
//!
//! #[derive(Clone, Debug, Serialize, Deserialize)]
//! struct User {
//!     id: String,
//!     name: String,
//!     created_at: Option<DateTime<Utc>>,
//!     updated_at: Option<DateTime<Utc>>,
//!     deleted_at: Option<DateTime<Utc>>,
//! }
//!
//! impl PersistentEntity for User { /* ... */ }
//!
//! let repo = InMemoryRepository::<User>::new();
//! let user = repo.create(user).await?;
//! ```
//!
//! ## PostgreSQL (production)
//!
//! ```ignore
//! use backbone_core::persistence::{PostgresRepository, PostgresEntity, PersistentEntity};
//! use sqlx::FromRow;
//!
//! #[derive(Clone, Debug, Serialize, Deserialize, FromRow)]
//! struct User {
//!     id: String,
//!     name: String,
//!     created_at: Option<DateTime<Utc>>,
//!     updated_at: Option<DateTime<Utc>>,
//!     deleted_at: Option<DateTime<Utc>>,
//! }
//!
//! impl PersistentEntity for User { /* ... */ }
//!
//! impl PostgresEntity for User {
//!     fn table_name() -> &'static str { "users" }
//!     fn select_columns() -> &'static [&'static str] {
//!         &["id", "name", "created_at", "updated_at", "deleted_at"]
//!     }
//!     fn insert_columns() -> &'static [&'static str] {
//!         &["id", "name", "created_at", "updated_at"]
//!     }
//!     fn bind_for_insert<'q>(entity: &Self, query: Query<'q, ...>) -> Query<'q, ...> {
//!         query.bind(&entity.id).bind(&entity.name)
//!             .bind(entity.created_at).bind(entity.updated_at)
//!     }
//! }
//!
//! let pool = PgPool::connect("...").await?;
//! let repo = PostgresRepository::<User>::new(pool);
//! let user = repo.create(user).await?;
//! ```

pub mod adapter;
pub mod memory;
pub mod traits;

#[cfg(feature = "postgres")]
pub mod postgres;

// Re-exports from traits
pub use traits::{
    CrudRepository, PartialUpdatable, PersistentEntity, RepositoryError, SearchableRepository,
    Versioned,
};

// Re-export InMemoryRepository
pub use memory::InMemoryRepository;

// Re-export CrudServiceAdapter for bridging Repository to CrudService
pub use adapter::{AdapterError, CrudServiceAdapter, SearchableCrudServiceAdapter, SimpleCrudServiceAdapter};

// Re-export PostgreSQL types when feature is enabled
#[cfg(feature = "postgres")]
pub use postgres::{PostgresRepository, PostgresRepositoryBuilder};

#[cfg(feature = "postgres")]
pub use traits::PostgresEntity;
