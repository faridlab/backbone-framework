//! Advanced Pagination and Filtering Example
//!
//! This example demonstrates complex pagination, filtering, sorting,
//! and searching capabilities of Backbone Core.

use backbone_core::http::*;
use backbone_core::entity::*;
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use std::collections::HashMap;
use anyhow::Result;

// Define a Product entity with rich fields for filtering
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Product {
    id: Uuid,
    name: String,
    category: String,
    price: f64,
    stock: i32,
    rating: f32,
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

impl Product {
    fn new(name: String, category: String, price: f64, stock: i32, rating: f32) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            category,
            price,
            stock,
            rating,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        }
    }
}

// Advanced Product Service with filtering and sorting
struct ProductService {
    products: std::sync::Mutex<Vec<Product>>,
}

impl ProductService {
    fn new() -> Self {
        let mut service = Self {
            products: std::sync::Mutex::new(Vec::new()),
        };

        // Initialize with sample data
        let sample_products = vec![
            Product::new("iPhone 15 Pro".to_string(), "Electronics".to_string(), 999.99, 50, 4.8),
            Product::new("MacBook Pro".to_string(), "Electronics".to_string(), 2499.99, 25, 4.9),
            Product::new("AirPods Pro".to_string(), "Electronics".to_string(), 249.99, 100, 4.6),
            Product::new("Standing Desk".to_string(), "Furniture".to_string(), 599.99, 15, 4.3),
            Product::new("Ergonomic Chair".to_string(), "Furniture".to_string(), 399.99, 30, 4.5),
            Product::new("Coffee Maker".to_string(), "Kitchen".to_string(), 129.99, 40, 4.2),
            Product::new("Blender".to_string(), "Kitchen".to_string(), 79.99, 60, 4.1),
            Product::new("Yoga Mat".to_string(), "Sports".to_string(), 29.99, 200, 4.4),
            Product::new("Dumbbells Set".to_string(), "Sports".to_string(), 149.99, 35, 4.7),
            Product::new("Running Shoes".to_string(), "Sports".to_string(), 129.99, 80, 4.6),
            Product::new("iPad Air".to_string(), "Electronics".to_string(), 599.99, 45, 4.7),
            Product::new("Desk Lamp".to_string(), "Furniture".to_string(), 49.99, 75, 4.0),
            Product::new("Toaster".to_string(), "Kitchen".to_string(), 39.99, 90, 3.9),
            Product::new("Resistance Bands".to_string(), "Sports".to_string(), 19.99, 150, 4.3),
            Product::new("Monitor Stand".to_string(), "Furniture".to_string(), 89.99, 55, 4.2),
        ];

        for product in sample_products {
            service.products.lock().unwrap().push(product);
        }

        service
    }

    // Apply filters to products
    fn apply_filters(&self, products: &[Product], filters: &HashMap<String, String>) -> Vec<Product> {
        let mut filtered_products = products.to_vec();

        for (field, value) in filters {
            match field.as_str() {
                "category" => {
                    filtered_products.retain(|p| p.category.to_lowercase().contains(&value.to_lowercase()));
                },
                "min_price" => {
                    if let Ok(min_price) = value.parse::<f64>() {
                        filtered_products.retain(|p| p.price >= min_price);
                    }
                },
                "max_price" => {
                    if let Ok(max_price) = value.parse::<f64>() {
                        filtered_products.retain(|p| p.price <= max_price);
                    }
                },
                "min_stock" => {
                    if let Ok(min_stock) = value.parse::<i32>() {
                        filtered_products.retain(|p| p.stock >= min_stock);
                    }
                },
                "in_stock" => {
                    let in_stock = value.to_lowercase() == "true";
                    filtered_products.retain(|p| (p.stock > 0) == in_stock);
                },
                "min_rating" => {
                    if let Ok(min_rating) = value.parse::<f32>() {
                        filtered_products.retain(|p| p.rating >= min_rating);
                    }
                },
                "search" => {
                    let search_term = value.to_lowercase();
                    filtered_products.retain(|p| {
                        p.name.to_lowercase().contains(&search_term) ||
                        p.category.to_lowercase().contains(&search_term)
                    });
                },
                _ => {} // Unknown filter, ignore
            }
        }

        filtered_products
    }

    // Apply sorting to products
    fn apply_sorting(&self, mut products: Vec<Product>, sort_by: &str, sort_order: &SortOrder) -> Vec<Product> {
        products.sort_by(|a, b| {
            let ordering = match sort_by {
                "name" => a.name.cmp(&b.name),
                "price" => a.price.partial_cmp(&b.price).unwrap_or(std::cmp::Ordering::Equal),
                "stock" => a.stock.cmp(&b.stock),
                "rating" => a.rating.partial_cmp(&b.rating).unwrap_or(std::cmp::Ordering::Equal),
                "created_at" => a.created_at.cmp(&b.created_at),
                "category" => a.category.cmp(&b.category),
                _ => a.name.cmp(&b.name), // Default sort by name
            };

            match sort_order {
                SortOrder::Asc => ordering,
                SortOrder::Desc => ordering.reverse(),
            }
        });

        products
    }

    // Apply pagination to products
    fn apply_pagination(&self, products: Vec<Product>, page: u32, limit: u32) -> (Vec<Product>, u64) {
        let total = products.len() as u64;
        let offset = ((page - 1) * limit) as usize;
        let end = std::cmp::min(offset + limit as usize, products.len());

        let paginated_products = if offset < products.len() {
            products[offset..end].to_vec()
        } else {
            Vec::new()
        };

        (paginated_products, total)
    }
}

impl BackboneHttpHandler<Product> for ProductService {
    fn list(&self, request: ListRequest) -> Result<ApiResponse<Vec<Product>>> {
        let products = self.products.lock().unwrap();
        let active_products: Vec<Product> = products.iter()
            .filter(|p| !p.is_deleted())
            .cloned()
            .collect();

        // Apply filters
        let filtered_products = if let Some(filters) = &request.filters {
            self.apply_filters(&active_products, filters)
        } else {
            active_products
        };

        // Apply sorting
        let sorted_products = if let Some(sort_by) = &request.sort_by {
            let sort_order = request.sort_order.as_ref().unwrap_or(&SortOrder::Asc);
            self.apply_sorting(filtered_products, sort_by, sort_order)
        } else {
            filtered_products
        };

        // Apply pagination
        let page = request.page.unwrap_or(1);
        let limit = request.limit.unwrap_or(20);
        let (paginated_products, total) = self.apply_pagination(sorted_products, page, limit);

        let pagination_response = PaginationResponse::new(total, page, limit);

        println!("📊 Query Results:");
        println!("   Total items: {}", total);
        println!("   Page: {} of {}", page, pagination_response.total_pages);
        println!("   Items per page: {}", limit);
        println!("   Returned items: {}", paginated_products.len());

        Ok(ApiResponse::success(paginated_products, Some("Products listed successfully".to_string())))
    }

    // Simplified implementations for other methods
    fn create(&self, request: Product) -> Result<ApiResponse<Product>> {
        let mut products = self.products.lock().unwrap();
        let mut new_product = request;
        new_product.id = Uuid::new_v4();
        new_product.created_at = chrono::Utc::now();
        new_product.updated_at = chrono::Utc::now();

        products.push(new_product.clone());
        Ok(ApiResponse::success(new_product, Some("Product created successfully".to_string())))
    }

    fn get_by_id(&self, id: &Uuid) -> Result<ApiResponse<Product>> {
        let products = self.products.lock().unwrap();
        match products.iter().find(|product| product.id == *id && !product.is_deleted()) {
            Some(product) => Ok(ApiResponse::success(product.clone(), Some("Product found".to_string()))),
            None => Ok(ApiResponse::error("Product not found".to_string())),
        }
    }

    fn update(&self, id: &Uuid, request: Product) -> Result<ApiResponse<Product>> {
        let mut products = self.products.lock().unwrap();
        match products.iter_mut().find(|product| product.id == *id && !product.is_deleted()) {
            Some(product) => {
                product.name = request.name;
                product.category = request.category;
                product.price = request.price;
                product.stock = request.stock;
                product.rating = request.rating;
                product.updated_at = chrono::Utc::now();
                Ok(ApiResponse::success(product.clone(), Some("Product updated successfully".to_string())))
            },
            None => Ok(ApiResponse::error("Product not found".to_string())),
        }
    }

    fn partial_update(&self, id: &Uuid, fields: HashMap<String, serde_json::Value>) -> Result<ApiResponse<Product>> {
        let mut products = self.products.lock().unwrap();
        match products.iter_mut().find(|product| product.id == *id && !product.is_deleted()) {
            Some(product) => {
                for (field, value) in fields {
                    match field.as_str() {
                        "name" => if let Some(s) = value.as_str() { product.name = s.to_string(); },
                        "category" => if let Some(s) = value.as_str() { product.category = s.to_string(); },
                        "price" => if let Some(n) = value.as_f64() { product.price = n; },
                        "stock" => if let Some(n) = value.as_i64() { product.stock = n as i32; },
                        "rating" => if let Some(n) = value.as_f64() { product.rating = n as f32; },
                        _ => return Ok(ApiResponse::error(format!("Invalid field: {}", field))),
                    }
                }
                product.updated_at = chrono::Utc::now();
                Ok(ApiResponse::success(product.clone(), Some("Product partially updated".to_string())))
            },
            None => Ok(ApiResponse::error("Product not found".to_string())),
        }
    }

    fn soft_delete(&self, id: &Uuid) -> Result<ApiResponse<()>> {
        let mut products = self.products.lock().unwrap();
        match products.iter_mut().find(|product| product.id == *id && !product.is_deleted()) {
            Some(product) => {
                product.deleted_at = Some(chrono::Utc::now());
                product.updated_at = chrono::Utc::now();
                Ok(ApiResponse::success((), Some("Product deleted successfully".to_string())))
            },
            None => Ok(ApiResponse::error("Product not found".to_string())),
        }
    }

    fn bulk_create(&self, request: BulkCreateRequest<Product>) -> Result<ApiResponse<BulkResponse<Product>>> {
        let mut products = self.products.lock().unwrap();
        let mut created_products = Vec::new();

        for product in request.items {
            let mut new_product = product;
            new_product.id = Uuid::new_v4();
            new_product.created_at = chrono::Utc::now();
            new_product.updated_at = chrono::Utc::now();
            created_products.push(new_product.clone());
            products.push(new_product);
        }

        let response = BulkResponse {
            items: created_products.clone(),
            total: created_products.len(),
            failed: 0,
            errors: Vec::new(),
        };

        Ok(ApiResponse::success(response, Some("Bulk create completed".to_string())))
    }

    fn upsert(&self, request: UpsertRequest<Product>) -> Result<ApiResponse<Product>> {
        let mut products = self.products.lock().unwrap();
        let product = request.entity;

        if let Some(existing_product) = products.iter_mut().find(|p| p.name == product.name && !p.is_deleted()) {
            // Update existing product
            existing_product.category = product.category;
            existing_product.price = product.price;
            existing_product.stock = product.stock;
            existing_product.rating = product.rating;
            existing_product.updated_at = chrono::Utc::now();
            Ok(ApiResponse::success(existing_product.clone(), Some("Product updated".to_string())))
        } else if request.create_if_not_exists {
            // Create new product
            let mut new_product = product;
            new_product.id = Uuid::new_v4();
            new_product.created_at = chrono::Utc::now();
            new_product.updated_at = chrono::Utc::now();
            products.push(new_product.clone());
            Ok(ApiResponse::success(new_product, Some("Product created".to_string())))
        } else {
            Ok(ApiResponse::error("Product not found and create_if_not_exists is false".to_string()))
        }
    }

    fn list_deleted(&self, _request: ListRequest) -> Result<ApiResponse<Vec<Product>>> {
        let products = self.products.lock().unwrap();
        let deleted_products: Vec<Product> = products.iter()
            .filter(|product| product.is_deleted())
            .cloned()
            .collect();

        Ok(ApiResponse::success(deleted_products, Some("Deleted products listed".to_string())))
    }

    fn restore(&self, id: &Uuid) -> Result<ApiResponse<Product>> {
        let mut products = self.products.lock().unwrap();
        match products.iter_mut().find(|product| product.id == *id && product.is_deleted()) {
            Some(product) => {
                product.deleted_at = None;
                product.updated_at = chrono::Utc::now();
                Ok(ApiResponse::success(product.clone(), Some("Product restored successfully".to_string())))
            },
            None => Ok(ApiResponse::error("Deleted product not found".to_string())),
        }
    }

    fn empty_trash(&self) -> Result<ApiResponse<()>> {
        let mut products = self.products.lock().unwrap();
        let original_len = products.len();
        products.retain(|product| !product.is_deleted());
        let deleted_count = original_len - products.len();

        Ok(ApiResponse::success((), Some(format!("Permanently deleted {} products", deleted_count))))
    }

    fn get_deleted_by_id(&self, id: &Uuid) -> Result<ApiResponse<Product>> {
        let products = self.products.lock().unwrap();
        match products.iter().find(|p| p.id == *id && p.is_deleted()) {
            Some(product) => Ok(ApiResponse::success(product.clone(), Some("Deleted product found".to_string()))),
            None => Ok(ApiResponse::error("Deleted product not found".to_string())),
        }
    }
}

fn demonstrate_advanced_pagination(service: &ProductService) -> Result<()> {
    println!("\n🔍 Advanced Pagination and Filtering Examples");
    println!("==========================================");

    // Example 1: Basic pagination
    println!("\n1️⃣ Basic Pagination (Page 1, 5 items per page):");
    let basic_request = ListRequest {
        page: Some(1),
        limit: Some(5),
        sort_by: None,
        sort_order: None,
        filters: None,
    };
    let result = service.list(basic_request)?;
    let products = result.data.unwrap();
    for (i, product) in products.iter().enumerate() {
        println!("   {}. {} - {} (${})", i + 1, product.name, product.category, product.price);
    }

    // Example 2: Sort by price (highest first)
    println!("\n2️⃣ Sort by Price (Highest First):");
    let sort_request = ListRequest {
        page: Some(1),
        limit: Some(10),
        sort_by: Some("price".to_string()),
        sort_order: Some(SortOrder::Desc),
        filters: None,
    };
    let result = service.list(sort_request)?;
    let products = result.data.unwrap();
    println!("   Top 5 most expensive products:");
    for (i, product) in products.iter().take(5).enumerate() {
        println!("   {}. {} - ${:.2}", i + 1, product.name, product.price);
    }

    // Example 3: Filter by category
    println!("\n3️⃣ Filter by Category (Electronics):");
    let mut category_filters = HashMap::new();
    category_filters.insert("category".to_string(), "Electronics".to_string());
    let category_request = ListRequest {
        page: Some(1),
        limit: Some(10),
        sort_by: Some("name".to_string()),
        sort_order: Some(SortOrder::Asc),
        filters: Some(category_filters),
    };
    let result = service.list(category_request)?;
    let products = result.data.unwrap();
    println!("   Electronics products:");
    for product in products {
        println!("   - {} - ${:.2} (Rating: {:.1})", product.name, product.price, product.rating);
    }

    // Example 4: Filter by price range
    println!("\n4️⃣ Filter by Price Range ($50 - $200):");
    let mut price_filters = HashMap::new();
    price_filters.insert("min_price".to_string(), "50.0".to_string());
    price_filters.insert("max_price".to_string(), "200.0".to_string());
    let price_request = ListRequest {
        page: Some(1),
        limit: Some(10),
        sort_by: Some("price".to_string()),
        sort_order: Some(SortOrder::Asc),
        filters: Some(price_filters),
    };
    let result = service.list(price_request)?;
    let products = result.data.unwrap();
    println!("   Products between $50 and $200:");
    for product in products {
        println!("   - {} - ${:.2}", product.name, product.rating);
    }

    // Example 5: Filter by stock availability
    println!("\n5️⃣ Filter by In-Stock Items:");
    let mut stock_filters = HashMap::new();
    stock_filters.insert("in_stock".to_string(), "true".to_string());
    stock_filters.insert("min_stock".to_string(), "50".to_string());
    let stock_request = ListRequest {
        page: Some(1),
        limit: Some(10),
        sort_by: Some("stock".to_string()),
        sort_order: Some(SortOrder::Desc),
        filters: Some(stock_filters),
    };
    let result = service.list(stock_request)?;
    let products = result.data.unwrap();
    println!("   Products with 50+ items in stock:");
    for product in products {
        println!("   - {} - {} units", product.name, product.stock);
    }

    // Example 6: Filter by minimum rating
    println!("\n6️⃣ Filter by High Rating (4.5+):");
    let mut rating_filters = HashMap::new();
    rating_filters.insert("min_rating".to_string(), "4.5".to_string());
    let rating_request = ListRequest {
        page: Some(1),
        limit: Some(10),
        sort_by: Some("rating".to_string()),
        sort_order: Some(SortOrder::Desc),
        filters: Some(rating_filters),
    };
    let result = service.list(rating_request)?;
    let products = result.data.unwrap();
    println!("   Highly rated products (4.5+):");
    for product in products {
        println!("   - {} - {:.1}⭐ ({})", product.name, product.rating, product.category);
    }

    // Example 7: Search functionality
    println!("\n7️⃣ Search Products (containing 'Desk' or 'Chair'):");
    let mut search_filters = HashMap::new();
    search_filters.insert("search".to_string(), "desk".to_string());
    let search_request = ListRequest {
        page: Some(1),
        limit: Some(10),
        sort_by: Some("name".to_string()),
        sort_order: Some(SortOrder::Asc),
        filters: Some(search_filters),
    };
    let result = service.list(search_request)?;
    let products = result.data.unwrap();
    println!("   Search results for 'desk':");
    for product in products {
        println!("   - {} - {} (${})", product.name, product.category, product.price);
    }

    // Example 8: Complex filtering with multiple criteria
    println!("\n8️⃣ Complex Filter (Electronics + High Rating + In Stock):");
    let mut complex_filters = HashMap::new();
    complex_filters.insert("category".to_string(), "Electronics".to_string());
    complex_filters.insert("min_rating".to_string(), "4.6".to_string());
    complex_filters.insert("in_stock".to_string(), "true".to_string());
    let complex_request = ListRequest {
        page: Some(1),
        limit: Some(10),
        sort_by: Some("rating".to_string()),
        sort_order: Some(SortOrder::Desc),
        filters: Some(complex_filters),
    };
    let result = service.list(complex_request)?;
    let products = result.data.unwrap();
    println!("   Premium electronics in stock:");
    for product in products {
        println!("   - {} - {:.1}⭐ - ${:.2} - {} units",
                 product.name, product.rating, product.price, product.stock);
    }

    // Example 9: Navigate through pages
    println!("\n9️⃣ Navigate Through Pages (3 items per page):");
    let mut page_num = 1;
    loop {
        let page_request = ListRequest {
            page: Some(page_num),
            limit: Some(3),
            sort_by: Some("name".to_string()),
            sort_order: Some(SortOrder::Asc),
            filters: None,
        };
        let result = service.list(page_request)?;
        let products = result.data.unwrap();

        if products.is_empty() {
            break;
        }

        println!("   Page {}:", page_num);
        for product in &products {
            println!("     - {}", product.name);
        }

        page_num += 1;
        if page_num > 3 { // Limit to avoid too much output
            println!("     ... (and more pages)");
            break;
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    println!("🦴 Backbone Core - Advanced Pagination Example");
    println!("==============================================");

    let service = ProductService::new();

    // Run all pagination demonstrations
    demonstrate_advanced_pagination(&service)?;

    println!("\n🎉 Advanced pagination examples completed successfully!");
    println!("💡 Tips:");
    println!("   - Combine multiple filters for precise queries");
    println!("   - Use pagination for large datasets");
    println!("   - Sort by relevant fields for better UX");
    println!("   - Search functionality works on name and category");

    Ok(())
}