//! Comprehensive Error Handling Example
//!
//! This example demonstrates various error handling patterns
//! and best practices when working with Backbone Core.

use backbone_core::http::*;
use backbone_core::entity::*;
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use std::collections::HashMap;
use anyhow::{Result, anyhow, bail, Context};

// Define a Task entity
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Task {
    id: Uuid,
    title: String,
    description: String,
    status: TaskStatus,
    priority: TaskPriority,
    assignee_id: Option<Uuid>,
    due_date: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum TaskStatus {
    Todo,
    InProgress,
    Review,
    Done,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum TaskPriority {
    Low,
    Medium,
    High,
    Critical,
}

impl Entity for Task {
    fn id(&self) -> &Uuid { &self.id }
    fn created_at(&self) -> chrono::DateTime<chrono::Utc> { self.created_at }
    fn updated_at(&self) -> chrono::DateTime<chrono::Utc> { self.updated_at }
    fn deleted_at(&self) -> Option<chrono::DateTime<chrono::Utc>> { self.deleted_at }
}

// Custom error types for better error handling
#[derive(Debug, thiserror::Error)]
enum TaskError {
    #[error("Task not found with ID: {id}")]
    NotFound { id: Uuid },

    #[error("Invalid task status transition from {from:?} to {to:?}")]
    InvalidStatusTransition { from: TaskStatus, to: TaskStatus },

    #[error("Task title cannot be empty")]
    EmptyTitle,

    #[error("Task title too long (max {max} characters, got {actual})")]
    TitleTooLong { max: usize, actual: usize },

    #[error("Cannot assign task to non-existent user: {user_id}")]
    AssigneeNotFound { user_id: Uuid },

    #[error("Cannot edit task in {status:?} status")]
    CannotEdit { status: TaskStatus },

    #[error("Database operation failed: {operation}: {details}")]
    DatabaseError { operation: String, details: String },

    #[error("Validation failed: {field} - {message}")]
    ValidationError { field: String, message: String },

    #[error("Permission denied: {action}")]
    PermissionDenied { action: String },

    #[error("Rate limit exceeded: try again in {seconds}s")]
    RateLimitExceeded { seconds: u64 },
}

// User service to check if users exist
struct UserService {
    users: std::sync::Mutex<Vec<User>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct User {
    id: Uuid,
    name: String,
    email: String,
    is_active: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl User {
    fn new(name: String, email: String) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            email,
            is_active: true,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        }
    }
}

impl Entity for User {
    fn id(&self) -> &Uuid { &self.id }
    fn created_at(&self) -> chrono::DateTime<chrono::Utc> { self.created_at }
    fn updated_at(&self) -> chrono::DateTime<chrono::Utc> { self.updated_at }
    fn deleted_at(&self) -> Option<chrono::DateTime<chrono::Utc>> { self.deleted_at }
}

impl UserService {
    fn new() -> Self {
        let service = Self {
            users: std::sync::Mutex::new(Vec::new()),
        };

        {
            let mut users = service.users.lock().unwrap();
            users.push(User::new("John Doe".to_string(), "john@example.com".to_string()));
            users.push(User::new("Jane Smith".to_string(), "jane@example.com".to_string()));
            users.push(User::new("Bob Wilson".to_string(), "bob@example.com".to_string()));
        }

        service
    }

    fn user_exists(&self, user_id: &Uuid) -> Result<bool> {
        let users = self.users.lock().unwrap();
        let exists = users.iter()
            .any(|u| u.id == *user_id && u.is_active && !u.is_deleted());

        println!("🔍 Checking user existence: {} -> {}", user_id, exists);
        Ok(exists)
    }

    fn get_user(&self, user_id: &Uuid) -> Result<User> {
        let users = self.users.lock().unwrap();
        users.iter()
            .find(|u| u.id == *user_id && u.is_active && !u.is_deleted())
            .cloned()
            .ok_or_else(|| anyhow!("User not found: {}", user_id))
    }
}

// Task service with comprehensive error handling
struct TaskService {
    tasks: std::sync::Mutex<Vec<Task>>,
    user_service: UserService,
}

impl TaskService {
    fn new() -> Self {
        Self {
            tasks: std::sync::Mutex::new(Vec::new()),
            user_service: UserService::new(),
        }
    }

    // Validation methods with detailed errors
    fn validate_task_title(&self, title: &str) -> Result<()> {
        println!("🔍 Validating task title: '{}'", title);

        if title.trim().is_empty() {
            bail!(TaskError::EmptyTitle);
        }

        if title.len() > 200 {
            bail!(TaskError::TitleTooLong { max: 200, actual: title.len() });
        }

        if title.contains(char::is_control) {
            bail!(TaskError::ValidationError {
                field: "title".to_string(),
                message: "Title contains invalid characters".to_string(),
            });
        }

        println!("✅ Task title validation passed");
        Ok(())
    }

    fn validate_task_status_transition(&self, current: &TaskStatus, new: &TaskStatus) -> Result<()> {
        println!("🔍 Validating status transition: {:?} -> {:?}", current, new);

        let valid_transitions = match current {
            TaskStatus::Todo => vec![TaskStatus::InProgress, TaskStatus::Cancelled],
            TaskStatus::InProgress => vec![TaskStatus::Review, TaskStatus::Todo, TaskStatus::Done],
            TaskStatus::Review => vec![TaskStatus::InProgress, TaskStatus::Done],
            TaskStatus::Done => vec![],
            TaskStatus::Cancelled => vec![TaskStatus::Todo],
        };

        if !valid_transitions.contains(new) {
            bail!(TaskError::InvalidStatusTransition {
                from: current.clone(),
                to: new.clone(),
            });
        }

        println!("✅ Status transition validation passed");
        Ok(())
    }

    fn validate_assignee(&self, assignee_id: &Option<Uuid>) -> Result<()> {
        println!("🔍 Validating assignee: {:?}", assignee_id);

        if let Some(user_id) = assignee_id {
            if !self.user_service.user_exists(user_id)? {
                bail!(TaskError::AssigneeNotFound { user_id: *user_id });
            }
        }

        println!("✅ Assignee validation passed");
        Ok(())
    }

    fn validate_edit_permissions(&self, task: &Task) -> Result<()> {
        println!("🔍 Validating edit permissions for task status: {:?}", task.status);

        match task.status {
            TaskStatus::Done | TaskStatus::Cancelled => {
                bail!(TaskError::CannotEdit { status: task.status.clone() });
            },
            _ => {
                println!("✅ Edit permissions validation passed");
                Ok(())
            }
        }
    }

    // Create task with comprehensive validation
    fn create_task_with_validation(&self, title: String, description: String, priority: TaskPriority) -> Result<Task> {
        println!("\n🚀 Creating new task with validation...");

        // Validate title
        self.validate_task_title(&title)
            .context("Failed to validate task title")?;

        // Create task
        let mut task = Task {
            id: Uuid::new_v4(),
            title,
            description,
            status: TaskStatus::Todo,
            priority,
            assignee_id: None,
            due_date: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
        };

        // Simulate database save with error handling
        match self.save_task_to_database(&task) {
            Ok(_) => {
                println!("✅ Task created successfully: {}", task.title);
                Ok(task)
            },
            Err(e) => {
                let db_error = TaskError::DatabaseError {
                    operation: "create".to_string(),
                    details: e.to_string(),
                };
                Err(db_error.into())
            }
        }
    }

    // Simulate database operations with error injection
    fn save_task_to_database(&self, task: &Task) -> Result<()> {
        // Simulate random database errors
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        task.id.hash(&mut hasher);
        let hash = hasher.finish();

        if hash % 10 == 0 { // 10% chance of database error
            Err(anyhow!("Database connection timeout"))
        } else {
            let mut tasks = self.tasks.lock().unwrap();
            tasks.push(task.clone());
            Ok(())
        }
    }

    // Update task status with transition validation
    fn update_task_status(&self, task_id: &Uuid, new_status: TaskStatus) -> Result<Task> {
        println!("\n🔄 Updating task status: {} -> {:?}", task_id, new_status);

        let mut tasks = self.tasks.lock().unwrap();
        let task = tasks.iter_mut()
            .find(|t| t.id == *task_id && !t.is_deleted())
            .ok_or_else(|| TaskError::NotFound { id: *task_id })?;

        // Validate status transition
        self.validate_task_status_transition(&task.status, &new_status)
            .context("Invalid status transition")?;

        // Update status
        task.status = new_status.clone();
        task.updated_at = chrono::Utc::now();

        println!("✅ Task status updated: {} -> {:?}", task.title, new_status);
        Ok(task.clone())
    }

    // Assign task to user
    fn assign_task(&self, task_id: &Uuid, assignee_id: Uuid) -> Result<Task> {
        println!("\n👤 Assigning task {} to user {}", task_id, assignee_id);

        let mut tasks = self.tasks.lock().unwrap();
        let task = tasks.iter_mut()
            .find(|t| t.id == *task_id && !t.is_deleted())
            .ok_or_else(|| TaskError::NotFound { id: *task_id })?;

        // Check if task can be edited
        self.validate_edit_permissions(task)
            .context("Cannot edit task")?;

        // Validate assignee
        self.validate_assignee(&Some(assignee_id))
            .context("Invalid assignee")?;

        // Update assignee
        task.assignee_id = Some(assignee_id);
        task.updated_at = chrono::Utc::now();

        let user = self.user_service.get_user(&assignee_id)?;
        println!("✅ Task assigned to {} ({})", user.name, user.email);
        Ok(task.clone())
    }

    // Get task with error handling
    fn get_task_safely(&self, task_id: &Uuid) -> Result<Task> {
        println!("\n🔍 Getting task: {}", task_id);

        let tasks = self.tasks.lock().unwrap();
        let task = tasks.iter()
            .find(|t| t.id == *task_id && !t.is_deleted())
            .ok_or_else(|| TaskError::NotFound { id: *task_id })?;

        println!("✅ Found task: {} (Status: {:?})", task.title, task.status);
        Ok(task.clone())
    }

    // Delete task with checks
    fn delete_task_safely(&self, task_id: &Uuid) -> Result<()> {
        println!("\n🗑️  Deleting task: {}", task_id);

        let mut tasks = self.tasks.lock().unwrap();
        let task = tasks.iter_mut()
            .find(|t| t.id == *task_id && !t.is_deleted())
            .ok_or_else(|| TaskError::NotFound { id: *task_id })?;

        // Check if task can be deleted (business rule)
        match task.status {
            TaskStatus::InProgress => {
                bail!(TaskError::ValidationError {
                    field: "status".to_string(),
                    message: "Cannot delete task in progress".to_string(),
                });
            },
            _ => {
                task.deleted_at = Some(chrono::Utc::now());
                task.updated_at = chrono::Utc::now();
                println!("✅ Task deleted: {}", task.title);
                Ok(())
            }
        }
    }

    // Simulate rate limiting
    fn check_rate_limit(&self, user_id: &Uuid) -> Result<()> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        user_id.hash(&mut hasher);
        let hash = hasher.finish();

        if hash % 5 == 0 { // 20% chance of rate limit
            bail!(TaskError::RateLimitExceeded { seconds: 60 });
        }

        Ok(())
    }
}

impl BackboneHttpHandler<Task> for TaskService {
    fn list(&self, _request: ListRequest) -> Result<ApiResponse<Vec<Task>>> {
        println!("\n📋 Listing all tasks...");

        let tasks = self.tasks.lock().unwrap();
        let active_tasks: Vec<Task> = tasks.iter()
            .filter(|t| !t.is_deleted())
            .cloned()
            .collect();

        let count = active_tasks.len();
        let response = ApiResponse::success(
            active_tasks,
            Some(format!("Found {} tasks", count))
        );

        println!("✅ Listed {} tasks", response.data.as_ref().unwrap().len());
        Ok(response)
    }

    fn create(&self, request: Task) -> Result<ApiResponse<Task>> {
        // Check rate limit
        if let Some(assignee_id) = request.assignee_id {
            self.check_rate_limit(&assignee_id)?;
        }

        // Validate and create
        match self.create_task_with_validation(request.title, request.description, request.priority) {
            Ok(task) => {
                Ok(ApiResponse::success(task, Some("Task created successfully".to_string())))
            },
            Err(e) => {
                println!("❌ Failed to create task: {}", e);
                Ok(ApiResponse::error(format!("Failed to create task: {}", e)))
            }
        }
    }

    fn get_by_id(&self, id: &Uuid) -> Result<ApiResponse<Task>> {
        match self.get_task_safely(id) {
            Ok(task) => Ok(ApiResponse::success(task, Some("Task found".to_string()))),
            Err(e) => {
                println!("❌ Failed to get task: {}", e);
                Ok(ApiResponse::error(format!("Task not found: {}", e)))
            }
        }
    }

    fn update(&self, id: &Uuid, request: Task) -> Result<ApiResponse<Task>> {
        match self.get_task_safely(id) {
            Ok(mut existing_task) => {
                // Validate title
                if let Err(e) = self.validate_task_title(&request.title) {
                    return Ok(ApiResponse::error(format!("Validation failed: {}", e)));
                }

                // Validate status transition
                if let Err(e) = self.validate_task_status_transition(&existing_task.status, &request.status) {
                    return Ok(ApiResponse::error(format!("Invalid status transition: {}", e)));
                }

                // Update task
                let mut tasks = self.tasks.lock().unwrap();
                if let Some(task) = tasks.iter_mut().find(|t| t.id == *id && !t.is_deleted()) {
                    task.title = request.title;
                    task.description = request.description;
                    task.status = request.status;
                    task.priority = request.priority;
                    task.updated_at = chrono::Utc::now();
                    Ok(ApiResponse::success(task.clone(), Some("Task updated successfully".to_string())))
                } else {
                    Ok(ApiResponse::error("Task not found".to_string()))
                }
            },
            Err(e) => Ok(ApiResponse::error(format!("Update failed: {}", e))),
        }
    }

    fn partial_update(&self, id: &Uuid, fields: HashMap<String, serde_json::Value>) -> Result<ApiResponse<Task>> {
        match self.get_task_safely(id) {
            Ok(_) => {
                let mut tasks = self.tasks.lock().unwrap();
                if let Some(task) = tasks.iter_mut().find(|t| t.id == *id && !t.is_deleted()) {
                    for (field, value) in fields {
                        match field.as_str() {
                            "title" => {
                                if let Some(s) = value.as_str() {
                                    if let Err(e) = self.validate_task_title(s) {
                                        return Ok(ApiResponse::error(format!("Title validation failed: {}", e)));
                                    }
                                    task.title = s.to_string();
                                }
                            },
                            "description" => if let Some(s) = value.as_str() { task.description = s.to_string(); },
                            "status" => {
                                if let Some(s) = value.as_str() {
                                    let new_status = match s {
                                        "Todo" => TaskStatus::Todo,
                                        "InProgress" => TaskStatus::InProgress,
                                        "Review" => TaskStatus::Review,
                                        "Done" => TaskStatus::Done,
                                        "Cancelled" => TaskStatus::Cancelled,
                                        _ => return Ok(ApiResponse::error(format!("Invalid status: {}", s))),
                                    };
                                    if let Err(e) = self.validate_task_status_transition(&task.status, &new_status) {
                                        return Ok(ApiResponse::error(format!("Status transition failed: {}", e)));
                                    }
                                    task.status = new_status;
                                }
                            },
                            "priority" => {
                                if let Some(s) = value.as_str() {
                                    task.priority = match s {
                                        "Low" => TaskPriority::Low,
                                        "Medium" => TaskPriority::Medium,
                                        "High" => TaskPriority::High,
                                        "Critical" => TaskPriority::Critical,
                                        _ => return Ok(ApiResponse::error(format!("Invalid priority: {}", s))),
                                    };
                                }
                            },
                            _ => return Ok(ApiResponse::error(format!("Invalid field: {}", field))),
                        }
                    }
                    task.updated_at = chrono::Utc::now();
                    Ok(ApiResponse::success(task.clone(), Some("Task partially updated".to_string())))
                } else {
                    Ok(ApiResponse::error("Task not found".to_string()))
                }
            },
            Err(e) => Ok(ApiResponse::error(format!("Partial update failed: {}", e))),
        }
    }

    fn soft_delete(&self, id: &Uuid) -> Result<ApiResponse<()>> {
        match self.delete_task_safely(id) {
            Ok(_) => Ok(ApiResponse::success((), Some("Task deleted successfully".to_string()))),
            Err(e) => {
                println!("❌ Failed to delete task: {}", e);
                Ok(ApiResponse::error(format!("Delete failed: {}", e)))
            }
        }
    }

    fn bulk_create(&self, request: BulkCreateRequest<Task>) -> Result<ApiResponse<BulkResponse<Task>>> {
        println!("\n📦 Bulk creating {} tasks...", request.items.len());

        let mut created_tasks = Vec::new();
        let mut errors = Vec::new();

        for (index, task) in request.items.into_iter().enumerate() {
            match self.create_task_with_validation(task.title, task.description, task.priority) {
                Ok(created_task) => {
                    created_tasks.push(created_task);
                },
                Err(e) => {
                    errors.push(format!("Task {}: {}", index + 1, e));
                }
            }
        }

        let response = BulkResponse {
            items: created_tasks.clone(),
            total: created_tasks.len(),
            failed: errors.len(),
            errors,
        };
        let summary = format!(
            "Bulk create completed: {} successful, {} failed",
            created_tasks.len(),
            response.failed
        );

        Ok(ApiResponse::success(response, Some(summary)))
    }

    fn upsert(&self, request: UpsertRequest<Task>) -> Result<ApiResponse<Task>> {
        println!("\n🔄 Upserting task...");

        // For simplicity, we'll just create since we don't have unique identifiers to check against
        match self.create_task_with_validation(request.entity.title, request.entity.description, request.entity.priority) {
            Ok(task) => Ok(ApiResponse::success(task, Some("Task created (upsert)".to_string()))),
            Err(e) => Ok(ApiResponse::error(format!("Upsert failed: {}", e))),
        }
    }

    fn list_deleted(&self, _request: ListRequest) -> Result<ApiResponse<Vec<Task>>> {
        let tasks = self.tasks.lock().unwrap();
        let deleted_tasks: Vec<Task> = tasks.iter()
            .filter(|t| t.is_deleted())
            .cloned()
            .collect();

        Ok(ApiResponse::success(deleted_tasks, Some("Deleted tasks listed".to_string())))
    }

    fn restore(&self, id: &Uuid) -> Result<ApiResponse<Task>> {
        println!("\n🔄 Restoring task: {}", id);

        let mut tasks = self.tasks.lock().unwrap();
        match tasks.iter_mut().find(|t| t.id == *id && t.is_deleted()) {
            Some(task) => {
                task.deleted_at = None;
                task.updated_at = chrono::Utc::now();
                Ok(ApiResponse::success(task.clone(), Some("Task restored successfully".to_string())))
            },
            None => Ok(ApiResponse::error("Deleted task not found".to_string())),
        }
    }

    fn empty_trash(&self) -> Result<ApiResponse<()>> {
        println!("\n🗑️  Emptying trash...");

        let mut tasks = self.tasks.lock().unwrap();
        let original_len = tasks.len();
        tasks.retain(|t| !t.is_deleted());
        let deleted_count = original_len - tasks.len();

        Ok(ApiResponse::success((), Some(format!("Permanently deleted {} tasks", deleted_count))))
    }

    fn get_deleted_by_id(&self, id: &Uuid) -> Result<ApiResponse<Task>> {
        let tasks = self.tasks.lock().unwrap();
        match tasks.iter().find(|t| t.id == *id && t.is_deleted()) {
            Some(task) => Ok(ApiResponse::success(task.clone(), Some("Deleted task found".to_string()))),
            None => Ok(ApiResponse::error("Deleted task not found".to_string())),
        }
    }
}

fn demonstrate_error_handling(service: &TaskService) -> Result<()> {
    println!("\n🛡️ Error Handling Demonstration");
    println!("==================================");

    // 1. Validation Error
    println!("\n1️⃣ Validation Error (Empty Title):");
    match service.create_task_with_validation("".to_string(), "Test description".to_string(), TaskPriority::Medium) {
        Ok(_) => println!("❌ Should have failed!"),
        Err(e) => println!("✅ Caught validation error: {}", e),
    }

    // 2. Title Too Long Error
    println!("\n2️⃣ Validation Error (Title Too Long):");
    let long_title = "a".repeat(201);
    match service.create_task_with_validation(long_title.clone(), "Test description".to_string(), TaskPriority::Medium) {
        Ok(_) => println!("❌ Should have failed!"),
        Err(e) => println!("✅ Caught title length error: {}", e),
    }

    // 3. Task Not Found Error
    println!("\n3️⃣ Task Not Found Error:");
    let fake_id = Uuid::new_v4();
    match service.get_task_safely(&fake_id) {
        Ok(_) => println!("❌ Should have failed!"),
        Err(e) => println!("✅ Caught not found error: {}", e),
    }

    // 4. Invalid Status Transition Error
    println!("\n4️⃣ Invalid Status Transition Error:");
    if let Ok(mut task) = service.create_task_with_validation("Test Task".to_string(), "Description".to_string(), TaskPriority::Medium) {
        // Set to Done first
        task.status = TaskStatus::Done;

        // Try to transition from Done to InProgress (invalid)
        match service.validate_task_status_transition(&task.status, &TaskStatus::InProgress) {
            Ok(_) => println!("❌ Should have failed!"),
            Err(e) => println!("✅ Caught invalid transition error: {}", e),
        }
    }

    // 5. Assignee Not Found Error
    println!("\n5️⃣ Assignee Not Found Error:");
    let fake_user_id = Uuid::new_v4();
    match service.validate_assignee(&Some(fake_user_id)) {
        Ok(_) => println!("❌ Should have failed!"),
        Err(e) => println!("✅ Caught assignee not found error: {}", e),
    }

    // 6. Bulk Create with Some Errors
    println!("\n6️⃣ Bulk Create with Errors:");
    let bulk_tasks = vec![
        Task {
            id: Uuid::new_v4(),
            title: "Valid Task".to_string(),
            description: "Valid description".to_string(),
            status: TaskStatus::Todo,
            priority: TaskPriority::Medium,
            assignee_id: None,
            due_date: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
        },
        Task {
            id: Uuid::new_v4(),
            title: "".to_string(), // Invalid - empty title
            description: "Invalid description".to_string(),
            status: TaskStatus::Todo,
            priority: TaskPriority::Low,
            assignee_id: None,
            due_date: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
        },
        Task {
            id: Uuid::new_v4(),
            title: "Another Valid Task".to_string(),
            description: "Another valid description".to_string(),
            status: TaskStatus::Todo,
            priority: TaskPriority::High,
            assignee_id: None,
            due_date: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
        },
    ];

    let bulk_result = service.bulk_create(BulkCreateRequest { items: bulk_tasks })?;
    if let Some(response) = bulk_result.data {
        println!("✅ Bulk create completed:");
        println!("   - Successful: {}", response.total);
        println!("   - Failed: {}", response.failed);
        if !response.errors.is_empty() {
            println!("   - Errors:");
            for error in response.errors {
                println!("     {}", error);
            }
        }
    }

    // 7. Database Error Simulation
    println!("\n7️⃣ Database Error Simulation:");
    // Create multiple tasks to increase chance of simulated database error
    for i in 0..20 {
        let title = format!("Test Task {}", i);
        if let Err(e) = service.create_task_with_validation(title.clone(), "Test description".to_string(), TaskPriority::Medium) {
            println!("✅ Caught database error for task '{}': {}", title, e);
            break; // Stop after first error
        }
    }

    // 8. Rate Limit Error
    println!("\n8️⃣ Rate Limit Error:");
    let fake_user = User::new("Test User".to_string(), "test@example.com".to_string());
    for i in 0..10 { // Try multiple times to trigger rate limit
        match service.check_rate_limit(&fake_user.id) {
            Ok(_) => {
                if i == 9 {
                    println!("❌ Should have hit rate limit by now");
                }
            },
            Err(e) => {
                println!("✅ Caught rate limit error: {}", e);
                break;
            }
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    println!("🦴 Backbone Core - Error Handling Example");
    println!("=======================================");

    let service = TaskService::new();

    // Demonstrate error handling patterns
    demonstrate_error_handling(&service)?;

    // Show successful operations
    println!("\n🎉 Error handling examples completed!");
    println!("💡 Error Handling Best Practices:");
    println!("   ✅ Use custom error types with thiserror");
    println!("   ✅ Provide detailed error messages");
    println!("   ✅ Validate inputs early and clearly");
    println!("   ✅ Use context() for error chaining");
    println!("   ✅ Handle different error types appropriately");
    println!("   ✅ Log errors for debugging");
    println!("   ✅ Provide user-friendly error responses");

    Ok(())
}