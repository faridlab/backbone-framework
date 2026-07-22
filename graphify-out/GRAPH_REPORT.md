# Graph Report - backbone-framework  (2026-07-22)

## Corpus Check
- 303 files · ~334,359 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 8414 nodes · 20224 edges · 324 communities (316 shown, 8 thin omitted)
- Extraction: 100% EXTRACTED · 0% INFERRED · 0% AMBIGUOUS · INFERRED: 51 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `1654f880`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- Result
- jwt.rs
- AlertEvent
- RabbitMQQueueSimple
- RedisCache
- backbone-storage/src/compression.rs
- Error
- MemoryCache
- ModuleRegistry
- Result
- JobError
- IntegrationEventBus
- E
- RedisQueue
- backbone-maintenance/src/lib.rs
- SecurityEngine
- cqrs.rs
- usecase.rs
- E
- StorageFile
- QueueResult
- http.rs
- JobId
- LocalStorage
- String
- GenericCrudService<E, C, U, R>
- T
- ElasticsearchSearch
- TaskService
- EmailAddress
- SeedManager
- Result
- ECommerceService
- QueueMessage
- testing_examples.rs
- AlgoliaSearch
- MailgunEmailService
- LocalStorage
- ServiceContainer
- InMemoryRepository<E>
- ServiceRegistry
- QueueManager
- S3Storage
- bulk.rs
- ConfigurationBus
- ServiceResult
- company_scope.rs
- Backbone Cache
- Backbone Search
- Backbone Auth
- SmtpEmailService
- EventEnvelope
- MessageCompressor
- User
- SqsQueue
- Backbone Storage
- RepositoryError
- Backbone Email
- SesEmailService
- HealthChecker
- Self
- JobSchedulerBuilder
- MigrationManager
- cron.rs
- modules_config.rs
- utils.rs
- Backbone-ORM Examples
- FifoQueueServiceWrapper
- SecureUserDatabase
- GrpcResponse
- MonitoringService
- subscriber.rs
- InMemoryStore<T>
- MinIOStorage
- ApiResponse
- MockQueueService
- backbone-observability/src/middleware.rs
- fifo_tests.rs
- backbone-search/src/types.rs
- crud_event.rs
- PostgresRepository<T>
- Self
- messaging_tests.rs
- backbone-search/src/traits.rs
- backbone-core/src/integration.rs
- backbone-core/src/service.rs
- JobSchedulerConfig
- Self
- backbone-storage/src/traits.rs
- Job
- logging.rs
- filter/tests.rs
- AuthService
- Backbone Core Examples
- EventError
- SimpleUser
- backbone-authorization/src/types.rs
- FlowInstance
- ComponentStatus
- QueryValue
- orm_tests.rs
- compression_tests.rs
- BackboneConfig
- JobBuilder
- Backbone Search Examples
- ErrorHandlingService
- repository_tests.rs
- RateLimitConfig
- docs/architecture.md
- RabbitMQQueue
- AuthorizationService
- schema.rs
- backbone-core/src/error.rs
- Backbone Jobs
- cache.rs
- backbone-authorization/src/middleware.rs
- JobScheduler
- processor_demo.rs
- EcommerceCacheService
- Backbone Core
- company.rs
- backbone-observability/src/audit.rs
- JobSchedulerBuilder
- PgCronManager
- MonitoringDashboard
- monitoring_tests.rs
- Result
- InMemoryJobStorage
- metrics.rs
- RabbitMQ Integration Guide
- Glossary — Ubiquitous Language
- Result
- CustomHealthCheck
- sqs_tests.rs
- cache_tests.rs
- QueueWorker
- MockQueue
- integration_tests.rs
- tests/registry.rs
- database_config.rs
- LogLine
- CollectingHandler<E>
- IntegrationEvent
- tracing.rs
- .substitute_env_vars
- Query
- Backbone-ORM
- security_config.rs
- backbone-core/src/lib.rs
- health_tests.rs
- DomainEvent
- .from_event
- Backbone Queue Examples
- SearchStats
- AppState
- ConfigError
- value_object.rs
- EmailServiceStats
- SimpleHealthServer
- QueueStats
- RabbitMQConfig
- api_integration.rs
- real_world_scenarios.rs
- 📡 Backbone Core — API Reference
- Self
- dual.rs
- StorageError
- developer-guide.md
- TestAggregate
- RawQueryBuilder
- company_guard.rs
- RateLimitResult
- ServerConfig
- PersistentEntity
- CrudRepository
- FilterValue
- company_fence.rs
- AdvancedQueryBuilder
- rabbitmq_realtime_chat.rs
- InMemoryStorage
- .get_or_build
- PasswordService
- auth_service_tests.rs
- .get_handler
- routes.rs
- TenantId
- Changelog
- Environment
- Widget
- main
- BatchingProcessor
- deduplication_demo.rs
- InMemoryPermissionService<R>
- RedisPermissionCache
- crud_macro_compile.rs
- filter_bench.rs
- FilterCondition
- QueryFilter
- Usage
- Backbone Queue Module
- rate_limit_middleware
- backbone-cache/examples/basic_usage.rs
- 🧾 Backbone Core — OpenAPI / Swagger Guide
- 🚀 Backbone Core — Usage
- features_config.rs
- init_observability
- OutboxRow
- main
- fifo_queue_demo.rs
- Technical Details
- BatchProcessingResult
- backbone-tenant/src/lib.rs
- Developer Guide
- The reasoning
- SimplePermission
- 🏛️ Backbone Core — Architecture
- OutboxRecord
- SimpleMessageProcessor
- main
- main
- aggregate.rs
- runner.rs
- mechanics.rs
- ProcessedMessage
- main
- backbone-rate-limit
- main
- main
- Metaphor Crate
- .new
- EmailConfig
- email_service_tests.rs
- main
- main
- Background & Prior Art
- backbone-auth/examples/basic_usage.rs
- permissions.rs
- cli.rs
- main
- ConnectionManager
- parser.rs
- validate_schema
- ProcessorStats
- run_all_tests
- .build
- Maintainer Guide
- Philosophy & Motivation
- backbone-auth/src/audit.rs
- redis_cache_tests.rs
- domain_service.rs
- .from_request
- HealthConfig
- redis_integration.rs
- provision.rs
- Contribution Guide
- Backbone Framework
- MonitoringConfig
- state_machine.rs
- raw_query.rs
- once
- Advanced Examples
- Message Creation
- QueueService
- Architecture
- .new
- BackboneCrudHandler
- filter/validation.rs
- 📋 Best Practices
- RetryConfig
- UserRepository
- ApiResponse<T>
- backbone-graphql/src/error.rs
- main
- main
- 🔧 Advanced Usage
- OutboxError
- relay_runner.rs
- main
- main
- Queue Operations
- Best Practices
- Common Issues
- ADR-0001: Distribute via git tags, not crates.io
- AuthorizationServiceTrait
- ListRequest
- Projector
- PaginationInfo
- 🔧 Best Practices
- 🏢 Production Deployment
- 🔐 Security Best Practices
- 🔄 Message Patterns
- Example Output
- Advanced Usage
- Overview
- Backend Implementations
- Real-World Examples
- Testing
- Backbone Framework — Handbook
- BackboneHttpHandler
- 📋 Predefined Job Types
- 🔧 Configuration Options
- 🚀 Quick Start
- 🐛 Troubleshooting
- 📊 Monitoring & Health Checks
- Monitoring and Metrics
- SearchService
- backbone-tenant
- GraphQLListResult<E>
- openapi.rs
- backbone-outbox/README.md

## God Nodes (most connected - your core abstractions)
1. `String` - 993 edges
2. `T` - 164 edges
3. `QueueMessage` - 90 edges
4. `Job` - 62 edges
5. `RepositoryError` - 54 edges
6. `ApiResponse` - 52 edges
7. `StorageFile` - 49 edges
8. `JobScheduler` - 47 edges
9. `QueueManager` - 40 edges
10. `GenericCrudService<E, C, U, R>` - 39 edges

## Surprising Connections (you probably didn't know these)
- `RegisterRequest` --references--> `String`  [EXTRACTED]
  backbone-auth/examples/api_integration.rs → backbone-core/src/persistence/adapter.rs
- `UserPreferences` --references--> `String`  [EXTRACTED]
  backbone-cache/examples/advanced_usage.rs → backbone-core/src/persistence/adapter.rs
- `User` --references--> `String`  [EXTRACTED]
  backbone-cache/examples/basic_usage.rs → backbone-core/src/persistence/adapter.rs
- `CartSummary` --references--> `String`  [EXTRACTED]
  backbone-cache/examples/real_world_scenarios.rs → backbone-core/src/persistence/adapter.rs
- `EmailError` --references--> `String`  [EXTRACTED]
  backbone-email/src/lib.rs → backbone-core/src/persistence/adapter.rs

## Import Cycles
- 1-file cycle: `backbone-queue/src/fifo.rs -> backbone-queue/src/fifo.rs`
- 2-file cycle: `backbone-orm/src/query_builder.rs -> backbone-orm/src/raw_query.rs -> backbone-orm/src/query_builder.rs`

## Communities (324 total, 8 thin omitted)

### Community 0 - "Result"
Cohesion: 0.05
Nodes (51): AuthContext, main(), MockMongoUserRepository, MockPostgresUserRepository, MongoUserRepository, PostgresSessionRepository, PostgresUserRepository, Box (+43 more)

### Community 1 - "jwt.rs"
Cohesion: 0.06
Nodes (48): Algorithm, Claims, JwtAlgorithm, JwtKey, JwtService, KeyMaterial, KeyRotationConfig, make_test_claims() (+40 more)

### Community 2 - "AlertEvent"
Cohesion: 0.06
Nodes (47): AlertAggregator, DatabaseAlertCallback, main(), MetricsExporter, PerformanceAnalyzer, QueueMonitor, Arc, Box (+39 more)

### Community 3 - "RabbitMQQueueSimple"
Cohesion: 0.05
Nodes (32): batch_process_webhooks(), main(), monitor_webhook_processing(), process_incoming_webhook(), process_priority_webhooks(), retry_failed_webhook(), Box, Error (+24 more)

### Community 4 - "RedisCache"
Cohesion: 0.08
Nodes (42): LargeData, LoadTestConfig, main(), NestedData, OperationType, PerformanceMetrics, PerformanceTestSuite, Box (+34 more)

### Community 5 - "backbone-storage/src/compression.rs"
Cohesion: 0.06
Nodes (45): CompressionAlgorithm, CompressionConfig, CompressionQuality, CompressionResult, deserialize(), detect_category_from_extension(), detect_file_category(), DocumentCompressionConfig (+37 more)

### Community 6 - "Error"
Cohesion: 0.09
Nodes (25): CrudService, AdapterError, CrudServiceAdapter, CrudServiceAdapter<R, E, C, U>, Arc, C, E, Error (+17 more)

### Community 7 - "MemoryCache"
Cohesion: 0.06
Nodes (43): AdvancedCacheManager, ApiRequest, CachedResponse, CacheStrategy, main(), ProfileData, Box, DateTime (+35 more)

### Community 8 - "ModuleRegistry"
Cohesion: 0.05
Nodes (33): BackboneModule, MigrationInfo, CircularA, CircularB, ModuleRegistry, ModuleRegistryError, Arc, Default (+25 more)

### Community 9 - "Result"
Cohesion: 0.08
Nodes (27): and_conditions(), build_update_parts(), bulk_update_rows(), company_fence(), fetch_by_ids_as_json(), GenericCrudRepository, GenericCrudRepository<T, D>, GenericCrudRepository<T, HardDelete> (+19 more)

### Community 10 - "JobError"
Cohesion: 0.06
Nodes (26): JobError, Error, From, Self, QueueError, ConfigValidator, ErrorHandler, AlertThresholds (+18 more)

### Community 11 - "IntegrationEventBus"
Cohesion: 0.07
Nodes (31): CountingHandler, DeadLetterEntry, IntegrationBusConfig, IntegrationEventBus, IntegrationEventHandler, IntegrationLoggingHandler, Arc, AtomicUsize (+23 more)

### Community 12 - "E"
Cohesion: 0.07
Nodes (43): Display, Error, Formatter, From, Result, UseCaseError, entity_validator_collects_all_errors(), EntityValidator (+35 more)

### Community 13 - "RedisQueue"
Cohesion: 0.08
Nodes (38): RedisQueue, RedisQueueBuilder, RedisQueueConfig, Client, Default, Into, Option, QueueResult (+30 more)

### Community 14 - "backbone-maintenance/src/lib.rs"
Cohesion: 0.06
Nodes (58): AtomicBool, admin_toggle_handler(), apply_update_estimated_end_at_empty_string_clears(), apply_update_toggle_off_clears_started_at(), apply_update_toggle_on_stamps_started_at(), build_503(), constant_time_eq(), default_config_has_safe_allow_paths() (+50 more)

### Community 15 - "SecurityEngine"
Cohesion: 0.08
Nodes (35): default_config(), from_env(), require_env_non_empty(), Box, StorageConfig, StorageResult, test_default_config(), test_require_env_non_empty_blank() (+27 more)

### Community 16 - "cqrs.rs"
Cohesion: 0.06
Nodes (51): Command, CommandDispatcher, CommandHandler, C, Error, Result, Send, Sync (+43 more)

### Community 17 - "usecase.rs"
Cohesion: 0.09
Nodes (42): create_use_case_persists_entity(), CreateFooDto, CreateUseCase, CreateUseCase<E, DTO, S>, DefaultHooks, DefaultHooks<E, DTO>, delete_use_case_removes_entity(), DeleteUseCase (+34 more)

### Community 18 - "E"
Cohesion: 0.07
Nodes (31): all_of_denies_when_any_denies(), AllOfPolicy, AllOfPolicy<E>, any_of_permits_when_one_permits(), AnyOfPolicy, AnyOfPolicy<E>, deny_all_always_denies_with_reason(), DenyAllPolicy (+23 more)

### Community 19 - "StorageFile"
Cohesion: 0.07
Nodes (37): AccessControlConfig, AccessPolicy, ByteRange, CorsConfig, EncryptionAlgorithm, EncryptionConfig, FileMetadata, GcsStorageConfig (+29 more)

### Community 20 - "QueueResult"
Cohesion: 0.08
Nodes (30): DeduplicationCache, DeduplicationCacheBackend, DeduplicationConfig, DeduplicationEntry, DeduplicationStats, DeduplicationStrategy, ExactlyOnceRecord, ExactlyOnceStorage (+22 more)

### Community 21 - "http.rs"
Cohesion: 0.05
Nodes (38): BatchIdsRequest, bulk_patch_request_parses_per_item_shape(), bulk_patch_request_parses_shared_shape(), BulkCreateRequest, BulkPatchItem, BulkPatchRequest, BulkResponse, camel_to_snake_case() (+30 more)

### Community 22 - "JobId"
Cohesion: 0.06
Nodes (22): JobExecutionAttempt, JobExecutionContext, JobExecutionResult, JobId, JobPriority, JobStatus, RetryPolicy, DateTime (+14 more)

### Community 23 - "LocalStorage"
Cohesion: 0.09
Nodes (23): LocalStorage, LocalStorageBuilder, AsyncRead, AsyncWrite, Box, Bytes, Default, HashMap (+15 more)

### Community 24 - "String"
Cohesion: 0.07
Nodes (37): AuthContext, AuthExtractor, AuthMiddleware, Self, Vec, CacheConfig, CacheError, CacheKey (+29 more)

### Community 25 - "GenericCrudService<E, C, U, R>"
Cohesion: 0.13
Nodes (19): check_batch_size(), first_duplicate_id(), GenericCrudService<E, C, U, R>, InMemoryWidgetRepo, C, Clone, CrudRepository, E (+11 more)

### Community 26 - "T"
Cohesion: 0.08
Nodes (36): AlwaysFalse, AlwaysFalse<T>, AlwaysTrue, AlwaysTrue<T>, AndSpecification, AndSpecification<L, R, T>, IsActive, NotSpecification (+28 more)

### Community 27 - "ElasticsearchSearch"
Cohesion: 0.09
Nodes (15): ElasticsearchConfig, ElasticsearchSearch, ElasticsearchSearchBuilder, Default, HashMap, Into, Option, Result (+7 more)

### Community 28 - "TaskService"
Cohesion: 0.11
Nodes (21): demonstrate_error_handling(), main(), DateTime, Entity, HashMap, Mutex, Option, Result (+13 more)

### Community 29 - "EmailAddress"
Cohesion: 0.09
Nodes (18): EmailAddress, EmailAttachment, EmailMessage, EmailMessageBuilder, EmailPriority, EmailRecipients, EmailTemplate, DateTime (+10 more)

### Community 30 - "SeedManager"
Cohesion: 0.07
Nodes (17): DateTime, Option, PgPool, Result, Self, Utc, Value, Vec (+9 more)

### Community 31 - "Result"
Cohesion: 0.11
Nodes (30): GeoLocation, InMemorySessionRepository, main(), Arc, Box, DateTime, Error, HashMap (+22 more)

### Community 32 - "ECommerceService"
Cohesion: 0.13
Nodes (21): demonstrate_ecommerce_workflow(), ECommerceService, main(), Order, OrderItem, OrderStatus, Product, Review (+13 more)

### Community 33 - "QueueMessage"
Cohesion: 0.06
Nodes (21): create_test_message_with_priority(), BatchReceiveResult, MessageStatus, QueueMessage, QueueMessageBuilder, QueuePriority, DateTime, Default (+13 more)

### Community 34 - "testing_examples.rs"
Cohesion: 0.09
Nodes (53): create_mock_search_response(), create_mock_search_service(), create_test_documents(), e2e_testing_examples(), integration_testing_examples(), latency_testing_example(), load_testing_example(), main() (+45 more)

### Community 35 - "AlgoliaSearch"
Cohesion: 0.10
Nodes (14): AlgoliaClient, AlgoliaConfig, AlgoliaSearch, AlgoliaSearchBuilder, AlgoliaSearchOptions, Default, HashMap, Into (+6 more)

### Community 36 - "MailgunEmailService"
Cohesion: 0.09
Nodes (24): MailgunConfig, MailgunEmailService, MailgunEmailTracking, MailgunRegion, MailgunResponse, MailgunServiceBuilder, Arc, Client (+16 more)

### Community 37 - "LocalStorage"
Cohesion: 0.09
Nodes (23): detect_mime_type(), LocalStorage, LocalStorageBuilder, md5_compute(), AsyncRead, AsyncWrite, Box, Bytes (+15 more)

### Community 38 - "ServiceContainer"
Cohesion: 0.08
Nodes (25): Any, Application, ApplicationBuilder, Module, ModuleError, Arc, AtomicU32, Box (+17 more)

### Community 39 - "InMemoryRepository<E>"
Cohesion: 0.10
Nodes (20): InMemoryRepository, InMemoryRepository<E>, CrudRepository, DateTime, Default, E, HashMap, Option (+12 more)

### Community 40 - "ServiceRegistry"
Cohesion: 0.09
Nodes (30): HealthStatus, ModuleService, RegistryHealth, RegistryStatistics, Arc, DateTime, Default, HashMap (+22 more)

### Community 41 - "QueueManager"
Cohesion: 0.10
Nodes (23): create_sample_maintenance_result(), MaintenanceAction, MaintenanceResult, QueueAdminService, QueueConfig, QueueManager, AlertThresholds, Arc (+15 more)

### Community 42 - "S3Storage"
Cohesion: 0.11
Nodes (21): sanitize_filename(), AsyncRead, AsyncWrite, Box, Bytes, Client, Display, HashMap (+13 more)

### Community 43 - "bulk.rs"
Cohesion: 0.08
Nodes (29): bulk_create_aborts_on_first_error(), bulk_create_collects_errors_in_continue_mode(), bulk_create_rejects_oversized_batch(), BulkCapableService, BulkFailureMode, BulkItemResult, BulkItemResult<E>, BulkOperationConfig (+21 more)

### Community 44 - "ConfigurationBus"
Cohesion: 0.08
Nodes (22): ConfigChangeEvent, ConfigurationBus, ConfigValue, Arc, DateTime, Default, From, HashMap (+14 more)

### Community 45 - "ServiceResult"
Cohesion: 0.10
Nodes (26): create_and_query_roundtrip(), CreateItemInput, FakeItemService, GenericGraphQLResolver, GenericGraphQLResolver<E, C, U, S>, GraphQLCapableService, GraphQLListResult, GraphQLPaginationInput (+18 more)

### Community 46 - "company_scope.rs"
Cohesion: 0.14
Nodes (48): admin_pool(), app_pool(), bind_company(), bind_company_on(), bind_current_company(), current_company(), dsn(), execute_scoped() (+40 more)

### Community 47 - "Backbone Cache"
Cohesion: 0.04
Nodes (48): 🔧 Advanced Usage, Architecture, Async/Await Support, Backbone Cache, Basic Usage, Cache Decorator Pattern, Cache Warming, Configuration Options (+40 more)

### Community 48 - "Backbone Search"
Cohesion: 0.04
Nodes (48): 🙏 Acknowledgments, Advanced Search, Algolia Backend, Algolia Integration, Analytics and Monitoring, API Key Management, Architecture Principles, Backbone Search (+40 more)

### Community 49 - "Backbone Auth"
Cohesion: 0.04
Nodes (47): 🔧 Advanced Usage, API Integration Example, 📚 API Reference, Application Configuration (YAML), 🏗️ Architecture, 🔐 Authentication, 🔑 Authorization (RBAC), Backbone Auth (+39 more)

### Community 50 - "SmtpEmailService"
Cohesion: 0.10
Nodes (21): Arc, DateTime, Default, EmailResult, Error, HashMap, Into, Message (+13 more)

### Community 51 - "EventEnvelope"
Cohesion: 0.10
Nodes (25): EventBus, EventBus<E>, EventBusConfig, Arc, Clone, DateTime, Default, E (+17 more)

### Community 52 - "MessageCompressor"
Cohesion: 0.10
Nodes (20): CompressedMessageBuilder, CompressionAlgorithm, CompressionConfig, CompressionError, CompressionStats, crate::QueueError, estimate_compression_ratio(), get_compression_recommendations() (+12 more)

### Community 53 - "User"
Cohesion: 0.12
Nodes (16): main(), DateTime, Entity, HashMap, Mutex, Option, Result, Self (+8 more)

### Community 54 - "SqsQueue"
Cohesion: 0.13
Nodes (13): Default, HashMap, Into, Message, Option, QueueResult, Self, Vec (+5 more)

### Community 55 - "Backbone Storage"
Cohesion: 0.04
Nodes (46): 1. File Upload Service, 2. Backup Service, 3. File Processing Pipeline, 4. Storage Monitoring, 📊 **Advanced Features**, Backbone Storage, Basic Configuration, Basic File Operations (+38 more)

### Community 56 - "RepositoryError"
Cohesion: 0.11
Nodes (20): PostgresRepository, PostgresRepository<E>, PostgresRepositoryBuilder, PostgresRepositoryBuilder<E>, Clone, CrudRepository, Default, E (+12 more)

### Community 57 - "Backbone Email"
Cohesion: 0.04
Nodes (45): 1. Welcome Email Service, 2. Bulk Email Newsletter, 3. Email Service Health Check, Advanced Configuration, Backbone Email, Basic Configuration, Basic Email Sending, Common SMTP Providers (+37 more)

### Community 58 - "SesEmailService"
Cohesion: 0.11
Nodes (21): Arc, DateTime, Default, EmailResult, Error, HashMap, Into, Message (+13 more)

### Community 59 - "HealthChecker"
Cohesion: 0.09
Nodes (17): HealthChecker, HealthCheckerBuilder, Arc, Box, Clone, Debug, Default, Duration (+9 more)

### Community 60 - "Self"
Cohesion: 0.09
Nodes (22): ActionExecutor, event_matches(), Arc, C, Default, HashMap, Into, Option (+14 more)

### Community 61 - "JobSchedulerBuilder"
Cohesion: 0.14
Nodes (18): create_high_throughput_scheduler(), create_in_memory_scheduler(), create_postgres_scheduler(), create_production_scheduler(), JobSchedulerBuilder, JobSchedulerExt, Arc, Default (+10 more)

### Community 62 - "MigrationManager"
Cohesion: 0.09
Nodes (13): Migration, MigrationFile, MigrationManager, MigrationRecord, MigrationStatus, DateTime, Option, PgPool (+5 more)

### Community 63 - "cron.rs"
Cohesion: 0.11
Nodes (26): CronExpression, CronField, CronScheduler, describe_expression(), describe_field(), is_valid_expression(), Box, DateTime (+18 more)

### Community 64 - "modules_config.rs"
Cohesion: 0.10
Nodes (31): BucketConfig, default_bucket_context(), default_domain_version(), default_hash_length(), default_iterations(), default_lockout_duration(), default_max_attempts(), default_memory() (+23 more)

### Community 65 - "utils.rs"
Cohesion: 0.10
Nodes (27): BackboneError, datetime_to_prost_timestamp(), days_ago(), days_from_now(), hours_ago(), hours_from_now(), is_future(), is_past() (+19 more)

### Community 66 - "Backbone-ORM Examples"
Cohesion: 0.05
Nodes (41): 1. Connection Pool Configuration, 2. Batch Operations, 3. Query Optimization, Advanced Querying, Aggregation and GROUP BY, Backbone-ORM Examples, Basic CRUD Operations, Basic Filtering (+33 more)

### Community 67 - "FifoQueueServiceWrapper"
Cohesion: 0.12
Nodes (21): FifoQueueConfig, FifoQueueService, FifoQueueServiceWrapper, FifoQueueStats, get_recommended_config(), MessageGroupStats, MessageVolume, Arc (+13 more)

### Community 68 - "SecureUserDatabase"
Cohesion: 0.11
Nodes (20): AdvancedSecurityService, main(), Box, Error, HashMap, Instant, Option, Result (+12 more)

### Community 69 - "GrpcResponse"
Cohesion: 0.11
Nodes (27): BackboneGrpcService, GenericGrpcService, GenericGrpcService<E, C, U, S>, GrpcBulkCreateRequest, GrpcBulkResponse, GrpcCapableService, GrpcConfig, GrpcListResponse (+19 more)

### Community 70 - "MonitoringService"
Cohesion: 0.13
Nodes (27): ComponentHealth, health_check_handler(), HealthCheckResponse, HealthStatus, HistoryQuery, Metrics, metrics_handler(), metrics_history_handler() (+19 more)

### Community 71 - "subscriber.rs"
Cohesion: 0.10
Nodes (21): EventHandler, Send, Sync, CountingHandler, FakeEvent, GenericEventSubscriber, GenericEventSubscriber<Event>, registry_dispatches_to_all_subscribers() (+13 more)

### Community 72 - "InMemoryStore<T>"
Cohesion: 0.10
Nodes (16): InMemoryStore, InMemoryStore<T>, Arc, Default, HashMap, Option, RwLock, Self (+8 more)

### Community 73 - "MinIOStorage"
Cohesion: 0.10
Nodes (18): MinIOStorage, AsyncRead, AsyncWrite, Box, Bytes, HashMap, Item, Option (+10 more)

### Community 74 - "ApiResponse"
Cohesion: 0.14
Nodes (18): demonstrate_advanced_pagination(), main(), Product, ProductService, DateTime, Entity, HashMap, Mutex (+10 more)

### Community 75 - "MockQueueService"
Cohesion: 0.11
Nodes (14): CallbackJobExecutor, DefaultJobExecutionCallback, JobExecutor, MockQueueService, Arc, Clone, Default, Duration (+6 more)

### Community 76 - "backbone-observability/src/middleware.rs"
Cohesion: 0.08
Nodes (31): extract_path_template(), HttpStatus, looks_like_id(), ObservabilityLayer, ObservabilityMiddleware, ObservabilityMiddleware<S>, record_http_metrics(), Response<B> (+23 more)

### Community 77 - "fifo_tests.rs"
Cohesion: 0.09
Nodes (29): create_test_fifo_message(), MockQueueService, Arc, Mutex, Option, QueueResult, Self, Value (+21 more)

### Community 78 - "backbone-search/src/types.rs"
Cohesion: 0.13
Nodes (34): Aggregation, AggregationType, DocumentMetadata, FacetBucket, FacetConfig, FacetResult, FacetSort, FacetType (+26 more)

### Community 79 - "crud_event.rs"
Cohesion: 0.10
Nodes (25): aggregate_id_roundtrips(), CrudEvent, CrudEvent<E>, CrudEventPublisher, event_type_names_are_stable(), EventMetadata, FakeEntity, meta() (+17 more)

### Community 80 - "PostgresRepository<T>"
Cohesion: 0.09
Nodes (20): DatabaseOperations, Entity, FilterCondition, FilterParams, PaginationInfo, PaginationParams, PostgresRepository, PostgresRepository<T> (+12 more)

### Community 81 - "Self"
Cohesion: 0.26
Nodes (11): JsonOrForm, BackboneCrudHandler<S, E, C, U, R>, batch_size_error(), Arc, C, IntoResponse, Path, Response (+3 more)

### Community 82 - "messaging_tests.rs"
Cohesion: 0.09
Nodes (18): CollectingIntegrationHandler, FailingIntegrationHandler, Arc, AtomicU32, Self, Vec, test_concurrent_publishing(), test_config_disables_dead_letter() (+10 more)

### Community 83 - "backbone-search/src/traits.rs"
Cohesion: 0.14
Nodes (34): Analyzer, BulkError, BulkOperation, BulkOperationResult, BulkOperationType, BulkResult, CountPoint, DynamicMapping (+26 more)

### Community 84 - "backbone-core/src/integration.rs"
Cohesion: 0.10
Nodes (23): A, adapter_maps_fields_correctly(), EventBridge, EventBridge<External, Internal, A>, ExternalUserEvent, identity_adapter(), identity_adapter_roundtrips(), IdentityAdapter (+15 more)

### Community 85 - "backbone-core/src/service.rs"
Cohesion: 0.15
Nodes (25): bulk_partial_update_success(), bulk_restore_and_restore_all(), bulk_soft_delete_is_all_or_nothing_on_missing_id(), bulk_soft_delete_rejects_oversized_batch(), bulk_soft_delete_success(), bulk_soft_delete_tolerates_duplicate_ids(), bulk_update_is_all_or_nothing_on_missing_id(), bulk_update_rejects_duplicate_ids() (+17 more)

### Community 86 - "JobSchedulerConfig"
Cohesion: 0.10
Nodes (21): AlertThresholds, DatabaseConfig, JobSchedulerConfig, LoggingConfig, MonitoringConfig, QueueConfig, AlertThresholds, DatabaseConfig (+13 more)

### Community 87 - "Self"
Cohesion: 0.12
Nodes (8): Default, Error, Into, Result, Self, SearchDocumentBuilder, SearchQueryBuilder, SortOrder

### Community 88 - "backbone-storage/src/traits.rs"
Cohesion: 0.12
Nodes (34): AlertSeverity, AccessPolicy, AlertSeverity, AlertThresholds, BackupResult, BucketConfig, BucketInfo, FileListResult (+26 more)

### Community 89 - "Job"
Cohesion: 0.11
Nodes (16): Job, DateTime, JobResult, Option, Utc, test_job_retry_logic(), test_success_rate(), cache_warming() (+8 more)

### Community 90 - "logging.rs"
Cohesion: 0.11
Nodes (29): create_log_entry(), create_request_id(), format_timestamp(), get_aggregated_logs(), get_hostname(), init_structured_logging(), LogBuffer, LogEntry (+21 more)

### Community 91 - "filter/tests.rs"
Cohesion: 0.08
Nodes (17): parse_filters(), test_audit_metadata_does_not_affect_other_fields(), test_audit_metadata_rewrite_bracket(), test_audit_metadata_rewrite_orderby(), test_audit_metadata_rewrite_simple_equality(), test_enum_normalization_already_snake_case(), test_enum_normalization_bracket_notation(), test_enum_normalization_does_not_affect_builtin_types() (+9 more)

### Community 92 - "AuthService"
Cohesion: 0.16
Nodes (13): AuthResult, AuthService, AuthServiceConfig, email_regex(), Default, Option, Result, Self (+5 more)

### Community 93 - "Backbone Core Examples"
Cohesion: 0.06
Nodes (33): 1. **Basic Usage** (`basic_usage.rs`), 2. **Advanced Pagination** (`advanced_pagination.rs`), 3. **E-commerce Scenario** (`scenario_ecommerce.rs`), 4. **Error Handling** (`error_handling.rs`), Adding Custom Fields, Adding New Examples, 📚 Additional Resources, Async Database Operations (+25 more)

### Community 94 - "EventError"
Cohesion: 0.10
Nodes (19): EventError, Error, From, Into, Self, NoOpEventBus, NoOpPublisher, NoOpPublisher<E> (+11 more)

### Community 95 - "SimpleUser"
Cohesion: 0.11
Nodes (17): AuthContext, AuthResultEnhanced, PasswordPolicy, PasswordResetConfirmation, PasswordResetRequest, DateTime, Default, Option (+9 more)

### Community 96 - "backbone-authorization/src/types.rs"
Cohesion: 0.11
Nodes (18): Action, AuthorizationRequest, AuthorizationResponse, AuthUser, PermissionAction, PermissionCheck, Resource, Role (+10 more)

### Community 97 - "FlowInstance"
Cohesion: 0.11
Nodes (20): FlowError, FlowExecutor, FlowExecutor<H>, FlowInstance, FlowInstance<S>, FlowStatus, Arc, DateTime (+12 more)

### Community 98 - "ComponentStatus"
Cohesion: 0.16
Nodes (12): ComponentStatus, HealthReport, HealthStatus, HealthSummary, DateTime, Default, Duration, HashMap (+4 more)

### Community 99 - "QueryValue"
Cohesion: 0.13
Nodes (9): QueryBuilder, QueryValue, NaiveDateTime, Option, PgPool, Result, Self, Uuid (+1 more)

### Community 100 - "orm_tests.rs"
Cohesion: 0.09
Nodes (16): MockPool, Entity, NaiveDateTime, Option, Self, Uuid, test_complete_entity_lifecycle(), test_entity_serialization() (+8 more)

### Community 101 - "compression_tests.rs"
Cohesion: 0.12
Nodes (23): create_large_payload(), create_test_message(), Value, test_compress_force_compression(), test_compress_message_gzip(), test_compress_message_none_algorithm(), test_compress_message_zlib(), test_compress_small_message_below_threshold() (+15 more)

### Community 102 - "BackboneConfig"
Cohesion: 0.11
Nodes (16): BackboneConfig, ConfigResult, DatabaseConfig, Default, HashMap, LoggingConfig, MonitoringConfig, Option (+8 more)

### Community 103 - "JobBuilder"
Cohesion: 0.14
Nodes (13): JobBuilder, Default, HashMap, Into, RetryPolicy, S, Self, Value (+5 more)

### Community 104 - "Backbone Search Examples"
Cohesion: 0.06
Nodes (31): 1. [Basic Usage](./basic_usage.rs), 2. [Advanced Search](./advanced_search.rs), 3. [Elasticsearch Setup](./elasticsearch_setup.rs), 4. [Algolia Setup](./algolia_setup.rs), 5. [Testing Examples](./testing_examples.rs), 🔗 Additional Resources, Advanced (1-2 months), Advanced Query Pattern (+23 more)

### Community 105 - "ErrorHandlingService"
Cohesion: 0.19
Nodes (19): ComplexData, ErrorHandlingService, main(), NestedStruct, Box, DateTime, Error, Instant (+11 more)

### Community 106 - "repository_tests.rs"
Cohesion: 0.08
Nodes (4): Entity, NaiveDateTime, Option, TestUser

### Community 107 - "RateLimitConfig"
Cohesion: 0.10
Nodes (17): RateLimiter<B>, B, Self, RateLimitMiddleware<B>, RateLimitMiddleware<crate::redis_storage::RedisStorage>, RateLimitMiddleware<InMemoryStorage>, Result, Self (+9 more)

### Community 108 - "docs/architecture.md"
Cohesion: 0.14
Nodes (16): ADR-0002: Self-describing crates; no workspace dependency inheritance, Alternatives considered, Consequences, Context, Decision, ADR-0003: Protocol-agnostic core with pluggable backends, Alternatives considered, Consequences (+8 more)

### Community 109 - "RabbitMQQueue"
Cohesion: 0.14
Nodes (15): AMQPValue, ConnectionPool, RabbitMQQueue, Arc, Channel, Connection, Consumer, Mutex (+7 more)

### Community 110 - "AuthorizationService"
Cohesion: 0.17
Nodes (13): AuthorizationService, Arc, Default, HashMap, HashSet, Result, RwLock, Self (+5 more)

### Community 111 - "schema.rs"
Cohesion: 0.11
Nodes (16): ConfigValidationError, BucketConfig, ConfigResult, DatabaseConfig, LoggingConfig, Result, SecurityConfig, validate_bucket_module() (+8 more)

### Community 112 - "backbone-core/src/error.rs"
Cohesion: 0.09
Nodes (17): CommonError, ErrorCategory, ErrorResponse, ModuleError, Debug, Display, E, Error (+9 more)

### Community 113 - "Backbone Jobs"
Cohesion: 0.07
Nodes (30): Advanced Examples, AWS SQS Queue, Backbone Jobs, Common Patterns, 🤝 Contributing, 🕐 Cron Expressions, Custom Metrics, 🔍 Error Handling (+22 more)

### Community 114 - "cache.rs"
Cohesion: 0.14
Nodes (21): CachedEntry, CacheError, InMemoryPermissionCache, PermissionCacheBackend, Arc, DateTime, Default, HashMap (+13 more)

### Community 115 - "backbone-authorization/src/middleware.rs"
Cohesion: 0.10
Nodes (21): AuthorizationLayer, AuthorizationMiddleware, AuthorizationMiddleware<S>, AuthState, is_public_endpoint(), Arc, Clone, Context (+13 more)

### Community 116 - "JobScheduler"
Cohesion: 0.19
Nodes (9): JobScheduler, Clone, DateTime, HashSet, JobResult, Option, Utc, Vec (+1 more)

### Community 117 - "processor_demo.rs"
Cohesion: 0.19
Nodes (21): create_processing_context(), create_processing_context_with_attempt(), CustomOrderProcessor, demo_batch_processing(), demo_batching_processor(), demo_custom_processor(), demo_individual_processing(), demo_performance_monitoring() (+13 more)

### Community 118 - "EcommerceCacheService"
Cohesion: 0.28
Nodes (11): EcommerceCacheService, main(), ProductCategory, Box, Error, Option, Result, Self (+3 more)

### Community 119 - "Backbone Core"
Cohesion: 0.07
Nodes (28): 1. Add Dependency, 2. Define Your Entity, 3. Implement HTTP Handler, 4. Implement gRPC Service, 📊 Advanced CRUD Operations, Atomic batch endpoints, Backbone Core, 🤝 Contributing (+20 more)

### Community 120 - "company.rs"
Cohesion: 0.12
Nodes (22): company_auth(), CompanyClaims, CompanyContext, CompanyVerifier, internal_error(), Arc, Error, Next (+14 more)

### Community 121 - "backbone-observability/src/audit.rs"
Cohesion: 0.12
Nodes (20): StatusCode, audit_middleware(), AuditConfig, classify_event(), extract_client_ip(), extract_user_agent(), ip_falls_through_to_real_ip_when_xff_blank(), ip_falls_through_to_real_ip_when_xff_missing() (+12 more)

### Community 122 - "JobSchedulerBuilder"
Cohesion: 0.16
Nodes (14): JobExecutionCallback, Send, Sync, JobStorage, Send, Sync, JobSchedulerBuilder, Arc (+6 more)

### Community 123 - "PgCronManager"
Cohesion: 0.14
Nodes (13): PgCronJobInfo, PgCronManager, PgCronStatistics, DateTime, JobResult, Option, PgPool, Self (+5 more)

### Community 124 - "MonitoringDashboard"
Cohesion: 0.17
Nodes (16): AlertThresholds, main(), MonitoringDashboard, QueueMetrics, Arc, Box, Default, Error (+8 more)

### Community 125 - "monitoring_tests.rs"
Cohesion: 0.31
Nodes (26): create_test_monitor(), create_test_queue(), Box, Error, Result, test_alert_event_creation(), test_alert_thresholds_default(), test_console_alert_callback() (+18 more)

### Community 127 - "Result"
Cohesion: 0.18
Nodes (14): main(), MockSecurityService, MockUserDatabase, Box, Error, HashMap, Option, Result (+6 more)

### Community 128 - "InMemoryJobStorage"
Cohesion: 0.16
Nodes (11): InMemoryJobStorage, Arc, DateTime, Default, HashMap, JobResult, Option, RwLock (+3 more)

### Community 129 - "metrics.rs"
Cohesion: 0.15
Nodes (22): MetricsConfig, MetricsExporterType, init_metrics(), init_prometheus(), metrics_handler(), metrics_server_task(), MetricsError, MetricsHandle (+14 more)

### Community 131 - "RabbitMQ Integration Guide"
Cohesion: 0.08
Nodes (26): 📚 Advanced Examples, Advanced Routing with Headers, Basic Routing, 🔧 Configuration Validation, Connection Failures, Connection Pooling, Dead Letter Exchange Setup, Direct Exchange (+18 more)

### Community 132 - "Glossary — Ubiquitous Language"
Cohesion: 0.08
Nodes (26): AccessScope, Atomic batch endpoint, Backbone Framework, `BackboneCrudHandler`, Crate (project type), `CrudService`, Entity, Feature (Cargo feature flag) (+18 more)

### Community 133 - "Result"
Cohesion: 0.18
Nodes (13): ApiClient, Client, Option, Result, RwLock, User, Uuid, test_api_error_serialization() (+5 more)

### Community 134 - "CustomHealthCheck"
Cohesion: 0.12
Nodes (16): CustomHealthCheck, MockHealthCheck, Box, Duration, F, Future, HealthResult, Output (+8 more)

### Community 135 - "sqs_tests.rs"
Cohesion: 0.20
Nodes (24): create_mock_sqs_client(), create_test_config(), create_test_message(), create_test_message_with_priority(), create_test_sqs_queue(), Client, QueueResult, test_sqs_batch_message_processing() (+16 more)

### Community 136 - "cache_tests.rs"
Cohesion: 0.17
Nodes (17): CacheResult, Self, test_cache_complex_data_types(), test_cache_concurrent_access(), test_cache_entry_access_tracking(), test_cache_entry_creation(), test_cache_error_handling(), test_memory_cache_basic_operations() (+9 more)

### Community 137 - "QueueWorker"
Cohesion: 0.16
Nodes (17): main(), ProcessResult, QueueWorker, Arc, Box, Default, Duration, Error (+9 more)

### Community 138 - "MockQueue"
Cohesion: 0.16
Nodes (6): MockQueue, Option, QueueResult, Vec, test_get_queue_stats(), QueueHealth

### Community 139 - "integration_tests.rs"
Cohesion: 0.29
Nodes (22): create_test_message_with_id(), create_test_redis_queue(), create_test_sqs_queue(), Box, Default, Error, Result, Self (+14 more)

### Community 140 - "tests/registry.rs"
Cohesion: 0.15
Nodes (18): a_failed_build_does_not_poison_the_slot(), builds_once_then_serves_from_cache(), capacity_evicts_the_least_recently_used(), concurrent_first_requests_build_once(), distinct_tenants_get_distinct_runtimes(), evict_idle_drops_only_stale_tenants(), eviction_does_not_kill_an_in_flight_holder(), FakeRuntime (+10 more)

### Community 141 - "database_config.rs"
Cohesion: 0.17
Nodes (16): CacheConfig, DatabaseConfig, default_cache_driver(), default_cache_max_connections(), default_cache_ttl(), default_cache_url(), default_connect_timeout(), default_idle_timeout() (+8 more)

### Community 142 - "LogLine"
Cohesion: 0.20
Nodes (5): LogLine, DateTime, Option, Utc, Thing

### Community 143 - "CollectingHandler<E>"
Cohesion: 0.13
Nodes (9): CollectingHandler, CollectingHandler<E>, LoggingHandler, Arc, E, Result, RwLock, Self (+1 more)

### Community 144 - "IntegrationEvent"
Cohesion: 0.13
Nodes (11): IntegrationEvent, Clone, Deserialize, Send, Serialize, Sync, OrderPaid, DateTime (+3 more)

### Community 145 - "tracing.rs"
Cohesion: 0.13
Nodes (18): LoggingConfig, ObservabilityConfig, Default, LoggingConfig, Option, Self, Vec, build_otel_tracer() (+10 more)

### Community 146 - ".substitute_env_vars"
Cohesion: 0.18
Nodes (12): ConfigLoader, ConfigResult, Option, P, Path, PathBuf, test_env_specific_path(), test_from_yaml_str() (+4 more)

### Community 147 - "Query"
Cohesion: 0.14
Nodes (17): CacheableQuery, PaginatedQuery, PaginatedQueryResult, PaginatedQueryResult<T>, Query, QueryDispatcher, QueryHandler, Error (+9 more)

### Community 148 - "Backbone-ORM"
Cohesion: 0.09
Nodes (21): Advanced Querying, Backbone-ORM, Basic CRUD Operations, Comprehensive testing approach:, 🤝 Contributing, 🔗 Dependencies, Developer Experience, 🚀 Features (+13 more)

### Community 149 - "security_config.rs"
Cohesion: 0.21
Nodes (16): CsrfConfig, default_cors_headers(), default_cors_methods(), default_cors_origins(), default_csrf_expires(), default_csrf_token_length(), default_hsts(), default_x_content_type_options() (+8 more)

### Community 150 - "backbone-core/src/lib.rs"
Cohesion: 0.15
Nodes (10): BaseEntity, Entity, DateTime, Default, Deserialize, Option, Self, Serialize (+2 more)

### Community 151 - "health_tests.rs"
Cohesion: 0.28
Nodes (17): Box, Error, Result, test_component_status_lifecycle(), test_consecutive_failure_threshold(), test_custom_health_check(), test_health_checker_builder(), test_health_checker_component_management() (+9 more)

### Community 152 - "DomainEvent"
Cohesion: 0.10
Nodes (11): TestEvent, DomainEvent, Clone, DeserializeOwned, Send, Serialize, Sync, SerializableEvent (+3 more)

### Community 153 - ".from_event"
Cohesion: 0.17
Nodes (12): DateTime, E, Error, Into, Result, Self, Utc, test_envelope_deserialize() (+4 more)

### Community 154 - "Backbone Queue Examples"
Cohesion: 0.10
Nodes (21): Additional Resources, AWS SQS Setup, Backbone Queue Examples, Best Practices, Common Requirements, Connection Issues, Contributing, Custom Alert Thresholds (+13 more)

### Community 155 - "SearchStats"
Cohesion: 0.10
Nodes (12): DateTime, Default, HashMap, Option, Self, Utc, Value, SearchBackend (+4 more)

### Community 156 - "AppState"
Cohesion: 0.15
Nodes (20): AppState, AuthResponse, axum_login(), jwt_middleware(), login(), LoginRequest, protected_resource(), Arc (+12 more)

### Community 157 - "ConfigError"
Cohesion: 0.23
Nodes (9): ConfigError, Error, From, P, PathBuf, S, Self, test_error_display() (+1 more)

### Community 158 - "value_object.rs"
Cohesion: 0.15
Nodes (17): Option<T>, OptionalValueObject, Clone, Debug, Error, Result, Send, Sync (+9 more)

### Community 159 - "EmailServiceStats"
Cohesion: 0.15
Nodes (13): EmailQueue, EmailService, EmailServiceStats, DateTime, HashMap, Option, Send, Sync (+5 more)

### Community 160 - "SimpleHealthServer"
Cohesion: 0.19
Nodes (9): Arc, HealthResult, Self, SimpleHealthServer, test_detailed_health_json_generation(), test_health_json_generation(), test_simple_health_server_creation(), BufWriter (+1 more)

### Community 161 - "QueueStats"
Cohesion: 0.11
Nodes (8): QueueBackend, QueueConfig, QueueStats, Default, HashMap, Option, Self, Value

### Community 162 - "RabbitMQConfig"
Cohesion: 0.21
Nodes (11): AckMode, dev_config(), ExchangeType, prod_config(), QosConfig, RabbitMQConfig, Default, HashMap (+3 more)

### Community 163 - "api_integration.rs"
Cohesion: 0.16
Nodes (18): ApiError, auth_middleware(), AuthError, jwt_validator(), main(), RegisterRequest, Box, Clone (+10 more)

### Community 164 - "real_world_scenarios.rs"
Cohesion: 0.20
Nodes (17): ApiRateLimit, CartItem, CartSummary, NotificationSettings, Product, DateTime, HashMap, UserPreferences (+9 more)

### Community 165 - "📡 Backbone Core — API Reference"
Cohesion: 0.11
Nodes (19): `ApiResponse<T>` — single resource, 📡 Backbone Core — API Reference, Bulk create — `POST {base}/bulk`, Bulk full update — `PUT {base}/bulk`, Bulk partial update — `PATCH {base}/bulk` (two auto-detected shapes), `BulkResponse<T>`, 🧭 Endpoint catalogue, Field-level security (`@private` / `@owner`) (+11 more)

### Community 166 - "Self"
Cohesion: 0.22
Nodes (6): EventEnvelope<E>, EventEnvelopeBuilder<E>, E, Into, Option, Self

### Community 167 - "dual.rs"
Cohesion: 0.25
Nodes (17): always_fallback_falls_back_in_any_env(), apply_fallback(), build(), DualStorage, empty_redis_url_returns_in_memory_silently(), fallback_in_dev_errors_in_prod(), fallback_in_dev_falls_back_when_dev(), FallbackPolicy (+9 more)

### Community 168 - "StorageError"
Cohesion: 0.16
Nodes (9): Error, From, Into, Option, Response, Self, StorageError, DecodeError (+1 more)

### Community 169 - "developer-guide.md"
Cohesion: 0.22
Nodes (10): 🗂️ Application configuration (`config` module), ⚙️ Backbone Core — Configuration, 🧩 Cargo feature matrix, Limits (public constants), Mount path, 🎛️ Runtime knobs, 📚 Backbone Core — Documentation, 🧩 Feature flags (+2 more)

### Community 170 - "TestAggregate"
Cohesion: 0.23
Nodes (7): DateTime, Event, Option, Utc, Vec, test_aggregate_events(), TestAggregate

### Community 171 - "RawQueryBuilder"
Cohesion: 0.30
Nodes (7): RawQuery, RawQueryBuilder, Option, PgPool, Result, Vec, WindowFunction

### Community 173 - "company_guard.rs"
Cohesion: 0.23
Nodes (15): a_bare_token_without_the_bearer_prefix_is_rejected(), app(), call(), guarded_handler(), Option, Response, Router, Uuid (+7 more)

### Community 174 - "RateLimitResult"
Cohesion: 0.21
Nodes (5): RateLimitResult, now_unix(), RedisStorage, Self, StorageBackend

### Community 175 - "ServerConfig"
Cohesion: 0.18
Nodes (11): default_host(), default_keep_alive(), default_port(), default_read_timeout(), default_shutdown_timeout(), default_write_timeout(), Default, Duration (+3 more)

### Community 176 - "PersistentEntity"
Cohesion: 0.18
Nodes (16): CrudRepository, PartialUpdatable, PersistentEntity, PostgresEntity, Clone, Debug, DeserializeOwned, E (+8 more)

### Community 177 - "CrudRepository"
Cohesion: 0.18
Nodes (13): BulkRepository, CrudRepository, DomainPaginatedResult, DomainPaginationParams, PaginatedRepository, Repository, E, Self (+5 more)

### Community 179 - "FilterValue"
Cohesion: 0.18
Nodes (7): FilterOperator, FilterValue, Option, Self, Vec, SortDirection, SortSpec

### Community 180 - "company_fence.rs"
Cohesion: 0.14
Nodes (6): EntityRepoMeta, Fenced, Global, HashMap, Option, tf6_fence_ands_with_the_soft_delete_guard()

### Community 182 - "rabbitmq_realtime_chat.rs"
Cohesion: 0.35
Nodes (16): chat_room_management(), ChatMessage, ChatRoom, main(), message_history(), private_messaging(), real_time_messaging(), Box (+8 more)

### Community 184 - "InMemoryStorage"
Cohesion: 0.17
Nodes (11): CounterState, InMemoryStorage, Arc, Default, HashMap, Option, RwLock, Self (+3 more)

### Community 185 - ".get_or_build"
Cohesion: 0.13
Nodes (8): RegistryError, E, Error, F, Result, S, Self, TenantRegistry<F>

### Community 186 - "PasswordService"
Cohesion: 0.23
Nodes (6): Argon2, PasswordService, PasswordValidator, Default, Result, Self

### Community 187 - "auth_service_tests.rs"
Cohesion: 0.17
Nodes (7): Result, test_auth_service_creation(), test_auth_service_with_secret(), test_complete_auth_flow(), test_jwt_validation(), test_password_hashing(), test_permission_service_flow()

### Community 188 - ".get_handler"
Cohesion: 0.36
Nodes (11): AccessScope, apply_field_security(), include_relations(), is_bad_query_error(), ListQueryParams, pagination_depth_error(), Option, Uuid (+3 more)

### Community 189 - "routes.rs"
Cohesion: 0.31
Nodes (15): checker(), detailed_returns_200_with_report_body(), health(), health_detailed(), health_returns_200_with_status_body(), health_routes(), livez(), livez_returns_200() (+7 more)

### Community 190 - "TenantId"
Cohesion: 0.39
Nodes (7): From, TenantId, ProvisionError, Error, PgConnection, Result, TenantProvisioner

### Community 191 - "Changelog"
Cohesion: 0.12
Nodes (16): [2.0.0], [2.2.0], [2.2.1], [2.2.2], [2.3.0], Added, Added, Added (+8 more)

### Community 192 - "Environment"
Cohesion: 0.16
Nodes (10): AppConfig, Environment, Default, Display, Err, Formatter, FromStr, Option (+2 more)

### Community 193 - "Widget"
Cohesion: 0.26
Nodes (7): ApplyUpdateDto, CreateWidgetDto, FromCreateDto, DateTime, Sized, Utc, Widget

### Community 194 - "main"
Cohesion: 0.50
Nodes (14): basic_direct_exchange_example(), batch_operations_example(), fanout_exchange_example(), health_monitoring_example(), main(), message_with_headers_example(), microservices_communication_example(), priority_message_example() (+6 more)

### Community 195 - "BatchingProcessor"
Cohesion: 0.25
Nodes (10): BatchConfig, BatchingProcessor, BatchTimeoutPolicy, MessageProcessor, RetryPolicy, Arc, Default, RwLock (+2 more)

### Community 197 - "deduplication_demo.rs"
Cohesion: 0.37
Nodes (10): demo_cleanup_operations(), demo_content_deduplication(), demo_deduplication_statistics(), demo_exactly_once_processing(), demo_message_id_deduplication(), main(), Box, Error (+2 more)

### Community 198 - "InMemoryPermissionService<R>"
Cohesion: 0.25
Nodes (4): InMemoryPermissionService<R>, R, Result, Vec

### Community 199 - "RedisPermissionCache"
Cohesion: 0.33
Nodes (6): RedisPermissionCache, HashSet, Option, Result, Self, CacheError

### Community 200 - "crud_macro_compile.rs"
Cohesion: 0.19
Nodes (8): _batch_signatures(), LogRepo, HashMap, ThingRepo, HardDelete, SoftDelete, Deref, Target

### Community 201 - "filter_bench.rs"
Cohesion: 0.30
Nodes (13): bench_build_order_by_clause(), bench_build_where_clause(), bench_filter_operator_from_str(), bench_parse_filters(), bench_parse_filters_with_whitelist(), bench_query_builder_build_sql(), bench_sanitize_field_name(), make_column_types() (+5 more)

### Community 202 - "FilterCondition"
Cohesion: 0.22
Nodes (7): FilterCondition, FilterOperator, FilterValue, Option, Self, Vec, FilterLogical

### Community 203 - "QueryFilter"
Cohesion: 0.21
Nodes (5): QueryFilter, FilterCondition, Option, Self, Vec

### Community 204 - "Usage"
Cohesion: 0.14
Nodes (14): Advanced Configuration, AWS SQS, AWS SQS Queue, Backend Selection Guide, Quick Start Examples, RabbitMQ, RabbitMQ Configuration, RabbitMQ Exchange Types (+6 more)

### Community 205 - "Backbone Queue Module"
Cohesion: 0.14
Nodes (14): Backbone Queue Module, Between Backends, Contributing, Error Handling, License, Migration Guide, Performance, Priority Levels (+6 more)

### Community 207 - "rate_limit_middleware"
Cohesion: 0.27
Nodes (12): RateLimiter, extract_key(), from_config(), new(), rate_limit_middleware(), RateLimitMiddleware, Arc, B (+4 more)

### Community 208 - "backbone-cache/examples/basic_usage.rs"
Cohesion: 0.15
Nodes (8): main(), Box, DateTime, Error, Result, Utc, Session, User

### Community 209 - "🧾 Backbone Core — OpenAPI / Swagger Guide"
Cohesion: 0.15
Nodes (13): 1. Enable the feature, 2. What core provides, 3. Annotate your entity and paths, 4. Aggregate into one document, 🧾 Backbone Core — OpenAPI / Swagger Guide, Caveats, No UI — raw document route, Path A — generate with utoipa (+5 more)

### Community 210 - "🚀 Backbone Core — Usage"
Cohesion: 0.15
Nodes (13): 1. Add the dependency, 2. Define your entity, 3. Define DTOs, 4. Get a service, 5. Mount the router, 6. Call it, 7. Feature flags for examples, 🚀 Backbone Core — Usage (+5 more)

### Community 211 - "features_config.rs"
Cohesion: 0.24
Nodes (8): default_burst_limit(), default_rate_limit(), default_rate_storage(), FeaturesConfig, RateLimitingConfig, Default, Option, Self

### Community 212 - "init_observability"
Cohesion: 0.21
Nodes (11): get_aggregated_logs(), init_observability(), init_structured_logging(), OtelShutdownGuard, Drop, Error, Option, Result (+3 more)

### Community 213 - "OutboxRow"
Cohesion: 0.24
Nodes (11): drain_all(), drain_once(), OutboxRow, DateTime, F, Option, PgPool, Result (+3 more)

### Community 214 - "main"
Cohesion: 0.49
Nodes (12): benchmark_compression(), create_large_payload(), create_small_payload(), main(), Box, Error, Result, Value (+4 more)

### Community 215 - "fifo_queue_demo.rs"
Cohesion: 0.41
Nodes (12): create_activity_message(), create_fifo_message(), create_order_message(), demo_cleanup_operations(), demo_configuration_recommendations(), demo_group_statistics(), demo_message_deduplication(), main() (+4 more)

### Community 216 - "Technical Details"
Cohesion: 0.15
Nodes (13): Configuration Reference, Core Architecture, Dependencies, Error Handling Strategy, Latency (P99), Message Flow, Performance Characteristics, QueueService Trait (+5 more)

### Community 217 - "BatchProcessingResult"
Cohesion: 0.23
Nodes (6): BatchProcessingResult, DateTime, Duration, QueueResult, Utc, Vec

### Community 218 - "backbone-tenant/src/lib.rs"
Cohesion: 0.23
Nodes (12): Entry, Inner, Arc, HashMap, Mutex, R, Send, Sync (+4 more)

### Community 219 - "Developer Guide"
Cohesion: 0.15
Nodes (13): Configuration, Developer Guide, How do I expand a related object (relation expansion)?, How do I paginate, filter, and sort a list?, How do I restrict field visibility (`@private` / `@owner`)?, How do I return only some fields (sparse fieldsets)?, How do I serve an OpenAPI / Swagger spec?, Install (+5 more)

### Community 220 - "The reasoning"
Cohesion: 0.15
Nodes (13): Axum + Tower / tower-http — the HTTP adapter, Deeper reasoning, Rust, edition 2021 — *the whole premise*, Serde family — one serialization model, three formats, SQLx — persistence, and why it is *optional* in core, Technology & the "Why", The cross-cutting rule: features gate the weight, The reasoning (+5 more)

### Community 221 - "SimplePermission"
Cohesion: 0.35
Nodes (3): Option, SimplePermission, SimpleRole

### Community 222 - "🏛️ Backbone Core — Architecture"
Cohesion: 0.17
Nodes (12): 400 vs 500 classification, 🏛️ Backbone Core — Architecture, 📐 Guard rails (constants), 🧱 Layers, Lifecycle hooks & events, Module map (`src/`), 🔁 Request lifecycle, Route precedence (+4 more)

### Community 223 - "OutboxRecord"
Cohesion: 0.29
Nodes (8): OutboxRecord, DateTime, Into, Option, Self, Utc, Uuid, Value

### Community 224 - "SimpleMessageProcessor"
Cohesion: 0.35
Nodes (3): Into, Self, SimpleMessageProcessor

### Community 225 - "main"
Cohesion: 0.33
Nodes (11): analytics_example(), complex_filters_example(), faceted_search_example(), geospatial_search_example(), main(), multi_index_search_example(), Box, Error (+3 more)

### Community 226 - "main"
Cohesion: 0.55
Nodes (11): advanced_settings_example(), authentication_examples(), basic_setup_example(), cluster_configuration_example(), create_product_mapping(), index_configuration_example(), main(), performance_optimization_example() (+3 more)

### Community 227 - "aggregate.rs"
Cohesion: 0.27
Nodes (7): AggregateMetadata, AggregateRoot, EventSourcedAggregate, InvariantAggregate, Self, test_aggregate_metadata(), TestEvent

### Community 228 - "runner.rs"
Cohesion: 0.24
Nodes (9): RelayConfig, Duration, F, Into, PgPool, Result, S, Self (+1 more)

### Community 229 - "mechanics.rs"
Cohesion: 0.49
Nodes (10): fresh_schema(), m1_stage_atomic_with_tx(), m2_stage_idempotent(), m3_relay_drains_and_retries(), m4_inbox_once_dedups_per_consumer(), m5_crash_window_exactly_once_draw(), m6_relay_does_not_deadlock_a_reborrowing_consumer(), pool() (+2 more)

### Community 230 - "ProcessedMessage"
Cohesion: 0.31
Nodes (7): BatchContext, ProcessedMessage, ProcessingContext, ProcessingOutcome, Instant, Option, create_test_processed_message()

### Community 231 - "main"
Cohesion: 0.53
Nodes (9): demo_comprehensive_validation(), demo_environment_specific_validation(), demo_invalid_configuration(), demo_valid_configuration(), demonstrate_validation_error_examples(), main(), Box, Error (+1 more)

### Community 232 - "backbone-rate-limit"
Cohesion: 0.18
Nodes (10): Advanced Example: User-Based Rate Limiting, backbone-rate-limit, Basic Example, Configuration, Features, Hard Lockout, Installation, License (+2 more)

### Community 233 - "main"
Cohesion: 0.62
Nodes (10): analytics_and_testing_example(), authentication_examples(), basic_setup_example(), index_configuration_example(), main(), performance_optimization_example(), Box, Error (+2 more)

### Community 234 - "main"
Cohesion: 0.33
Nodes (10): algolia_example(), bulk_operations_example(), demonstrate_error_handling(), elasticsearch_example(), main(), Box, Error, Result (+2 more)

### Community 235 - "Metaphor Crate"
Cohesion: 0.18
Nodes (10): Anti-patterns, Common tasks, Deeper knowledge (load on demand), Folder cheatsheet, Golden path, graphify, Key files to read before editing, Metaphor Crate (+2 more)

### Community 236 - ".new"
Cohesion: 0.24
Nodes (7): axum_protected(), Json, Request, Self, Value, run_actix_server(), run_axum_server()

### Community 237 - "EmailConfig"
Cohesion: 0.20
Nodes (7): EmailConfig, EmailError, EmailProvider, Default, Option, Self, SmtpConfig

### Community 238 - "email_service_tests.rs"
Cohesion: 0.31
Nodes (6): Result, test_email_config_default(), test_email_message_builder(), test_email_message_validation(), test_smtp_config_validation(), test_smtp_service_creation()

### Community 239 - "main"
Cohesion: 0.49
Nodes (9): demonstrate_cron_patterns(), format_duration(), main(), Box, Duration, Error, Result, schedule_real_world_jobs() (+1 more)

### Community 240 - "main"
Cohesion: 0.56
Nodes (9): demo_advanced_configuration(), demo_maintenance_operations(), demo_queue_config_creation(), demo_queue_management(), main(), Box, Error, Result (+1 more)

### Community 241 - "Background & Prior Art"
Cohesion: 0.20
Nodes (10): Background & Prior Art, Django REST Framework / API Platform — generated CRUD, Laravel / Rails — convention over configuration, Prior art, and what Backbone takes from it, Rust building blocks it stands on — not reinvented, Spring Boot — batteries included, one runtime, The direct ancestor: `monorepo-backbone`, The problem being solved (+2 more)

### Community 242 - "backbone-auth/examples/basic_usage.rs"
Cohesion: 0.33
Nodes (8): main(), Box, Error, Result, test_basic_auth_flow(), test_permission_system(), test_role_management(), test_wildcard_permissions()

### Community 243 - "permissions.rs"
Cohesion: 0.39
Nodes (8): InMemoryPermissionService, PermissionChecker, PermissionLike, RoleLike, Clone, HashMap, Send, Sync

### Community 244 - "cli.rs"
Cohesion: 0.42
Nodes (8): empty_healthcheck_url_does_not_override_port(), healthcheck_url(), probe(), Duration, HealthResult, run_healthcheck(), url_falls_back_to_port_env_then_default(), url_uses_explicit_env_when_set()

### Community 245 - "main"
Cohesion: 0.67
Nodes (8): main(), Box, Error, Result, schedule_archiving_jobs(), schedule_cleanup_jobs(), schedule_optimization_jobs(), show_job_summary()

### Community 246 - "ConnectionManager"
Cohesion: 0.33
Nodes (4): ConnectionManager, PgPool, Result, Self

### Community 247 - "parser.rs"
Cohesion: 0.25
Nodes (8): audit_metadata_sql_expr(), is_custom_enum_type(), normalize_enum_value(), HashMap, HashSet, Option, Result, to_snake_case()

### Community 248 - "validate_schema"
Cohesion: 0.36
Nodes (8): Result, validate_schema(), migrate(), pending_count(), E, PgPool, Result, stage()

### Community 249 - "ProcessorStats"
Cohesion: 0.28
Nodes (3): ProcessorStats, HashMap, Value

### Community 250 - "run_all_tests"
Cohesion: 0.31
Nodes (7): Box, Error, Result, run_all_tests(), test_config_parsing(), test_in_memory_backend(), test_redis_backend()

### Community 251 - ".build"
Cohesion: 0.33
Nodes (4): PgPoolFactory, Into, PgPool, Self

### Community 252 - "Maintainer Guide"
Cohesion: 0.22
Nodes (9): Before you touch anything, Maintainer Guide, Recipe: add a new backend to an infrastructure crate, Recipe: add a whole new crate to the workspace, Recipe: add an optional capability to an existing crate, The four rules that are never negotiable, Versioning & release, What will break things (+1 more)

### Community 253 - "Philosophy & Motivation"
Cohesion: 0.22
Nodes (9): 1. Lift-and-shift discipline, 2. Protocol-agnostic core, 3. Pluggable backends, 4. Consistency by generation, not by discipline, Philosophy & Motivation, The worldview, What Backbone deliberately is *not* (non-goals), Where to go next (+1 more)

### Community 255 - "redis_cache_tests.rs"
Cohesion: 0.57
Nodes (7): HashSet, setup_cache(), test_permissions(), test_redis_clear(), test_redis_delete(), test_redis_get_set(), test_redis_ttl_expiry()

### Community 256 - "domain_service.rs"
Cohesion: 0.32
Nodes (5): DomainService, Send, Sync, TestService, TransactionalDomainService

### Community 257 - ".from_request"
Cohesion: 0.29
Nodes (7): JsonOrForm<T>, Rejection, Request, Result, S, Self, FromRequest

### Community 258 - "HealthConfig"
Cohesion: 0.25
Nodes (6): HealthConfig, HealthError, Default, Duration, Option, Self

### Community 259 - "redis_integration.rs"
Cohesion: 0.39
Nodes (5): test_config(), test_redis_custom_prefix(), test_redis_storage_get_count(), test_redis_storage_increment(), test_redis_storage_reset()

### Community 260 - "provision.rs"
Cohesion: 0.43
Nodes (7): injection_shaped_ids_are_rejected(), over_length_names_are_rejected(), provisioner(), render_dsn(), Option, tenant_dsn_swaps_the_database(), valid_slugs_produce_prefixed_names()

### Community 261 - "Contribution Guide"
Cohesion: 0.25
Nodes (8): Contribution Guide, Conventions that gate a PR, Dev setup, PR checklist, Releasing (maintainers), Review expectations, Tests & lint before you push, Where your change goes

### Community 262 - "Backbone Framework"
Cohesion: 0.25
Nodes (8): Backbone Framework, Building, Getting Started, License, Philosophy, Repository Layout, Versioning & Releases, Workspace Crates

### Community 263 - "MonitoringConfig"
Cohesion: 0.29
Nodes (4): MonitoringConfig, Default, Option, Self

### Community 264 - "state_machine.rs"
Cohesion: 0.29
Nodes (6): Display, Sized, StateMachineBehavior, StateMachineError, TransitionMeta, Copy

### Community 265 - "raw_query.rs"
Cohesion: 0.38
Nodes (3): CteClause, JoinClause, JoinType

### Community 266 - "once"
Cohesion: 0.38
Nodes (6): once(), E, PgPool, Result, Uuid, was_consumed()

### Community 267 - "Advanced Examples"
Cohesion: 0.29
Nodes (7): 1. Basic Redis Queue, 2. SQS Queue Examples, 3. Worker Pool, 4. Monitoring Dashboard, Advanced Examples, Basic Examples, Running Examples

### Community 268 - "Message Creation"
Cohesion: 0.29
Nodes (7): Basic Message, FIFO Message (SQS), JSON Payload, Message Creation, Message with Expiration, Message with Headers, RabbitMQ Message with Routing

### Community 269 - "QueueService"
Cohesion: 0.57
Nodes (6): MessageProcessor, QueueManager, QueueMonitor, QueueService, Send, Sync

### Community 270 - "Architecture"
Cohesion: 0.29
Nodes (7): 1. Context, 2. Containers — the crates, 3. Components / modules — inside `backbone-core`, 4. Data & control flow — a `GET /api/v1/{collection}` request, Architecture, Key decisions, The 17 crates

### Community 271 - ".new"
Cohesion: 0.53
Nodes (3): InMemoryPermissionService<SimpleRole>, Default, Self

### Community 272 - "BackboneCrudHandler"
Cohesion: 0.33
Nodes (6): BackboneCrudHandler, BulkUpdateItem, E, PhantomData, R, U

### Community 273 - "filter/validation.rs"
Cohesion: 0.47
Nodes (5): FilterableEntity, is_valid_field(), HashSet, Result, sanitize_field_name()

### Community 274 - "📋 Best Practices"
Cohesion: 0.33
Nodes (6): 1. Use Appropriate Exchange Types, 2. Implement Proper Error Handling, 3. Monitor Performance, 4. Security Considerations, 5. Resource Management, 📋 Best Practices

### Community 275 - "RetryConfig"
Cohesion: 0.47
Nodes (3): RetryConfig, RetryHandler, RetryPolicy

### Community 276 - "UserRepository"
Cohesion: 0.50
Nodes (5): AuthenticatableUser, Clone, Send, Sync, UserRepository

### Community 278 - "backbone-graphql/src/error.rs"
Cohesion: 0.60
Nodes (4): not_found_error(), E, Error, service_error()

### Community 279 - "main"
Cohesion: 0.40
Nodes (4): main(), Box, Error, Result

### Community 280 - "main"
Cohesion: 0.40
Nodes (4): main(), Box, Error, Result

### Community 281 - "🔧 Advanced Usage"
Cohesion: 0.40
Nodes (5): 🔧 Advanced Usage, Custom Job Configuration, Job Lifecycle Management, Job Statistics and Monitoring, Scheduler Configuration

### Community 282 - "OutboxError"
Cohesion: 0.40
Nodes (4): OutboxError, Error, ok_publish(), Result

### Community 283 - "relay_runner.rs"
Cohesion: 0.80
Nodes (4): fresh_schema(), pool(), PgPool, runner_delivers_then_stops()

### Community 284 - "main"
Cohesion: 0.40
Nodes (4): main(), Box, Error, Result

### Community 285 - "main"
Cohesion: 0.40
Nodes (4): main(), Box, Error, Result

### Community 286 - "Queue Operations"
Cohesion: 0.40
Nodes (5): Basic Operations, Batch Operations, Health Monitoring, Queue Management, Queue Operations

### Community 287 - "Best Practices"
Cohesion: 0.40
Nodes (5): Best Practices, Error Handling, Message Design, Monitoring, Resource Management

### Community 288 - "Common Issues"
Cohesion: 0.40
Nodes (5): Common Issues, Large Message Errors, Redis Connection Errors, SQS Permission Errors, Troubleshooting

### Community 289 - "ADR-0001: Distribute via git tags, not crates.io"
Cohesion: 0.40
Nodes (5): ADR-0001: Distribute via git tags, not crates.io, Alternatives considered, Consequences, Context, Decision

### Community 290 - "AuthorizationServiceTrait"
Cohesion: 0.50
Nodes (3): AuthorizationServiceTrait, Send, Sync

### Community 291 - "ListRequest"
Cohesion: 0.50
Nodes (3): ListRequest, Default, SortOrder

### Community 292 - "Projector"
Cohesion: 0.50
Nodes (3): Projector, Send, Sync

### Community 295 - "🔧 Best Practices"
Cohesion: 0.50
Nodes (4): 🔧 Best Practices, Cron Expressions, Job Design, Performance

### Community 296 - "🏢 Production Deployment"
Cohesion: 0.50
Nodes (4): Configuration, Docker Deployment, Kubernetes Deployment, 🏢 Production Deployment

### Community 297 - "🔐 Security Best Practices"
Cohesion: 0.50
Nodes (4): Access Control, Authentication, 🔐 Security Best Practices, TLS/SSL Configuration

### Community 298 - "🔄 Message Patterns"
Cohesion: 0.50
Nodes (4): 🔄 Message Patterns, Producer-Consumer Pattern, Publish-Subscribe Pattern, Request-Reply Pattern

### Community 299 - "Example Output"
Cohesion: 0.50
Nodes (4): Basic Redis Queue, Example Output, Monitoring Dashboard, Worker Pool

### Community 300 - "Advanced Usage"
Cohesion: 0.50
Nodes (4): Advanced Usage, Dead Letter Queue Handling, Message Compression, Visibility Timeouts

### Community 301 - "Overview"
Cohesion: 0.50
Nodes (4): Architecture, Key Features, Overview, Supported Backends

### Community 302 - "Backend Implementations"
Cohesion: 0.50
Nodes (4): AWS SQS Backend, Backend Implementations, RabbitMQ Backend, Redis Backend

### Community 303 - "Real-World Examples"
Cohesion: 0.50
Nodes (4): Background Job Processing, Microservices Communication, Real-World Examples, Webhook Processing

### Community 304 - "Testing"
Cohesion: 0.50
Nodes (4): Integration Tests, Test Coverage, Testing, Unit Tests

### Community 305 - "Backbone Framework — Handbook"
Cohesion: 0.50
Nodes (4): Backbone Framework — Handbook, Per-crate documentation, Start here by who you are, The whole handbook

### Community 306 - "BackboneHttpHandler"
Cohesion: 0.67
Nodes (3): BackboneHttpHandler, Send, Sync

### Community 307 - "📋 Predefined Job Types"
Cohesion: 0.67
Nodes (3): Available Predefined Jobs, 📋 Predefined Job Types, Using Predefined Jobs

### Community 308 - "🔧 Configuration Options"
Cohesion: 0.67
Nodes (3): Advanced Queue Arguments, 🔧 Configuration Options, Connection Settings

### Community 309 - "🚀 Quick Start"
Cohesion: 0.67
Nodes (3): Basic Setup, Production Setup, 🚀 Quick Start

### Community 310 - "🐛 Troubleshooting"
Cohesion: 0.67
Nodes (3): Common Issues, Debug Mode, 🐛 Troubleshooting

### Community 311 - "📊 Monitoring & Health Checks"
Cohesion: 0.67
Nodes (3): Configuration Validation, Health Monitoring, 📊 Monitoring & Health Checks

### Community 312 - "Monitoring and Metrics"
Cohesion: 0.67
Nodes (3): Health Check, Monitoring and Metrics, Queue Statistics

### Community 313 - "SearchService"
Cohesion: 0.67
Nodes (3): Send, Sync, SearchService

## Knowledge Gaps
- **644 isolated node(s):** `AuthMiddleware`, `AuthExtractor`, `SecurityAlertType`, `GraphQLListResult<E>`, `GrpcService` (+639 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **8 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `String` connect `String` to `Result`, `jwt.rs`, `AlertEvent`, `RabbitMQQueueSimple`, `RedisCache`, `backbone-storage/src/compression.rs`, `Error`, `MemoryCache`, `ModuleRegistry`, `Result`, `JobError`, `IntegrationEventBus`, `E`, `RedisQueue`, `backbone-maintenance/src/lib.rs`, `SecurityEngine`, `cqrs.rs`, `usecase.rs`, `E`, `StorageFile`, `QueueResult`, `http.rs`, `JobId`, `LocalStorage`, `GenericCrudService<E, C, U, R>`, `T`, `ElasticsearchSearch`, `TaskService`, `EmailAddress`, `SeedManager`, `Result`, `ECommerceService`, `QueueMessage`, `testing_examples.rs`, `AlgoliaSearch`, `MailgunEmailService`, `LocalStorage`, `ServiceContainer`, `InMemoryRepository<E>`, `ServiceRegistry`, `QueueManager`, `S3Storage`, `bulk.rs`, `ConfigurationBus`, `ServiceResult`, `company_scope.rs`, `SmtpEmailService`, `EventEnvelope`, `MessageCompressor`, `User`, `SqsQueue`, `RepositoryError`, `SesEmailService`, `HealthChecker`, `Self`, `JobSchedulerBuilder`, `MigrationManager`, `cron.rs`, `modules_config.rs`, `utils.rs`, `FifoQueueServiceWrapper`, `SecureUserDatabase`, `GrpcResponse`, `MonitoringService`, `subscriber.rs`, `InMemoryStore<T>`, `MinIOStorage`, `ApiResponse`, `MockQueueService`, `backbone-observability/src/middleware.rs`, `fifo_tests.rs`, `backbone-search/src/types.rs`, `crud_event.rs`, `PostgresRepository<T>`, `Self`, `backbone-search/src/traits.rs`, `backbone-core/src/integration.rs`, `backbone-core/src/service.rs`, `JobSchedulerConfig`, `Self`, `backbone-storage/src/traits.rs`, `Job`, `logging.rs`, `filter/tests.rs`, `AuthService`, `EventError`, `SimpleUser`, `backbone-authorization/src/types.rs`, `FlowInstance`, `ComponentStatus`, `QueryValue`, `orm_tests.rs`, `BackboneConfig`, `JobBuilder`, `ErrorHandlingService`, `repository_tests.rs`, `RateLimitConfig`, `RabbitMQQueue`, `AuthorizationService`, `schema.rs`, `backbone-core/src/error.rs`, `cache.rs`, `backbone-authorization/src/middleware.rs`, `processor_demo.rs`, `EcommerceCacheService`, `company.rs`, `backbone-observability/src/audit.rs`, `JobSchedulerBuilder`, `PgCronManager`, `MonitoringDashboard`, `Result`, `metrics.rs`, `Result`, `CustomHealthCheck`, `cache_tests.rs`, `QueueWorker`, `MockQueue`, `integration_tests.rs`, `tests/registry.rs`, `database_config.rs`, `LogLine`, `IntegrationEvent`, `tracing.rs`, `.substitute_env_vars`, `Query`, `security_config.rs`, `DomainEvent`, `.from_event`, `SearchStats`, `AppState`, `ConfigError`, `value_object.rs`, `EmailServiceStats`, `SimpleHealthServer`, `QueueStats`, `RabbitMQConfig`, `api_integration.rs`, `real_world_scenarios.rs`, `Self`, `StorageError`, `TestAggregate`, `RawQueryBuilder`, `company_guard.rs`, `RateLimitResult`, `ServerConfig`, `FilterValue`, `company_fence.rs`, `AdvancedQueryBuilder`, `rabbitmq_realtime_chat.rs`, `InMemoryStorage`, `.get_or_build`, `PasswordService`, `.get_handler`, `TenantId`, `Environment`, `Widget`, `InMemoryPermissionService<R>`, `RedisPermissionCache`, `crud_macro_compile.rs`, `filter_bench.rs`, `FilterCondition`, `QueryFilter`, `rate_limit_middleware`, `backbone-cache/examples/basic_usage.rs`, `features_config.rs`, `OutboxRow`, `BatchProcessingResult`, `SimplePermission`, `OutboxRecord`, `SimpleMessageProcessor`, `aggregate.rs`, `runner.rs`, `mechanics.rs`, `ProcessedMessage`, `EmailConfig`, `main`, `permissions.rs`, `cli.rs`, `parser.rs`, `ProcessorStats`, `.build`, `backbone-auth/src/audit.rs`, `HealthConfig`, `provision.rs`, `state_machine.rs`, `raw_query.rs`, `BackboneCrudHandler`, `filter/validation.rs`, `ApiResponse<T>`, `OutboxError`, `relay_runner.rs`, `ListRequest`?**
  _High betweenness centrality (0.748) - this node is a cross-community bridge._
- **Why does `QueueMessage` connect `QueueMessage` to `RabbitMQQueueSimple`, `sqs_tests.rs`, `QueueWorker`, `MockQueue`, `integration_tests.rs`, `RedisQueue`, `QueueResult`, `String`, `MessageCompressor`, `SqsQueue`, `FifoQueueServiceWrapper`, `BatchingProcessor`, `MockQueueService`, `fifo_tests.rs`, `fifo_queue_demo.rs`, `BatchProcessingResult`, `compression_tests.rs`, `ProcessedMessage`, `RabbitMQQueue`?**
  _High betweenness centrality (0.047) - this node is a cross-community bridge._
- **Why does `extract_path_template()` connect `backbone-observability/src/middleware.rs` to `String`?**
  _High betweenness centrality (0.028) - this node is a cross-community bridge._
- **What connects `AuthMiddleware`, `AuthExtractor`, `SecurityAlertType` to the rest of the system?**
  _644 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Result` be split into smaller, more focused modules?**
  _Cohesion score 0.053019145802650956 - nodes in this community are weakly interconnected._
- **Should `jwt.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.0616729088639201 - nodes in this community are weakly interconnected._
- **Should `AlertEvent` be split into smaller, more focused modules?**
  _Cohesion score 0.05515832482124617 - nodes in this community are weakly interconnected._