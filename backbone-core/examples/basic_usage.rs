//! Basic Usage Example
//!
//! This example demonstrates the simplest way to implement Backbone CRUD
//! operations for a User entity with both HTTP and gRPC handlers.

use backbone_core::http::*;
use backbone_core::grpc::*;
use backbone_core::entity::*;
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use std::collections::HashMap;
use anyhow::Result;

// Define our User entity
#[derive(Debug, Clone, Serialize, Deserialize)]
struct User {
    id: Uuid,
    name: String,
    email: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Entity for User {
    fn id(&self) -> &Uuid { &self.id }
    fn created_at(&self) -> chrono::DateTime<chrono::Utc> { self.created_at }
    fn updated_at(&self) -> chrono::DateTime<chrono::Utc> { self.updated_at }
    fn deleted_at(&self) -> Option<chrono::DateTime<chrono::Utc>> { self.deleted_at }
}

impl User {
    fn new(name: String, email: String) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            email,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        }
    }
}

// HTTP Handler Implementation
struct UserService {
    // In a real app, this would contain database connections, etc.
    users: std::sync::Mutex<Vec<User>>,
}

impl UserService {
    fn new() -> Self {
        Self {
            users: std::sync::Mutex::new(Vec::new()),
        }
    }
}

impl BackboneHttpHandler<User> for UserService {
    fn list(&self, _request: ListRequest) -> Result<ApiResponse<Vec<User>>> {
        let users = self.users.lock().unwrap();
        let filtered_users = users.iter()
            .filter(|user| !user.is_deleted())
            .cloned()
            .collect();

        Ok(ApiResponse::success(filtered_users, Some("Users listed successfully".to_string())))
    }

    fn create(&self, request: User) -> Result<ApiResponse<User>> {
        let mut users = self.users.lock().unwrap();
        let mut new_user = request;
        new_user.id = Uuid::new_v4();
        new_user.created_at = chrono::Utc::now();
        new_user.updated_at = chrono::Utc::now();

        users.push(new_user.clone());
        Ok(ApiResponse::success(new_user, Some("User created successfully".to_string())))
    }

    fn get_by_id(&self, id: &Uuid) -> Result<ApiResponse<User>> {
        let users = self.users.lock().unwrap();
        match users.iter().find(|user| user.id == *id && !user.is_deleted()) {
            Some(user) => Ok(ApiResponse::success(user.clone(), Some("User found".to_string()))),
            None => Ok(ApiResponse::error("User not found".to_string())),
        }
    }

    fn update(&self, id: &Uuid, request: User) -> Result<ApiResponse<User>> {
        let mut users = self.users.lock().unwrap();
        match users.iter_mut().find(|user| user.id == *id && !user.is_deleted()) {
            Some(user) => {
                user.name = request.name;
                user.email = request.email;
                user.updated_at = chrono::Utc::now();
                Ok(ApiResponse::success(user.clone(), Some("User updated successfully".to_string())))
            },
            None => Ok(ApiResponse::error("User not found".to_string())),
        }
    }

    fn partial_update(&self, id: &Uuid, fields: HashMap<String, serde_json::Value>) -> Result<ApiResponse<User>> {
        let mut users = self.users.lock().unwrap();
        match users.iter_mut().find(|user| user.id == *id && !user.is_deleted()) {
            Some(user) => {
                for (field, value) in fields {
                    match field.as_str() {
                        "name" => if let Some(s) = value.as_str() { user.name = s.to_string(); },
                        "email" => if let Some(s) = value.as_str() { user.email = s.to_string(); },
                        _ => return Ok(ApiResponse::error(format!("Invalid field: {}", field))),
                    }
                }
                user.updated_at = chrono::Utc::now();
                Ok(ApiResponse::success(user.clone(), Some("User partially updated".to_string())))
            },
            None => Ok(ApiResponse::error("User not found".to_string())),
        }
    }

    fn soft_delete(&self, id: &Uuid) -> Result<ApiResponse<()>> {
        let mut users = self.users.lock().unwrap();
        match users.iter_mut().find(|user| user.id == *id && !user.is_deleted()) {
            Some(user) => {
                user.deleted_at = Some(chrono::Utc::now());
                user.updated_at = chrono::Utc::now();
                Ok(ApiResponse::success((), Some("User deleted successfully".to_string())))
            },
            None => Ok(ApiResponse::error("User not found".to_string())),
        }
    }

    fn bulk_create(&self, request: BulkCreateRequest<User>) -> Result<ApiResponse<BulkResponse<User>>> {
        let mut users = self.users.lock().unwrap();
        let mut created_users = Vec::new();
        let errors: Vec<String> = Vec::new();

        for user in request.items {
            let mut new_user = user;
            new_user.id = Uuid::new_v4();
            new_user.created_at = chrono::Utc::now();
            new_user.updated_at = chrono::Utc::now();
            created_users.push(new_user.clone());
            users.push(new_user);
        }

        let response = BulkResponse {
            items: created_users.clone(),
            total: created_users.len(),
            failed: errors.len(),
            errors,
        };

        Ok(ApiResponse::success(response, Some("Bulk create completed".to_string())))
    }

    fn upsert(&self, request: UpsertRequest<User>) -> Result<ApiResponse<User>> {
        let user = request.entity;
        let mut users = self.users.lock().unwrap();

        if let Some(existing_user) = users.iter_mut().find(|u| u.email == user.email) {
            // Update existing user
            existing_user.name = user.name;
            existing_user.updated_at = chrono::Utc::now();
            Ok(ApiResponse::success(existing_user.clone(), Some("User updated".to_string())))
        } else if request.create_if_not_exists {
            // Create new user
            let mut new_user = user;
            new_user.id = Uuid::new_v4();
            new_user.created_at = chrono::Utc::now();
            new_user.updated_at = chrono::Utc::now();
            users.push(new_user.clone());
            Ok(ApiResponse::success(new_user, Some("User created".to_string())))
        } else {
            Ok(ApiResponse::error("User not found and create_if_not_exists is false".to_string()))
        }
    }

    fn list_deleted(&self, _request: ListRequest) -> Result<ApiResponse<Vec<User>>> {
        let users = self.users.lock().unwrap();
        let deleted_users = users.iter()
            .filter(|user| user.is_deleted())
            .cloned()
            .collect();

        Ok(ApiResponse::success(deleted_users, Some("Deleted users listed".to_string())))
    }

    fn restore(&self, id: &Uuid) -> Result<ApiResponse<User>> {
        let mut users = self.users.lock().unwrap();
        match users.iter_mut().find(|user| user.id == *id && user.is_deleted()) {
            Some(user) => {
                user.deleted_at = None;
                user.updated_at = chrono::Utc::now();
                Ok(ApiResponse::success(user.clone(), Some("User restored successfully".to_string())))
            },
            None => Ok(ApiResponse::error("Deleted user not found".to_string())),
        }
    }

    fn empty_trash(&self) -> Result<ApiResponse<()>> {
        let mut users = self.users.lock().unwrap();
        let original_len = users.len();
        users.retain(|user| !user.is_deleted());
        let deleted_count = original_len - users.len();

        Ok(ApiResponse::success((), Some(format!("Permanently deleted {} users", deleted_count))))
    }

    fn get_deleted_by_id(&self, id: &Uuid) -> Result<ApiResponse<User>> {
        let users = self.users.lock().unwrap();
        match users.iter().find(|user| user.id == *id && user.is_deleted()) {
            Some(user) => Ok(ApiResponse::success(user.clone(), Some("Deleted user found".to_string()))),
            None => Ok(ApiResponse::error("Deleted user not found".to_string())),
        }
    }
}

// gRPC Service Implementation
struct UserGrpcService {
    http_service: UserService,
}

impl UserGrpcService {
    fn new() -> Self {
        Self {
            http_service: UserService::new(),
        }
    }
}

impl BackboneGrpcService<User> for UserGrpcService {
    fn list(&self, request: GrpcListRequest) -> Result<GrpcResponse<GrpcListResponse<User>>> {
        let list_request = ListRequest {
            page: Some(request.page),
            limit: Some(request.limit),
            sort_by: request.sort_by,
            sort_order: request.sort_order.map(|s| match s.as_str() {
                "desc" => SortOrder::Desc,
                _ => SortOrder::Asc,
            }),
            filters: request.filters,
        };

        match self.http_service.list(list_request) {
            Ok(api_response) => {
                let items = api_response.data.unwrap_or_default();
                let total = items.len() as u64;
                let grpc_response = GrpcListResponse {
                    items,
                    total,
                    page: request.page,
                    limit: request.limit,
                    total_pages: 1, // Simplified for example
                };
                Ok(GrpcResponse::success(grpc_response))
            },
            Err(e) => Ok(GrpcResponse::error(e.to_string())),
        }
    }

    fn create(&self, request: User) -> Result<GrpcResponse<User>> {
        match self.http_service.create(request) {
            Ok(api_response) => Ok(GrpcResponse::success(api_response.data.unwrap())),
            Err(e) => Ok(GrpcResponse::error(e.to_string())),
        }
    }

    fn get_by_id(&self, request: Uuid) -> Result<GrpcResponse<User>> {
        match self.http_service.get_by_id(&request) {
            Ok(api_response) => Ok(GrpcResponse::success(api_response.data.unwrap())),
            Err(e) => Ok(GrpcResponse::error(e.to_string())),
        }
    }

    fn update(&self, request: User) -> Result<GrpcResponse<User>> {
        let id = request.id;
        match self.http_service.update(&id, request) {
            Ok(api_response) => Ok(GrpcResponse::success(api_response.data.unwrap())),
            Err(e) => Ok(GrpcResponse::error(e.to_string())),
        }
    }

    fn partial_update(&self, request: GrpcPartialUpdateRequest) -> Result<GrpcResponse<User>> {
        let fields = request
            .fields
            .into_iter()
            .map(|(k, v)| (k, serde_json::Value::String(v)))
            .collect();
        match self.http_service.partial_update(&request.id, fields) {
            Ok(api_response) => Ok(GrpcResponse::success(api_response.data.unwrap())),
            Err(e) => Ok(GrpcResponse::error(e.to_string())),
        }
    }

    fn soft_delete(&self, request: Uuid) -> Result<GrpcResponse<()>> {
        match self.http_service.soft_delete(&request) {
            Ok(_) => Ok(GrpcResponse::success(())),
            Err(e) => Ok(GrpcResponse::error(e.to_string())),
        }
    }

    fn bulk_create(&self, request: GrpcBulkCreateRequest<User>) -> Result<GrpcResponse<GrpcBulkResponse<User>>> {
        let bulk_request = BulkCreateRequest { items: request.items };
        match self.http_service.bulk_create(bulk_request) {
            Ok(api_response) => {
                let bulk_resp = api_response.data.unwrap();
                let grpc_bulk = GrpcBulkResponse {
                    items: bulk_resp.items,
                    total: bulk_resp.total,
                    failed: bulk_resp.failed,
                    errors: bulk_resp.errors,
                };
                Ok(GrpcResponse::success(grpc_bulk))
            },
            Err(e) => Ok(GrpcResponse::error(e.to_string())),
        }
    }

    fn upsert(&self, request: GrpcUpsertRequest<User>) -> Result<GrpcResponse<User>> {
        let upsert_request = UpsertRequest {
            entity: request.entity,
            create_if_not_exists: request.create_if_not_exists,
        };
        match self.http_service.upsert(upsert_request) {
            Ok(api_response) => Ok(GrpcResponse::success(api_response.data.unwrap())),
            Err(e) => Ok(GrpcResponse::error(e.to_string())),
        }
    }

    fn list_deleted(&self, request: GrpcListRequest) -> Result<GrpcResponse<GrpcListResponse<User>>> {
        let list_request = ListRequest::default();
        match self.http_service.list_deleted(list_request) {
            Ok(api_response) => {
                let items = api_response.data.unwrap_or_default();
                let total = items.len() as u64;
                let grpc_response = GrpcListResponse {
                    items,
                    total,
                    page: request.page,
                    limit: request.limit,
                    total_pages: 1,
                };
                Ok(GrpcResponse::success(grpc_response))
            },
            Err(e) => Ok(GrpcResponse::error(e.to_string())),
        }
    }

    fn restore(&self, request: Uuid) -> Result<GrpcResponse<User>> {
        match self.http_service.restore(&request) {
            Ok(api_response) => Ok(GrpcResponse::success(api_response.data.unwrap())),
            Err(e) => Ok(GrpcResponse::error(e.to_string())),
        }
    }

    fn empty_trash(&self, _request: ()) -> Result<GrpcResponse<()>> {
        match self.http_service.empty_trash() {
            Ok(_) => Ok(GrpcResponse::success(())),
            Err(e) => Ok(GrpcResponse::error(e.to_string())),
        }
    }
}

fn main() -> Result<()> {
    println!("🦴 Backbone Core - Basic Usage Example");
    println!("=====================================");

    // Initialize services
    let http_service = UserService::new();
    let grpc_service = UserGrpcService::new();

    // Example 1: Create a user via HTTP
    println!("\n1. Creating user via HTTP...");
    let user1 = User::new("Alice Smith".to_string(), "alice@example.com".to_string());
    let create_result = http_service.create(user1)?;
    println!("✅ Created user: {:?}", create_result.data.unwrap().name);

    // Example 2: Create a user via gRPC
    println!("\n2. Creating user via gRPC...");
    let user2 = User::new("Bob Johnson".to_string(), "bob@example.com".to_string());
    let grpc_create_result = grpc_service.create(user2)?;
    println!("✅ Created user: {:?}", grpc_create_result.data.unwrap().name);

    // Example 3: List users via HTTP
    println!("\n3. Listing users via HTTP...");
    let list_result = http_service.list(ListRequest::default())?;
    let users = list_result.data.unwrap();
    println!("✅ Found {} users:", users.len());
    for user in users {
        println!("   - {} ({})", user.name, user.email);
    }

    // Example 4: List users via gRPC with pagination
    println!("\n4. Listing users via gRPC with pagination...");
    let grpc_list_request = GrpcListRequest {
        page: 1,
        limit: 10,
        sort_by: Some("name".to_string()),
        sort_order: Some("asc".to_string()),
        filters: None,
    };
    let grpc_list_result = grpc_service.list(grpc_list_request)?;
    if let Some(response_data) = grpc_list_result.data {
        let grpc_users = response_data.items;
        println!("✅ Found {} users via gRPC:", grpc_users.len());
        for user in grpc_users {
            println!("   - {} ({})", user.name, user.email);
        }
    }

    // Example 5: Partial update via HTTP
    println!("\n5. Partial update via HTTP...");
    if let Some(created_user) = http_service.create(User::new("Charlie Brown".to_string(), "charlie@example.com".to_string()))?.data {
        let mut fields = HashMap::new();
        fields.insert(
            "name".to_string(),
            serde_json::Value::String("Charlie Davis".to_string()),
        );
        let update_result = http_service.partial_update(&created_user.id, fields)?;
        println!("✅ Updated user: {:?}", update_result.data.unwrap().name);
    }

    // Example 6: Bulk create via gRPC
    println!("\n6. Bulk create via gRPC...");
    let bulk_users = vec![
        User::new("Diana Prince".to_string(), "diana@example.com".to_string()),
        User::new("Eve Wilson".to_string(), "eve@example.com".to_string()),
    ];
    let grpc_bulk_request = GrpcBulkCreateRequest { items: bulk_users };
    let bulk_result = grpc_service.bulk_create(grpc_bulk_request)?;
    if let Some(bulk_response) = bulk_result.data {
        println!("✅ Bulk created {} users", bulk_response.total);
        for user in bulk_response.items {
            println!("   - {} ({})", user.name, user.email);
        }
    }

    // Example 7: Soft delete and restore
    println!("\n7. Soft delete and restore workflow...");
    if let Some(user_to_delete) = http_service.list(ListRequest::default())?.data.and_then(|v| v.first().cloned()) {
        let user_id = user_to_delete.id;
        println!("   Deleting user: {}", user_to_delete.name);
        http_service.soft_delete(&user_id)?;

        println!("   Listing active users...");
        let active_users = http_service.list(ListRequest::default())?.data.unwrap();
        println!("   ✅ Active users: {}", active_users.len());

        println!("   Listing deleted users...");
        let deleted_users = http_service.list_deleted(ListRequest::default())?.data.unwrap();
        println!("   ✅ Deleted users: {}", deleted_users.len());

        println!("   Restoring user...");
        http_service.restore(&user_id)?;
        let active_users_after_restore = http_service.list(ListRequest::default())?.data.unwrap();
        println!("   ✅ Active users after restore: {}", active_users_after_restore.len());
    }

    println!("\n🎉 All examples completed successfully!");
    Ok(())
}