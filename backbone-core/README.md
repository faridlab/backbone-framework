# Backbone Core

**Status:** ✅ FULLY IMPLEMENTED
**Last Updated:** 2026-02-11

🦴 **Foundation for generic CRUD system with 11 standard endpoints**

Backbone Core is a foundational library that provides generic CRUD (Create, Read, Update, Delete) operations for any entity in Backbone Framework. It implements **11 standard Backbone endpoints** consistently across both HTTP REST and gRPC protocols.

## 📋 Table of Contents

- [Overview](#overview)
- [Features](#features)
- [Usage](#usage)
- [Technical Details](#technical-details)
- [Examples](#examples)
- [Testing](#testing)
- [Contributing](#contributing)

## 🎯 Overview

Backbone Core is a **protocol-agnostic, generic CRUD foundation** that enables any service to automatically get:

- ✅ **11 Standard Backbone endpoints** for every entity
- ✅ **Both HTTP REST and gRPC** protocol support
- ✅ **Pagination, filtering, and sorting**
- ✅ **Bulk operations and soft delete**
- ✅ **Type-safe generic implementations**
- ✅ **Production-ready error handling**

### The 11 Backbone Endpoints

| # | HTTP Method | HTTP Endpoint | gRPC Method | Purpose |
|---|-------------|---------------|-------------|---------|
| 1 | `GET` | `/api/v1/{collection}` | `list()` | List with pagination, filtering, sorting |
| 2 | `POST` | `/api/v1/{collection}` | `create()` | Create new entity |
| 3 | `GET` | `/api/v1/{collection}/:id` | `get_by_id()` | Get entity by ID |
| 4 | `PUT` | `/api/v1/{collection}/:id` | `update()` | Full entity update |
| 5 | `PATCH` | `/api/v1/{collection}/:id` | `partial_update()` | Partial update (selected fields) |
| 6 | `DELETE` | `/api/v1/{collection}/:id` | `soft_delete()` | Soft delete (mark as deleted) |
| 7 | `POST` | `/api/v1/{collection}/bulk` | `bulk_create()` | Create multiple entities |
| 8 | `POST` | `/api/v1/{collection}/upsert` | `upsert()` | Update or insert if not exists |
| 9 | `GET` | `/api/v1/{collection}/trash` | `list_deleted()` | List deleted entities |
| 10 | `POST` | `/api/v1/{collection}/:id/restore` | `restore()` | Restore soft-deleted entity |
| 11 | `DELETE` | `/api/v1/{collection}/empty` | `empty_trash()` | Permanently delete all trash |

## 🚀 Features

### 🔄 Protocol Agnostic
- **HTTP REST**: Standard REST endpoints with JSON responses
- **gRPC**: High-performance RPC with Protocol Buffers
- **Same interface**: Both protocols provide identical functionality

### 📊 Advanced CRUD Operations
- **Pagination**: Automatic pagination with metadata
- **Filtering**: HashMap-based field filtering
- **Sorting**: Multi-field sorting with configurable order
- **Bulk Operations**: Efficient bulk create and upsert
- **Soft Delete**: Trash management with restore functionality

### 🛡️ Type Safety
- **Generic Types**: Works with any entity implementing `Entity` trait
- **Compile-time Safety**: Catch errors at compile time, not runtime
- **Proper Error Handling**: Comprehensive `anyhow::Error` support

## 📖 Usage

### 1. Add Dependency

```toml
[dependencies]
backbone-core = "2.0.0"
```

### 2. Define Your Entity

```rust
use backbone_core::entity::*;
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct User {
    id: Uuid,
    name: String,
    email: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    deleted_at: Option<DateTime<Utc>>,
}

impl Entity for User {
    fn id(&self) -> &Uuid { &self.id }
    fn created_at(&self) -> DateTime<Utc> { self.created_at }
    fn updated_at(&self) -> DateTime<Utc> { self.updated_at }
    fn deleted_at(&self) -> Option<DateTime<Utc>> { self.deleted_at }
}
```

### 3. Implement HTTP Handler

```rust
use backbone_core::http::*;
use backbone_core::entity::*;

struct UserService;

impl BackboneHttpHandler<User> for UserService {
    fn list(&self, request: ListRequest) -> Result<ApiResponse<Vec<User>>> {
        let users = vec![]; // Fetch from database
        Ok(ApiResponse::success(users))
    }

    fn create(&self, request: User) -> Result<ApiResponse<User>> {
        Ok(ApiResponse::success(request))
    }
}
```

### 4. Implement gRPC Service

```rust
use backbone_core::grpc::*;

struct UserGrpcService;

impl BackboneGrpcService<User> for UserGrpcService {
    fn list(&self, request: GrpcListRequest) -> Result<GrpcResponse<GrpcListResponse<User>>> {
        let response = GrpcListResponse {
            items: users,
            total: users.len() as u64,
        };
        Ok(GrpcResponse::success(response))
    }
}
```

## 🔧 Technical Details

### Core Traits

#### Entity Trait
```rust
pub trait Entity: Serialize + for<'de> Deserialize<'de> {
    fn id(&self) -> &Uuid;
    fn created_at(&self) -> DateTime<Utc>;
    fn updated_at(&self) -> DateTime<Utc>;
    fn deleted_at(&self) -> Option<DateTime<Utc>>;
}
```

#### Repository Traits
- **Repository<T>** - Basic CRUD operations
- **SearchableRepository<T>** - Search and filtering
- **SoftDeletableRepository<T>** - Soft delete operations
- **PaginatedRepository<T>** - Pagination support
- **BulkRepository<T>** - Bulk operations

## 📚 Examples

### Pagination with Filtering

```rust
let list_request = ListRequest {
    page: Some(1),
    limit: Some(20),
    sort_by: Some("name".to_string()),
    filters: Some(HashMap::from([
        ("status".to_string(), "active".to_string()),
    ("role".to_string(), "admin".to_string()),
    ])),
};
```

## 🧪 Testing

### Run Tests

```bash
# Run all tests
cargo test

# Run integration tests (requires database)
cargo test --test '*integration_tests*'
```

## 🔗 Dependencies

```toml
[dependencies]
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
uuid = { version = "1.0", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1.0"
thiserror = "1.0"

# gRPC
tonic = "0.12"
prost = "0.13"

# Web Framework
axum = "0.7"
tower = "0.4"
```

## 🤝 Contributing

1. Follow existing code style
2. Write tests for new features
3. Update documentation

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

**🦴 Backbone Core - Foundation for generic CRUD operations**
