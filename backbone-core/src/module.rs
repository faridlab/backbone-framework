//! Backbone Module System
//!
//! This module provides the trait for modules to register themselves with
//! the framework, including migration discovery for automatic database setup.
//!
//! Inspired by Laravel's Service Provider pattern.

use async_trait::async_trait;
use std::path::PathBuf;

/// Metadata about a module's migrations
#[derive(Debug, Clone)]
pub struct MigrationInfo {
    /// Module name (e.g., "bersihir")
    pub module: String,
    /// Path to migrations directory
    pub path: PathBuf,
    /// Number of migrations found (optional, for status display)
    pub count: Option<usize>,
}

/// Metadata about a module's seeds
#[derive(Debug, Clone)]
pub struct SeedInfo {
    /// Module name
    pub module: String,
    /// Path to seeds directory
    pub path: PathBuf,
}

/// Trait for Backbone modules with migration support
///
/// Implement this trait to register a module with the Backbone framework.
/// The framework will automatically discover and run migrations for all
/// registered modules in dependency order.
///
/// # Example
///
/// ```ignore
/// use backbone_core::module::BackboneModule;
/// use std::path::PathBuf;
///
/// pub struct BersihirModule;
///
/// impl BackboneModule for BersihirModule {
///     fn name(&self) -> &'static str { "bersihir" }
///     fn version(&self) -> &'static str { env!("CARGO_PKG_VERSION") }
///
///     fn dependencies(&self) -> Vec<&'static str> {
///         vec!["sapiens"] // Depends on user module
///     }
///
///     fn migrations_path(&self) -> Option<PathBuf> {
///         Some(PathBuf::from("libs/modules/bersihir/migrations"))
///     }
/// }
/// ```
#[async_trait]
pub trait BackboneModule: Send + Sync {
    /// Returns the unique identifier for this module
    ///
    /// This should be a short, lowercase name like "bersihir", "sapiens", etc.
    fn name(&self) -> &'static str;

    /// Returns the version of this module
    ///
    /// Typically uses `env!("CARGO_PKG_VERSION")` to get from Cargo.toml
    fn version(&self) -> &'static str;

    /// Returns the list of module names this module depends on
    ///
    /// Dependencies are used to determine migration order. A module's
    /// migrations will only run after all its dependencies have been migrated.
    ///
    /// # Returns
    /// A list of module names (e.g., `vec!["sapiens", "bucket"]`)
    fn dependencies(&self) -> Vec<&'static str> {
        vec![]
    }

    /// Returns the path to this module's migrations directory
    ///
    /// Path should be relative to the repository root.
    /// Return `None` if the module has no migrations.
    ///
    /// # Example paths:
    /// - `libs/modules/bersihir/migrations`
    /// - `libs/modules/sapiens/migrations`
    fn migrations_path(&self) -> Option<PathBuf>;

    /// Returns the path to this module's seeds directory (optional)
    ///
    /// Seeds are used to populate the database with initial data.
    fn seeds_path(&self) -> Option<PathBuf> {
        None
    }

    /// Called after migrations are run to initialize the module
    ///
    /// Use this for any setup that needs to happen after the database
    /// is ready but before the application starts serving requests.
    async fn on_boot(&self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Called when the application is shutting down
    ///
    /// Use this for cleanup tasks.
    async fn on_shutdown(&self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Perform a health check for this module
    ///
    /// Returns `true` if the module is healthy and ready to serve requests.
    async fn health_check(&self) -> bool {
        true
    }

    /// Get migration info for this module
    fn migration_info(&self) -> Option<MigrationInfo> {
        self.migrations_path().map(|path| MigrationInfo {
            module: self.name().to_string(),
            path,
            count: None,
        })
    }

    /// Get seed info for this module
    fn seed_info(&self) -> Option<SeedInfo> {
        self.seeds_path().map(|path| SeedInfo {
            module: self.name().to_string(),
            path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestModule;

    impl BackboneModule for TestModule {
        fn name(&self) -> &'static str {
            "test"
        }

        fn version(&self) -> &'static str {
            "0.1.0"
        }

        fn dependencies(&self) -> Vec<&'static str> {
            vec!["core"]
        }

        fn migrations_path(&self) -> Option<PathBuf> {
            Some(PathBuf::from("libs/modules/test/migrations"))
        }

        fn seeds_path(&self) -> Option<PathBuf> {
            Some(PathBuf::from("libs/modules/test/migrations/seeds"))
        }
    }

    #[test]
    fn test_module_metadata() {
        let module = TestModule;
        assert_eq!(module.name(), "test");
        assert_eq!(module.version(), "0.1.0");
        assert_eq!(module.dependencies(), vec!["core"]);
    }

    #[test]
    fn test_migration_info() {
        let module = TestModule;
        let info = module.migration_info();
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.module, "test");
        assert_eq!(info.path, PathBuf::from("libs/modules/test/migrations"));
    }

    #[test]
    fn test_seed_info() {
        let module = TestModule;
        let info = module.seed_info();
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.module, "test");
        assert_eq!(info.path, PathBuf::from("libs/modules/test/migrations/seeds"));
    }
}
