//! Backbone Framework Core
//!
//! Foundation for generic CRUD system with 11 standard endpoints.
//! This is the core package that can be published and reused across projects.
//!
//! # Key Components
//!
//! - **`CrudService` trait**: Core async trait for entity CRUD operations
//! - **`BackboneCrudHandler`**: Generic Axum router builder for all 11 endpoints
//! - **Response types**: `ApiResponse`, `PaginatedResponse`, `BulkResponse`
//! - **Utilities**: Timestamp, ID generation, pagination, validation
//! - **Persistence**: Generic repositories for PostgreSQL and in-memory storage
//!
//! # Quick Start
//!
//! ## Option 1: Using BackboneCrudHandler (recommended)
//!
//! ```ignore
//! use backbone_core::{CrudService, BackboneCrudHandler, ApiResponse};
//!
//! // 1. Implement CrudService for your entity
//! impl CrudService<User, CreateUserDto, UpdateUserDto> for UserCrudService {
//!     // ... implement all 11 methods
//! }
//!
//! // 2. Create router with all 11 endpoints
//! let routes = BackboneCrudHandler::<_, User, CreateUserDto, UpdateUserDto, UserResponse>
//!     ::routes(Arc::new(user_crud), "/api/v1/users");
//! ```
//!
//! ## Option 2: Using Generic Repositories
//!
//! ```ignore
//! use backbone_core::persistence::{InMemoryRepository, PostgresRepository, CrudRepository};
//!
//! // For testing/prototyping
//! let repo = InMemoryRepository::<User>::new();
//!
//! // For production (requires "postgres" feature)
//! let repo = PostgresRepository::<User>::new(pool);
//! ```

// Macros must be declared before any items that use them.
// `#[macro_export]` makes them available as `backbone_core::impl_crud_repository!`.
pub mod macros;

/// Number of standard CRUD endpoints generated per entity
pub const STANDARD_ENDPOINT_COUNT: usize = 12;

pub mod config;
pub mod crud;
pub mod entity;
pub mod repository;
pub mod http;
pub mod grpc;
pub mod builder;
pub mod utils;
pub mod persistence;
pub mod specification;
pub mod registry;

// Module system - Laravel-style service provider pattern
pub mod module;
pub mod module_registry;

// DDD Pattern modules
pub mod domain_service;
pub mod command;
pub mod query;
pub mod value_object;
pub mod aggregate;
pub mod error;

// Generic base layer — Phase 0 composition foundation
pub mod service;
pub mod usecase;
pub mod validation;
pub mod policy;
pub mod bulk;
pub mod integration;
pub mod cqrs;
pub mod graphql;

// Category C generic base traits — Phase 0 extension for trigger/flow/projection
pub mod trigger;
pub mod flow;
pub mod projection;
pub mod state_machine;

pub use trigger::{
    TriggerHandler, TriggerEvent, TriggerContext, TriggerContextMut,
    ActionExecutor, TriggerRegistry,
};

// Re-export core types for convenience
pub use entity::*;
pub use repository::*;
pub use grpc::*;
pub use builder::*;

// Configuration exports
pub use config::{
    BackboneConfig, ConfigError, ConfigResult, ConfigLoader,
    // App config
    AppConfig, Environment,
    // Server config
    ServerConfig,
    // Database config
    DatabaseConfig, CacheConfig,
    // Module configs
    ModulesConfig, SapiensConfig, PostmanConfig, BucketConfig,
    SapiensAuthConfig, PasswordHasherConfig, SapiensLockoutConfig,
    SmtpConfig, TemplatesConfig, StorageConfig,
    // Logging config
    LoggingConfig, LoggingFileConfig,
    // Monitoring config
    MonitoringConfig,
    // Context config
    ContextsConfig, RedisEventBusConfig,
    // Features config
    FeaturesConfig, RateLimitingConfig,
    // Security config
    SecurityConfig, CsrfConfig, SecurityHeadersConfig,
    // Configuration Bus for cross-module config sharing
    ConfigurationBus, ConfigValue, ConfigChangeEvent,
};

// HTTP module exports - primary API for generic CRUD
pub use http::{
    // Core generic CRUD components
    CrudService,
    BackboneCrudHandler,
    BackboneHttpHandler,
    // Response types
    ApiResponse,
    PaginatedResponse,
    PaginationResponse,
    BulkResponse,
    BulkCreateRequest,
    // Request types
    ListQueryParams,
    ListRequest,
    UpsertRequest,
    FilterOptions,
    SortOrder,
    // Legacy
    PaginationRequest,
};

// Utility exports
pub use utils::{
    BackboneError, PaginationParams, PaginationMeta as BackbonePaginationMeta,
    now, days_ago, days_from_now, hours_ago, hours_from_now, minutes_from_now,
    new_id, new_id_string, parse_id, is_valid_uuid,
    is_valid_email, is_valid_username, normalize_string,
};

// Persistence layer exports - Generic repositories
pub use persistence::{
    // Core traits
    CrudRepository, PersistentEntity, RepositoryError, PartialUpdatable, SearchableRepository, Versioned,
    // In-memory repository (always available)
    InMemoryRepository,
    // Adapters for bridging Repository to CrudService
    CrudServiceAdapter, SimpleCrudServiceAdapter, AdapterError,
};

// Specification pattern exports - DDD business rules
pub use specification::{
    // Core trait
    Specification,
    // Composite specifications
    AndSpecification, OrSpecification, NotSpecification,
    // Result and evaluator
    SpecificationResult, SpecificationEvaluator,
    // Common specifications
    AlwaysTrue, AlwaysFalse, PredicateSpecification,
    // Helper function
    predicate,
};

// PostgreSQL exports (requires "postgres" feature)
#[cfg(feature = "postgres")]
pub use persistence::{PostgresRepository, PostgresRepositoryBuilder, PostgresEntity};

// Service Registry exports - Module service discovery
pub use registry::{
    // Core trait
    ModuleService,
    // Registry
    ServiceRegistry,
    // Health types
    ServiceHealth, HealthStatus, RegistryHealth, RegistryStatistics,
    // Metadata
    ServiceDescriptor,
};

// Module System exports - Laravel-style service provider pattern
pub use module::{
    BackboneModule,
    MigrationInfo,
    SeedInfo,
};

pub use module_registry::{
    ModuleRegistry,
    ModuleRegistryError,
    ModuleRegistryResult,
};

// DDD Pattern exports - Domain-Driven Design traits
pub use domain_service::{DomainService, TransactionalDomainService};

pub use command::{Command, CommandHandler, CommandDispatcher, ValidatableCommand, ValidatingCommandHandler};

pub use query::{
    Query, QueryHandler, QueryDispatcher,
    CacheableQuery, PaginatedQuery, PaginatedQueryResult,
};

pub use value_object::{ValueObject, OptionalValueObject, ValueObjectError};

pub use aggregate::{AggregateRoot, EventSourcedAggregate, InvariantAggregate, AggregateMetadata};

pub use error::{ModuleError, ErrorCategory, ErrorResponse, CommonError};

// Generic base layer re-exports
pub use service::{
    GenericCrudService, ServiceError, ServiceResult,
    FromCreateDto, ApplyUpdateDto, ServiceLifecycle, NoOpLifecycle,
};

pub use usecase::{
    UseCaseError, UseCaseResult,
    UseCaseHooks, DefaultHooks,
    UseCaseService, EntityFactory, EntityUpdater,
    CreateUseCase, UpdateUseCase, GetUseCase, DeleteUseCase,
    ListUseCase, ListParams, ListResult, ListService,
};

pub use validation::{
    ValidationError, ValidationErrors, FieldRule, EntityValidator,
    RequiredString, MaxLength, NonNegative, OptionalNotBlank, Regex, RequiredUuid,
};

pub use policy::{
    PolicyContext, PolicyOutcome, PolicyDecision, DomainPolicy,
    PermitAllPolicy, DenyAllPolicy,
    AllOfPolicy, AnyOfPolicy, NotPolicy,
};

pub use bulk::{
    BulkFailureMode, BulkOperationConfig, BulkItemResult, BulkOperationResult,
    BulkOperationProgress, BulkCapableService, GenericBulkService,
};

pub use integration::{
    ModuleAdapter, ProjectionAdapter, IntegrationError,
    EventBridge, IdentityAdapter, identity_adapter,
};

pub use cqrs::{
    // Commands
    GenericCreateCommand, GenericUpdateCommand, GenericDeleteCommand, GenericRestoreCommand,
    // Queries
    GenericGetQuery, GenericListQuery, GenericListDeletedQuery,
    // Handlers
    GenericCommandHandler, GenericQueryHandler,
    // Service contracts
    CqrsService, CqrsReadService,
};

pub use graphql::{
    GenericGraphQLResolver, GraphQLCapableService,
    GraphQLListResult, GraphQLPaginationInput,
};

// grpc module already fully re-exported via `pub use grpc::*` above.
// GenericGrpcService, GrpcCapableService and all gRPC types are available at crate root.

/// Core version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The 11 standard Backbone endpoints that are auto-generated for each entity:
/// 1. GET /api/v1/{collection} - List (paginated, filtered, sorted)
/// 2. POST /api/v1/{collection} - Create
/// 3. GET /api/v1/{collection}/:id - Get by ID
/// 4. PUT /api/v1/{collection}/:id - Full update
/// 5. PATCH /api/v1/{collection}/:id - Partial update
/// 6. DELETE /api/v1/{collection}/:id - Soft delete
/// 7. POST /api/v1/{collection}/bulk - Bulk create
/// 8. POST /api/v1/{collection}/upsert - Upsert
/// 9. GET /api/v1/{collection}/trash - List deleted
/// 10. POST /api/v1/{collection}/:id/restore - Restore
/// 11. DELETE /api/v1/{collection}/empty - Empty trash
pub const STANDARD_ENDPOINTS: [&str; 11] = [
    "GET /api/v1/{collection}",
    "POST /api/v1/{collection}",
    "GET /api/v1/{collection}/:id",
    "PUT /api/v1/{collection}/:id",
    "PATCH /api/v1/{collection}/:id",
    "DELETE /api/v1/{collection}/:id",
    "POST /api/v1/{collection}/bulk",
    "POST /api/v1/{collection}/upsert",
    "GET /api/v1/{collection}/trash",
    "POST /api/v1/{collection}/:id/restore",
    "DELETE /api/v1/{collection}/empty",
];