//! Real-world E-commerce Scenario Example
//!
//! This example demonstrates a complete e-commerce scenario using Backbone Core
//! with multiple entities: Users, Products, Orders, and Reviews.

use backbone_core::http::*;
use backbone_core::entity::*;
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use std::collections::HashMap;
use anyhow::Result;

// ==================== ENTITIES ====================

// User entity
#[derive(Debug, Clone, Serialize, Deserialize)]
struct User {
    id: Uuid,
    username: String,
    email: String,
    first_name: String,
    last_name: String,
    role: UserRole,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum UserRole {
    Customer,
    Admin,
    Moderator,
}

impl Entity for User {
    fn id(&self) -> &Uuid { &self.id }
    fn created_at(&self) -> chrono::DateTime<chrono::Utc> { self.created_at }
    fn updated_at(&self) -> chrono::DateTime<chrono::Utc> { self.updated_at }
    fn deleted_at(&self) -> Option<chrono::DateTime<chrono::Utc>> { self.deleted_at }
}

// Product entity
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Product {
    id: Uuid,
    name: String,
    description: String,
    price: f64,
    category: String,
    stock: i32,
    sku: String,
    is_active: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Entity for Product {
    fn id(&self) -> &Uuid { &self.id }
    fn created_at(&self) -> chrono::DateTime<chrono::Utc> { self.created_at }
    fn updated_at(&self) -> chrono::DateTime<chrono::Utc> { self.updated_at }
    fn deleted_at(&self) -> Option<chrono::DateTime<chrono::Utc>> { self.deleted_at }
}

// Order entity
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Order {
    id: Uuid,
    user_id: Uuid,
    status: OrderStatus,
    total_amount: f64,
    items: Vec<OrderItem>,
    shipping_address: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum OrderStatus {
    Pending,
    Confirmed,
    Processing,
    Shipped,
    Delivered,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OrderItem {
    product_id: Uuid,
    product_name: String,
    quantity: i32,
    price: f64,
}

impl Entity for Order {
    fn id(&self) -> &Uuid { &self.id }
    fn created_at(&self) -> chrono::DateTime<chrono::Utc> { self.created_at }
    fn updated_at(&self) -> chrono::DateTime<chrono::Utc> { self.updated_at }
    fn deleted_at(&self) -> Option<chrono::DateTime<chrono::Utc>> { self.deleted_at }
}

// Review entity
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Review {
    id: Uuid,
    product_id: Uuid,
    user_id: Uuid,
    rating: i32, // 1-5 stars
    title: String,
    comment: String,
    is_verified: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Entity for Review {
    fn id(&self) -> &Uuid { &self.id }
    fn created_at(&self) -> chrono::DateTime<chrono::Utc> { self.created_at }
    fn updated_at(&self) -> chrono::DateTime<chrono::Utc> { self.updated_at }
    fn deleted_at(&self) -> Option<chrono::DateTime<chrono::Utc>> { self.deleted_at }
}

// ==================== SERVICES ====================

struct ECommerceService {
    users: std::sync::Mutex<Vec<User>>,
    products: std::sync::Mutex<Vec<Product>>,
    orders: std::sync::Mutex<Vec<Order>>,
    reviews: std::sync::Mutex<Vec<Review>>,
}

impl ECommerceService {
    fn new() -> Self {
        let service = Self {
            users: std::sync::Mutex::new(Vec::new()),
            products: std::sync::Mutex::new(Vec::new()),
            orders: std::sync::Mutex::new(Vec::new()),
            reviews: std::sync::Mutex::new(Vec::new()),
        };

        // Initialize with sample data
        service.initialize_sample_data();
        service
    }

    fn initialize_sample_data(&self) {
        // Create users
        let mut users = self.users.lock().unwrap();
        users.push(User {
            id: Uuid::new_v4(),
            username: "john_doe".to_string(),
            email: "john@example.com".to_string(),
            first_name: "John".to_string(),
            last_name: "Doe".to_string(),
            role: UserRole::Customer,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
        });

        users.push(User {
            id: Uuid::new_v4(),
            username: "jane_admin".to_string(),
            email: "jane@ecommerce.com".to_string(),
            first_name: "Jane".to_string(),
            last_name: "Smith".to_string(),
            role: UserRole::Admin,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
        });

        // Create products
        let mut products = self.products.lock().unwrap();
        products.push(Product {
            id: Uuid::new_v4(),
            name: "Wireless Headphones".to_string(),
            description: "Premium noise-cancelling wireless headphones".to_string(),
            price: 199.99,
            category: "Electronics".to_string(),
            stock: 50,
            sku: "WH-001".to_string(),
            is_active: true,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
        });

        products.push(Product {
            id: Uuid::new_v4(),
            name: "Smart Watch".to_string(),
            description: "Fitness tracking smartwatch with heart rate monitor".to_string(),
            price: 299.99,
            category: "Electronics".to_string(),
            stock: 25,
            sku: "SW-002".to_string(),
            is_active: true,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
        });

        products.push(Product {
            id: Uuid::new_v4(),
            name: "Running Shoes".to_string(),
            description: "Professional running shoes for marathon training".to_string(),
            price: 129.99,
            category: "Sports".to_string(),
            stock: 100,
            sku: "RS-003".to_string(),
            is_active: true,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
        });
    }
}

// ==================== USER SERVICE IMPLEMENTATION ====================

impl BackboneHttpHandler<User> for ECommerceService {
    fn list(&self, request: ListRequest) -> Result<ApiResponse<Vec<User>>> {
        let users = self.users.lock().unwrap();
        let active_users: Vec<User> = users.iter()
            .filter(|u| !u.is_deleted())
            .cloned()
            .collect();

        Ok(ApiResponse::success(active_users, Some("Users listed".to_string())))
    }

    fn create(&self, request: User) -> Result<ApiResponse<User>> {
        let mut users = self.users.lock().unwrap();
        let mut new_user = request;
        new_user.id = Uuid::new_v4();
        new_user.created_at = chrono::Utc::now();
        new_user.updated_at = chrono::Utc::now();

        users.push(new_user.clone());
        Ok(ApiResponse::success(new_user, Some("User created".to_string())))
    }

    fn get_by_id(&self, id: &Uuid) -> Result<ApiResponse<User>> {
        let users = self.users.lock().unwrap();
        match users.iter().find(|u| u.id == *id && !u.is_deleted()) {
            Some(user) => Ok(ApiResponse::success(user.clone(), Some("User found".to_string()))),
            None => Ok(ApiResponse::error("User not found".to_string())),
        }
    }

    fn update(&self, id: &Uuid, request: User) -> Result<ApiResponse<User>> {
        let mut users = self.users.lock().unwrap();
        match users.iter_mut().find(|u| u.id == *id && !u.is_deleted()) {
            Some(user) => {
                user.username = request.username;
                user.email = request.email;
                user.first_name = request.first_name;
                user.last_name = request.last_name;
                user.role = request.role;
                user.updated_at = chrono::Utc::now();
                Ok(ApiResponse::success(user.clone(), Some("User updated".to_string())))
            },
            None => Ok(ApiResponse::error("User not found".to_string())),
        }
    }

    fn partial_update(&self, id: &Uuid, fields: HashMap<String, serde_json::Value>) -> Result<ApiResponse<User>> {
        let mut users = self.users.lock().unwrap();
        match users.iter_mut().find(|u| u.id == *id && !u.is_deleted()) {
            Some(user) => {
                for (field, value) in fields {
                    match field.as_str() {
                        "username" => if let Some(s) = value.as_str() { user.username = s.to_string(); },
                        "email" => if let Some(s) = value.as_str() { user.email = s.to_string(); },
                        "first_name" => if let Some(s) = value.as_str() { user.first_name = s.to_string(); },
                        "last_name" => if let Some(s) = value.as_str() { user.last_name = s.to_string(); },
                        "role" => if let Some(s) = value.as_str() {
                            user.role = match s {
                                "Admin" => UserRole::Admin,
                                "Moderator" => UserRole::Moderator,
                                _ => UserRole::Customer,
                            };
                        },
                        _ => return Ok(ApiResponse::error(format!("Invalid field: {}", field))),
                    }
                }
                user.updated_at = chrono::Utc::now();
                Ok(ApiResponse::success(user.clone(), Some("User updated".to_string())))
            },
            None => Ok(ApiResponse::error("User not found".to_string())),
        }
    }

    fn soft_delete(&self, id: &Uuid) -> Result<ApiResponse<()>> {
        let mut users = self.users.lock().unwrap();
        match users.iter_mut().find(|u| u.id == *id && !u.is_deleted()) {
            Some(user) => {
                user.deleted_at = Some(chrono::Utc::now());
                user.updated_at = chrono::Utc::now();
                Ok(ApiResponse::success((), Some("User deleted".to_string())))
            },
            None => Ok(ApiResponse::error("User not found".to_string())),
        }
    }

    fn bulk_create(&self, request: BulkCreateRequest<User>) -> Result<ApiResponse<BulkResponse<User>>> {
        let mut users = self.users.lock().unwrap();
        let mut created_users = Vec::new();

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
            failed: 0,
            errors: Vec::new(),
        };

        Ok(ApiResponse::success(response, Some("Users bulk created".to_string())))
    }

    fn upsert(&self, request: UpsertRequest<User>) -> Result<ApiResponse<User>> {
        let mut users = self.users.lock().unwrap();
        let user = request.entity;

        if let Some(existing_user) = users.iter_mut().find(|u| u.email == user.email && !u.is_deleted()) {
            // Update existing user by email
            existing_user.username = user.username;
            existing_user.first_name = user.first_name;
            existing_user.last_name = user.last_name;
            existing_user.role = user.role;
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
        let deleted_users: Vec<User> = users.iter()
            .filter(|u| u.is_deleted())
            .cloned()
            .collect();

        Ok(ApiResponse::success(deleted_users, Some("Deleted users listed".to_string())))
    }

    fn restore(&self, id: &Uuid) -> Result<ApiResponse<User>> {
        let mut users = self.users.lock().unwrap();
        match users.iter_mut().find(|u| u.id == *id && u.is_deleted()) {
            Some(user) => {
                user.deleted_at = None;
                user.updated_at = chrono::Utc::now();
                Ok(ApiResponse::success(user.clone(), Some("User restored".to_string())))
            },
            None => Ok(ApiResponse::error("Deleted user not found".to_string())),
        }
    }

    fn empty_trash(&self) -> Result<ApiResponse<()>> {
        let mut users = self.users.lock().unwrap();
        let original_len = users.len();
        users.retain(|u| !u.is_deleted());
        let deleted_count = original_len - users.len();

        Ok(ApiResponse::success((), Some(format!("Permanently deleted {} users", deleted_count))))
    }

    fn get_deleted_by_id(&self, id: &Uuid) -> Result<ApiResponse<User>> {
        let users = self.users.lock().unwrap();
        match users.iter().find(|u| u.id == *id && u.is_deleted()) {
            Some(user) => Ok(ApiResponse::success(user.clone(), Some("Deleted user found".to_string()))),
            None => Ok(ApiResponse::error("Deleted user not found".to_string())),
        }
    }
}

// Placeholder implementations for other entities (simplified for example)
impl BackboneHttpHandler<Product> for ECommerceService {
    fn list(&self, _request: ListRequest) -> Result<ApiResponse<Vec<Product>>> {
        let products = self.products.lock().unwrap();
        let active_products: Vec<Product> = products.iter()
            .filter(|p| !p.is_deleted() && p.is_active)
            .cloned()
            .collect();
        Ok(ApiResponse::success(active_products, Some("Products listed".to_string())))
    }

    fn create(&self, request: Product) -> Result<ApiResponse<Product>> {
        let mut products = self.products.lock().unwrap();
        let mut new_product = request;
        new_product.id = Uuid::new_v4();
        new_product.created_at = chrono::Utc::now();
        new_product.updated_at = chrono::Utc::now();
        products.push(new_product.clone());
        Ok(ApiResponse::success(new_product, Some("Product created".to_string())))
    }

    fn get_by_id(&self, id: &Uuid) -> Result<ApiResponse<Product>> {
        let products = self.products.lock().unwrap();
        match products.iter().find(|p| p.id == *id && !p.is_deleted() && p.is_active) {
            Some(product) => Ok(ApiResponse::success(product.clone(), Some("Product found".to_string()))),
            None => Ok(ApiResponse::error("Product not found".to_string())),
        }
    }

    fn update(&self, id: &Uuid, request: Product) -> Result<ApiResponse<Product>> { Ok(ApiResponse::error("Not implemented".to_string())) }
    fn partial_update(&self, id: &Uuid, request: HashMap<String, serde_json::Value>) -> Result<ApiResponse<Product>> { Ok(ApiResponse::error("Not implemented".to_string())) }
    fn soft_delete(&self, id: &Uuid) -> Result<ApiResponse<()>> { Ok(ApiResponse::error("Not implemented".to_string())) }
    fn bulk_create(&self, request: BulkCreateRequest<Product>) -> Result<ApiResponse<BulkResponse<Product>>> { Ok(ApiResponse::error("Not implemented".to_string())) }
    fn upsert(&self, request: UpsertRequest<Product>) -> Result<ApiResponse<Product>> { Ok(ApiResponse::error("Not implemented".to_string())) }
    fn list_deleted(&self, _request: ListRequest) -> Result<ApiResponse<Vec<Product>>> { Ok(ApiResponse::error("Not implemented".to_string())) }
    fn restore(&self, id: &Uuid) -> Result<ApiResponse<Product>> { Ok(ApiResponse::error("Not implemented".to_string())) }
    fn empty_trash(&self) -> Result<ApiResponse<()>> { Ok(ApiResponse::error("Not implemented".to_string())) }
    fn get_deleted_by_id(&self, _id: &Uuid) -> Result<ApiResponse<Product>> { Ok(ApiResponse::error("Not implemented".to_string())) }
}

impl BackboneHttpHandler<Order> for ECommerceService {
    fn list(&self, _request: ListRequest) -> Result<ApiResponse<Vec<Order>>> {
        let orders = self.orders.lock().unwrap();
        let active_orders: Vec<Order> = orders.iter()
            .filter(|o| !o.is_deleted())
            .cloned()
            .collect();
        Ok(ApiResponse::success(active_orders, Some("Orders listed".to_string())))
    }

    fn create(&self, request: Order) -> Result<ApiResponse<Order>> {
        let mut orders = self.orders.lock().unwrap();
        let mut new_order = request;
        new_order.id = Uuid::new_v4();
        new_order.created_at = chrono::Utc::now();
        new_order.updated_at = chrono::Utc::now();
        new_order.status = OrderStatus::Pending;
        orders.push(new_order.clone());
        Ok(ApiResponse::success(new_order, Some("Order created".to_string())))
    }

    fn get_by_id(&self, id: &Uuid) -> Result<ApiResponse<Order>> {
        let orders = self.orders.lock().unwrap();
        match orders.iter().find(|o| o.id == *id && !o.is_deleted()) {
            Some(order) => Ok(ApiResponse::success(order.clone(), Some("Order found".to_string()))),
            None => Ok(ApiResponse::error("Order not found".to_string())),
        }
    }

    fn update(&self, id: &Uuid, request: Order) -> Result<ApiResponse<Order>> { Ok(ApiResponse::error("Not implemented".to_string())) }
    fn partial_update(&self, id: &Uuid, request: HashMap<String, serde_json::Value>) -> Result<ApiResponse<Order>> { Ok(ApiResponse::error("Not implemented".to_string())) }
    fn soft_delete(&self, id: &Uuid) -> Result<ApiResponse<()>> { Ok(ApiResponse::error("Not implemented".to_string())) }
    fn bulk_create(&self, request: BulkCreateRequest<Order>) -> Result<ApiResponse<BulkResponse<Order>>> { Ok(ApiResponse::error("Not implemented".to_string())) }
    fn upsert(&self, request: UpsertRequest<Order>) -> Result<ApiResponse<Order>> { Ok(ApiResponse::error("Not implemented".to_string())) }
    fn list_deleted(&self, _request: ListRequest) -> Result<ApiResponse<Vec<Order>>> { Ok(ApiResponse::error("Not implemented".to_string())) }
    fn restore(&self, id: &Uuid) -> Result<ApiResponse<Order>> { Ok(ApiResponse::error("Not implemented".to_string())) }
    fn empty_trash(&self) -> Result<ApiResponse<()>> { Ok(ApiResponse::error("Not implemented".to_string())) }
    fn get_deleted_by_id(&self, _id: &Uuid) -> Result<ApiResponse<Order>> { Ok(ApiResponse::error("Not implemented".to_string())) }
}

impl BackboneHttpHandler<Review> for ECommerceService {
    fn list(&self, _request: ListRequest) -> Result<ApiResponse<Vec<Review>>> {
        let reviews = self.reviews.lock().unwrap();
        let active_reviews: Vec<Review> = reviews.iter()
            .filter(|r| !r.is_deleted())
            .cloned()
            .collect();
        Ok(ApiResponse::success(active_reviews, Some("Reviews listed".to_string())))
    }

    fn create(&self, request: Review) -> Result<ApiResponse<Review>> {
        let mut reviews = self.reviews.lock().unwrap();
        let mut new_review = request;
        new_review.id = Uuid::new_v4();
        new_review.created_at = chrono::Utc::now();
        new_review.updated_at = chrono::Utc::now();
        new_review.is_verified = false; // Reviews need verification
        reviews.push(new_review.clone());
        Ok(ApiResponse::success(new_review, Some("Review created".to_string())))
    }

    fn get_by_id(&self, id: &Uuid) -> Result<ApiResponse<Review>> {
        let reviews = self.reviews.lock().unwrap();
        match reviews.iter().find(|r| r.id == *id && !r.is_deleted()) {
            Some(review) => Ok(ApiResponse::success(review.clone(), Some("Review found".to_string()))),
            None => Ok(ApiResponse::error("Review not found".to_string())),
        }
    }

    fn update(&self, id: &Uuid, request: Review) -> Result<ApiResponse<Review>> { Ok(ApiResponse::error("Not implemented".to_string())) }
    fn partial_update(&self, id: &Uuid, request: HashMap<String, serde_json::Value>) -> Result<ApiResponse<Review>> { Ok(ApiResponse::error("Not implemented".to_string())) }
    fn soft_delete(&self, id: &Uuid) -> Result<ApiResponse<()>> { Ok(ApiResponse::error("Not implemented".to_string())) }
    fn bulk_create(&self, request: BulkCreateRequest<Review>) -> Result<ApiResponse<BulkResponse<Review>>> { Ok(ApiResponse::error("Not implemented".to_string())) }
    fn upsert(&self, request: UpsertRequest<Review>) -> Result<ApiResponse<Review>> { Ok(ApiResponse::error("Not implemented".to_string())) }
    fn list_deleted(&self, _request: ListRequest) -> Result<ApiResponse<Vec<Review>>> { Ok(ApiResponse::error("Not implemented".to_string())) }
    fn restore(&self, id: &Uuid) -> Result<ApiResponse<Review>> { Ok(ApiResponse::error("Not implemented".to_string())) }
    fn empty_trash(&self) -> Result<ApiResponse<()>> { Ok(ApiResponse::error("Not implemented".to_string())) }
    fn get_deleted_by_id(&self, _id: &Uuid) -> Result<ApiResponse<Review>> { Ok(ApiResponse::error("Not implemented".to_string())) }
}

// ==================== BUSINESS LOGIC METHODS ====================

impl ECommerceService {
    fn place_order(&self, user_id: &Uuid, product_ids: &[(Uuid, i32)], shipping_address: String) -> Result<Order> {
        // Get user
        let users = self.users.lock().unwrap();
        let user = users.iter().find(|u| u.id == *user_id && !u.is_deleted())
            .ok_or_else(|| anyhow::anyhow!("User not found"))?;
        drop(users);

        // Get products and calculate total
        let products = self.products.lock().unwrap();
        let mut order_items = Vec::new();
        let mut total_amount = 0.0;

        for (product_id, quantity) in product_ids {
            let product = products.iter()
                .find(|p| p.id == *product_id && !p.is_deleted() && p.is_active)
                .ok_or_else(|| anyhow::anyhow!("Product {} not found", product_id))?;

            if product.stock < *quantity {
                return Err(anyhow::anyhow!("Insufficient stock for product: {}", product.name));
            }

            order_items.push(OrderItem {
                product_id: *product_id,
                product_name: product.name.clone(),
                quantity: *quantity,
                price: product.price,
            });

            total_amount += product.price * *quantity as f64;
        }
        drop(products);

        // Create order
        let order = Order {
            id: Uuid::new_v4(),
            user_id: *user_id,
            status: OrderStatus::Pending,
            total_amount,
            items: order_items,
            shipping_address,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
        };

        let created_order = self.create(order.clone())?;
        Ok(created_order.data.unwrap_or(order))
    }

    fn add_review(&self, product_id: &Uuid, user_id: &Uuid, rating: i32, title: String, comment: String) -> Result<Review> {
        // Validate rating
        if rating < 1 || rating > 5 {
            return Err(anyhow::anyhow!("Rating must be between 1 and 5"));
        }

        // Check if user has purchased the product
        let orders = self.orders.lock().unwrap();
        let has_purchased = orders.iter().any(|order| {
            order.user_id == *user_id &&
            order.items.iter().any(|item| item.product_id == *product_id) &&
            !order.is_deleted()
        });

        if !has_purchased {
            return Err(anyhow::anyhow!("User must purchase product before reviewing"));
        }
        drop(orders);

        // Create review
        let review = Review {
            id: Uuid::new_v4(),
            product_id: *product_id,
            user_id: *user_id,
            rating,
            title,
            comment,
            is_verified: true, // Auto-verify since they purchased
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
        };

        self.create(review)
    }

    fn get_user_orders(&self, user_id: &Uuid) -> Result<Vec<Order>> {
        let orders = self.orders.lock().unwrap();
        let user_orders: Vec<Order> = orders.iter()
            .filter(|o| o.user_id == *user_id && !o.is_deleted())
            .cloned()
            .collect();
        Ok(user_orders)
    }

    fn get_product_reviews(&self, product_id: &Uuid) -> Result<Vec<Review>> {
        let reviews = self.reviews.lock().unwrap();
        let product_reviews: Vec<Review> = reviews.iter()
            .filter(|r| r.product_id == *product_id && !r.is_deleted())
            .cloned()
            .collect();
        Ok(product_reviews)
    }

    fn get_average_rating(&self, product_id: &Uuid) -> Result<f64> {
        let reviews = self.get_product_reviews(product_id)?;
        if reviews.is_empty() {
            return Ok(0.0);
        }

        let total_rating: i32 = reviews.iter().map(|r| r.rating).sum();
        Ok(total_rating as f64 / reviews.len() as f64)
    }
}

fn demonstrate_ecommerce_workflow(service: &ECommerceService) -> Result<()> {
    println!("\n🛒 E-commerce Workflow Demonstration");
    println!("===================================");

    // Get existing users
    let users = service.list(ListRequest::default())?.data.unwrap();
    let customer = users.iter().find(|u| matches!(u.role, UserRole::Customer)).unwrap();
    let admin = users.iter().find(|u| matches!(u.role, UserRole::Admin)).unwrap();

    println!("\n1️⃣ User Management:");
    println!("   Customer: {} ({})", customer.first_name, customer.email);
    println!("   Admin: {} ({})", admin.first_name, admin.email);

    // Show products
    println!("\n2️⃣ Available Products:");
    let products = service.list(ListRequest::default())?.data.unwrap();
    let product_ids: Vec<(Uuid, i32)> = products.iter().map(|p| (p.id, 1)).collect();
    for (i, product) in products.iter().enumerate() {
        println!("   {}. {} - ${:.2} (Stock: {})", i + 1, product.name, product.price, product.stock);
    }

    // Place an order
    println!("\n3️⃣ Placing Order:");
    let order = service.place_order(
        &customer.id,
        &product_ids,
        "123 Main St, City, State 12345".to_string(),
    )?;
    println!("   ✅ Order placed successfully!");
    println!("   Order ID: {}", order.id);
    println!("   Total: ${:.2}", order.total_amount);
    println!("   Items:");
    for item in &order.items {
        println!("     - {} x{} @ ${:.2}", item.product_name, item.quantity, item.price);
    }

    // Add reviews
    println!("\n4️⃣ Adding Reviews:");
    for product in &products {
        match service.add_review(&product.id, &customer.id, 5, "Great product!".to_string(), "I really love this item!".to_string()) {
            Ok(review) => println!("   ✅ Review added for {} - Rating: {}", product.name, review.rating),
            Err(e) => println!("   ❌ Failed to add review for {}: {}", product.name, e),
        }
    }

    // Get user's orders
    println!("\n5️⃣ User Order History:");
    let user_orders = service.get_user_orders(&customer.id)?;
    for order in user_orders {
        println!("   Order {} - Status: {:?}", order.id, order.status);
        println!("   Total: ${:.2}", order.total_amount);
    }

    // Show product ratings
    println!("\n6️⃣ Product Ratings:");
    for product in &products {
        match service.get_average_rating(&product.id) {
            Ok(rating) => println!("   {} - {:.1}⭐", product.name, rating),
            Err(_) => println!("   {} - No reviews yet", product.name),
        }
    }

    // Create a new customer via bulk operation
    println!("\n7️⃣ Bulk Customer Registration:");
    let new_customers = vec![
        User {
            id: Uuid::new_v4(), // Will be replaced
            username: "alice_customer".to_string(),
            email: "alice@example.com".to_string(),
            first_name: "Alice".to_string(),
            last_name: "Wilson".to_string(),
            role: UserRole::Customer,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
        },
        User {
            id: Uuid::new_v4(), // Will be replaced
            username: "bob_customer".to_string(),
            email: "bob@example.com".to_string(),
            first_name: "Bob".to_string(),
            last_name: "Johnson".to_string(),
            role: UserRole::Customer,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
        },
    ];

    let bulk_result = service.bulk_create(BulkCreateRequest { items: new_customers })?;
    if let Some(response) = bulk_result.data {
        println!("   ✅ Created {} new customers:", response.total);
        for customer in response.items {
            println!("     - {} ({})", customer.first_name, customer.email);
        }
    }

    // Upsert demonstration
    println!("\n8️⃣ User Upsert (Update or Create):");
    let upsert_user = User {
        id: Uuid::new_v4(),
        username: "charlie_upsert".to_string(),
        email: "charlie@example.com".to_string(), // This will be checked
        first_name: "Charlie".to_string(),
        last_name: "Brown".to_string(),
        role: UserRole::Customer,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        deleted_at: None,
    };

    match service.upsert(UpsertRequest {
        entity: upsert_user,
        create_if_not_exists: true,
    }) {
        Ok(result) => {
            if let Some(user) = result.data {
                println!("   ✅ User upserted: {} ({})", user.first_name, user.email);
            }
        },
        Err(e) => println!("   ❌ Upsert failed: {}", e),
    }

    // User management operations
    println!("\n9️⃣ User Management Operations:");
    let active_users = service.list(ListRequest::default())?.data.unwrap();
    println!("   Active users: {}", active_users.len());

    // Soft delete a user
    if let Some(user_to_delete) = active_users.iter().find(|u| u.username == "alice_customer") {
        service.soft_delete(&user_to_delete.id)?;
        println!("   ✅ Soft deleted user: {}", user_to_delete.first_name);

        // List deleted users
        let deleted_users = service.list_deleted(ListRequest::default())?.data.unwrap();
        println!("   Deleted users: {}", deleted_users.len());

        // Restore the user
        service.restore(&user_to_delete.id)?;
        println!("   ✅ Restored user: {}", user_to_delete.first_name);
    }

    Ok(())
}

fn main() -> Result<()> {
    println!("🦴 Backbone Core - E-commerce Scenario Example");
    println!("===============================================");

    let service = ECommerceService::new();

    // Run the complete e-commerce workflow
    demonstrate_ecommerce_workflow(&service)?;

    println!("\n🎉 E-commerce scenario completed successfully!");
    println!("💡 Key Features Demonstrated:");
    println!("   ✅ Multiple entity types (Users, Products, Orders, Reviews)");
    println!("   ✅ Business logic integration");
    println!("   ✅ Data validation and relationships");
    println!("   ✅ CRUD operations with business constraints");
    println!("   ✅ Bulk operations and upserts");
    println!("   ✅ Soft delete and restore functionality");

    Ok(())
}