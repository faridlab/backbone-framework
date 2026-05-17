//! Module Registry - Discovery and management of Backbone modules
//!
//! The Module Registry provides centralized registration of modules,
//! automatic migration discovery, and dependency-ordered initialization.
//!
//! # Architecture
//!
//! ```text
//!                    ┌────────────────────────────┐
//!                    │      ModuleRegistry        │
//!                    │   (Application Bootstrap)  │
//!                    └────────────────────────────┘
//!                              │
//!              ┌───────────────┼───────────────┐
//!              │               │               │
//!              ▼               ▼               ▼
//!     ┌────────────┐  ┌────────────┐  ┌────────────┐
//!     │  Sapiens   │  │  Bersihir  │  │  Bucket    │
//!     │  Module    │  │  Module    │  │  Module    │
//!     └────────────┘  └────────────┘  └────────────┘
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use backbone_core::module_registry::ModuleRegistry;
//! use backbone_core::module::BackboneModule;
//!
//! // Create registry
//! let mut registry = ModuleRegistry::new();
//!
//! // Register modules
//! registry.register(SapiensModule::new());
//! registry.register(BersihirModule::new());
//!
//! // Get modules in dependency order
//! for module in registry.modules_ordered() {
//!     // Run migrations, initialize, etc.
//! }
//!
//! // Get all migration paths
//! for (name, path) in registry.all_migration_paths() {
//!     println!("Module {}: {}", name, path.display());
//! }
//! ```

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::module::{BackboneModule, MigrationInfo, SeedInfo};

/// Error types for module registry operations
#[derive(Debug, thiserror::Error)]
pub enum ModuleRegistryError {
    #[error("Circular dependency detected: {0}")]
    CircularDependency(String),

    #[error("Unknown dependency '{dependency}' for module '{module}'")]
    UnknownDependency {
        module: String,
        dependency: String,
    },

    #[error("Module '{0}' is already registered")]
    DuplicateModule(String),
}

/// Result type for module registry operations
pub type ModuleRegistryResult<T> = Result<T, ModuleRegistryError>;

/// Registry for managing Backbone modules
///
/// The registry handles:
/// - Module registration
/// - Dependency resolution and ordering
/// - Migration path discovery
/// - Module lifecycle (boot, shutdown)
pub struct ModuleRegistry {
    modules: HashMap<String, Arc<dyn BackboneModule>>,
    order: Option<Vec<String>>,
}

impl ModuleRegistry {
    /// Creates a new empty module registry
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
            order: None,
        }
    }

    /// Registers a module with the registry
    ///
    /// # Arguments
    /// * `module` - The module to register
    ///
    /// # Returns
    /// An error if a module with the same name is already registered
    pub fn register<M: BackboneModule + 'static>(&mut self, module: M) -> ModuleRegistryResult<()> {
        let name = module.name().to_string();

        if self.modules.contains_key(&name) {
            return Err(ModuleRegistryError::DuplicateModule(name));
        }

        self.modules.insert(name, Arc::new(module));
        self.order = None; // Invalidate cached order
        Ok(())
    }

    /// Registers an Arc-wrapped module with the registry
    pub fn register_arc(&mut self, module: Arc<dyn BackboneModule>) -> ModuleRegistryResult<()> {
        let name = module.name().to_string();

        if self.modules.contains_key(&name) {
            return Err(ModuleRegistryError::DuplicateModule(name));
        }

        self.modules.insert(name, module);
        self.order = None;
        Ok(())
    }

    /// Returns the number of registered modules
    pub fn len(&self) -> usize {
        self.modules.len()
    }

    /// Returns true if no modules are registered
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    /// Gets a module by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn BackboneModule>> {
        self.modules.get(name).cloned()
    }

    /// Returns all module names
    pub fn module_names(&self) -> Vec<String> {
        self.modules.keys().cloned().collect()
    }

    /// Returns all modules in dependency order
    ///
    /// Modules with no dependencies come first, followed by modules
    /// whose dependencies have already been returned.
    ///
    /// # Returns
    /// An error if there are circular dependencies or unknown dependencies
    pub fn modules_ordered(&mut self) -> ModuleRegistryResult<Vec<Arc<dyn BackboneModule>>> {
        // Compute order if not cached
        if self.order.is_none() {
            self.order = Some(self.topological_sort()?);
        }

        let order = self.order.as_ref().unwrap();
        Ok(order
            .iter()
            .filter_map(|name| self.modules.get(name).cloned())
            .collect())
    }

    /// Returns all migration paths in dependency order
    ///
    /// # Returns
    /// A vector of (module_name, migrations_path) tuples
    pub fn all_migration_paths(&mut self) -> ModuleRegistryResult<Vec<MigrationInfo>> {
        let modules = self.modules_ordered()?;
        Ok(modules
            .iter()
            .filter_map(|m| m.migration_info())
            .collect())
    }

    /// Returns all seed paths in dependency order
    pub fn all_seed_paths(&mut self) -> ModuleRegistryResult<Vec<SeedInfo>> {
        let modules = self.modules_ordered()?;
        Ok(modules
            .iter()
            .filter_map(|m| m.seed_info())
            .collect())
    }

    /// Performs topological sort of modules based on dependencies
    fn topological_sort(&self) -> ModuleRegistryResult<Vec<String>> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut in_progress = HashSet::new();

        // Build dependency graph
        let module_names: HashSet<_> = self.modules.keys().cloned().collect();

        // Validate all dependencies exist
        for module in self.modules.values() {
            for dep in module.dependencies() {
                if !module_names.contains(dep) {
                    return Err(ModuleRegistryError::UnknownDependency {
                        module: module.name().to_string(),
                        dependency: dep.to_string(),
                    });
                }
            }
        }

        // Visit each module
        for name in self.modules.keys() {
            self.visit(name, &mut visited, &mut in_progress, &mut result)?;
        }

        Ok(result)
    }

    /// DFS visit for topological sort
    fn visit(
        &self,
        name: &str,
        visited: &mut HashSet<String>,
        in_progress: &mut HashSet<String>,
        result: &mut Vec<String>,
    ) -> ModuleRegistryResult<()> {
        if visited.contains(name) {
            return Ok(());
        }

        if in_progress.contains(name) {
            return Err(ModuleRegistryError::CircularDependency(name.to_string()));
        }

        in_progress.insert(name.to_string());

        // Visit dependencies first
        if let Some(module) = self.modules.get(name) {
            for dep in module.dependencies() {
                self.visit(dep, visited, in_progress, result)?;
            }
        }

        in_progress.remove(name);
        visited.insert(name.to_string());
        result.push(name.to_string());

        Ok(())
    }

    /// Boots all modules in dependency order
    pub async fn boot_all(&mut self) -> anyhow::Result<()> {
        let modules = self.modules_ordered()?;
        for module in modules {
            tracing::info!("Booting module: {} v{}", module.name(), module.version());
            module.on_boot().await?;
        }
        Ok(())
    }

    /// Shuts down all modules in reverse dependency order
    pub async fn shutdown_all(&mut self) -> anyhow::Result<()> {
        let mut modules = self.modules_ordered()?;
        modules.reverse(); // Shutdown in reverse order

        for module in modules {
            tracing::info!("Shutting down module: {}", module.name());
            if let Err(e) = module.on_shutdown().await {
                tracing::warn!("Error shutting down module {}: {}", module.name(), e);
            }
        }
        Ok(())
    }

    /// Performs health checks on all modules
    pub async fn health_check_all(&self) -> HashMap<String, bool> {
        let mut results = HashMap::new();

        for (name, module) in &self.modules {
            let healthy = module.health_check().await;
            results.insert(name.clone(), healthy);
        }

        results
    }

    /// Prints a summary of registered modules
    #[allow(clippy::print_literal)]
    pub fn print_summary(&self) {
        println!("\n📦 Registered Modules:");
        println!("  {0:<20} {1:<12} {2}", "Name", "Version", "Dependencies");
        println!("  {}", "-".repeat(60));

        for (name, module) in &self.modules {
            let deps = module.dependencies();
            let deps_str = if deps.is_empty() {
                "-".to_string()
            } else {
                deps.join(", ")
            };
            println!("  {:<20} {:<12} {}", name, module.version(), deps_str);
        }
        println!();
    }
}

impl Default for ModuleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::BackboneModule;
    use std::path::PathBuf;

    struct TestModuleA;
    struct TestModuleB;
    struct TestModuleC;

    impl BackboneModule for TestModuleA {
        fn name(&self) -> &'static str { "module_a" }
        fn version(&self) -> &'static str { "1.0.0" }
        fn dependencies(&self) -> Vec<&'static str> { vec![] }
        fn migrations_path(&self) -> Option<PathBuf> {
            Some(PathBuf::from("libs/modules/a/migrations"))
        }
    }

    impl BackboneModule for TestModuleB {
        fn name(&self) -> &'static str { "module_b" }
        fn version(&self) -> &'static str { "1.0.0" }
        fn dependencies(&self) -> Vec<&'static str> { vec!["module_a"] }
        fn migrations_path(&self) -> Option<PathBuf> {
            Some(PathBuf::from("libs/modules/b/migrations"))
        }
    }

    impl BackboneModule for TestModuleC {
        fn name(&self) -> &'static str { "module_c" }
        fn version(&self) -> &'static str { "1.0.0" }
        fn dependencies(&self) -> Vec<&'static str> { vec!["module_a", "module_b"] }
        fn migrations_path(&self) -> Option<PathBuf> {
            Some(PathBuf::from("libs/modules/c/migrations"))
        }
    }

    #[test]
    fn test_register_modules() {
        let mut registry = ModuleRegistry::new();

        assert!(registry.register(TestModuleA).is_ok());
        assert!(registry.register(TestModuleB).is_ok());

        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn test_duplicate_module() {
        let mut registry = ModuleRegistry::new();

        assert!(registry.register(TestModuleA).is_ok());

        // Attempting to register again should fail
        let result = registry.register(TestModuleA);
        assert!(result.is_err());
    }

    #[test]
    fn test_dependency_order() {
        let mut registry = ModuleRegistry::new();

        // Register in reverse dependency order
        registry.register(TestModuleC).unwrap();
        registry.register(TestModuleB).unwrap();
        registry.register(TestModuleA).unwrap();

        let ordered = registry.modules_ordered().unwrap();
        let names: Vec<_> = ordered.iter().map(|m| m.name()).collect();

        // A should come before B, B should come before C
        let pos_a = names.iter().position(|&n| n == "module_a").unwrap();
        let pos_b = names.iter().position(|&n| n == "module_b").unwrap();
        let pos_c = names.iter().position(|&n| n == "module_c").unwrap();

        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
    }

    #[test]
    fn test_migration_paths() {
        let mut registry = ModuleRegistry::new();

        registry.register(TestModuleA).unwrap();
        registry.register(TestModuleB).unwrap();
        registry.register(TestModuleC).unwrap();

        let paths = registry.all_migration_paths().unwrap();
        assert_eq!(paths.len(), 3);

        // First should be module_a
        assert_eq!(paths[0].module, "module_a");
    }

    struct CircularA;
    struct CircularB;

    impl BackboneModule for CircularA {
        fn name(&self) -> &'static str { "circular_a" }
        fn version(&self) -> &'static str { "1.0.0" }
        fn dependencies(&self) -> Vec<&'static str> { vec!["circular_b"] }
        fn migrations_path(&self) -> Option<PathBuf> { None }
    }

    impl BackboneModule for CircularB {
        fn name(&self) -> &'static str { "circular_b" }
        fn version(&self) -> &'static str { "1.0.0" }
        fn dependencies(&self) -> Vec<&'static str> { vec!["circular_a"] }
        fn migrations_path(&self) -> Option<PathBuf> { None }
    }

    #[test]
    fn test_circular_dependency_detection() {
        let mut registry = ModuleRegistry::new();

        registry.register(CircularA).unwrap();
        registry.register(CircularB).unwrap();

        let result = registry.modules_ordered();
        assert!(result.is_err());

        if let Err(ModuleRegistryError::CircularDependency(_)) = result {
            // Expected
        } else {
            panic!("Expected CircularDependency error");
        }
    }

    #[test]
    fn test_unknown_dependency() {
        let mut registry = ModuleRegistry::new();

        // Register B without A (B depends on A)
        registry.register(TestModuleB).unwrap();

        let result = registry.modules_ordered();
        assert!(result.is_err());

        if let Err(ModuleRegistryError::UnknownDependency { module, dependency }) = result {
            assert_eq!(module, "module_b");
            assert_eq!(dependency, "module_a");
        } else {
            panic!("Expected UnknownDependency error");
        }
    }
}
