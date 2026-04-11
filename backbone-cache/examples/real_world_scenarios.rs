//! Real-world caching scenarios for backbone-cache
//! Demonstrates practical use cases found in production applications

use backbone_cache::{RedisCache, MemoryCache, CacheKey};
use serde::{Serialize, Deserialize};
use std::time::Duration;
use tokio::time::sleep;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use std::sync::Arc;

// Real-world domain models
#[derive(Serialize, Deserialize, Debug, Clone)]
struct Product {
    id: String,
    name: String,
    description: String,
    price: f64,
    category_id: String,
    inventory_count: u32,
    images: Vec<String>,
    attributes: HashMap<String, String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ProductCategory {
    id: String,
    name: String,
    slug: String,
    description: Option<String>,
    parent_id: Option<String>,
    product_count: u32,
    is_active: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ShoppingCart {
    id: String,
    user_id: String,
    items: Vec<CartItem>,
    subtotal: f64,
    tax: f64,
    total: f64,
    currency: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct CartItem {
    product_id: String,
    quantity: u32,
    unit_price: f64,
    total_price: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct UserProfile {
    id: String,
    username: String,
    email: String,
    first_name: Option<String>,
    last_name: Option<String>,
    avatar_url: Option<String>,
    preferences: UserPreferences,
    subscription: SubscriptionInfo,
    stats: UserStats,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct UserPreferences {
    theme: String,
    language: String,
    currency: String,
    notifications: NotificationSettings,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct NotificationSettings {
    email: bool,
    push: bool,
    sms: bool,
    marketing: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SubscriptionInfo {
    tier: String,
    status: String,
    expires_at: Option<DateTime<Utc>>,
    features: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct UserStats {
    orders_count: u32,
    total_spent: f64,
    favorite_categories: Vec<String>,
    loyalty_points: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ApiRateLimit {
    client_ip: String,
    endpoint: String,
    requests_count: u32,
    window_start: DateTime<Utc>,
    window_duration_seconds: u32,
    limit: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SessionData {
    session_id: String,
    user_id: Option<String>,
    ip_address: String,
    user_agent: String,
    last_activity: DateTime<Utc>,
    data: HashMap<String, String>,
    is_authenticated: bool,
}

// E-commerce Cache Service
struct EcommerceCacheService {
    redis_cache: RedisCache,
    memory_cache: MemoryCache,
    default_product_ttl: u64,
    default_category_ttl: u64,
    default_cart_ttl: u64,
}

impl EcommerceCacheService {
    async fn new(redis_url: &str) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            redis_cache: RedisCache::new(redis_url).await?,
            memory_cache: MemoryCache::new(Some(10000)), // 10k products in memory
            default_product_ttl: 3600,    // 1 hour
            default_category_ttl: 7200,   // 2 hours
            default_cart_ttl: 1800,       // 30 minutes
        })
    }

    // SCENARIO 1: Product Catalog Caching
    async fn cache_product(&self, product: &Product) -> Result<(), Box<dyn std::error::Error>> {
        println!("📦 Caching product: {}", product.name);

        // Cache product details in both memory and Redis
        let product_key = CacheKey::build("product", &product.id);

        // Memory cache for frequently accessed products
        self.memory_cache.set(&product_key, product, Some(self.default_product_ttl)).await?;

        // Redis cache for persistence and cross-service access
        self.redis_cache.set(&product_key, product, Some(self.default_product_ttl * 2)).await?;

        // Cache product by category for category listings
        let category_key = CacheKey::build("category_products", &product.category_id);

        // Add to category product list (this would be a set or sorted set in real implementation)
        let mut category_products: Vec<String> = self.redis_cache
            .get(&category_key)
            .await?
            .unwrap_or_default();

        if !category_products.contains(&product.id) {
            category_products.push(product.id.clone());
            self.redis_cache.set(&category_key, &category_products, Some(self.default_product_ttl)).await?;
        }

        // Cache search keywords (extract from name and description)
        let search_keywords = self.extract_search_keywords(product);
        for keyword in search_keywords {
            let search_key = CacheKey::build("search", &keyword);
            let mut search_results: Vec<String> = self.redis_cache
                .get(&search_key)
                .await?
                .unwrap_or_default();

            if !search_results.contains(&product.id) && search_results.len() < 50 {
                search_results.push(product.id.clone());
                self.redis_cache.set(&search_key, &search_results, Some(self.default_product_ttl)).await?;
            }
        }

        Ok(())
    }

    async fn get_product(&self, product_id: &str) -> Result<Option<Product>, Box<dyn std::error::Error>> {
        let product_key = CacheKey::build("product", product_id);

        // Try memory cache first
        if let Some(product) = self.memory_cache.get::<Product>(&product_key).await? {
            println!("🚀 Product {} found in memory cache", product_id);
            return Ok(Some(product));
        }

        // Fall back to Redis cache
        if let Some(product) = self.redis_cache.get::<Product>(&product_key).await? {
            println!("🔗 Product {} found in Redis cache", product_id);

            // Promote to memory cache for faster future access
            self.memory_cache.set(&product_key, &product, Some(self.default_product_ttl)).await?;
            return Ok(Some(product));
        }

        println!("❌ Product {} not found in cache", product_id);
        Ok(None)
    }

    async fn cache_product_category(&self, category: &ProductCategory) -> Result<(), Box<dyn std::error::Error>> {
        println!("📂 Caching category: {}", category.name);

        let category_key = CacheKey::build("category", &category.id);

        // Cache category in both layers
        self.memory_cache.set(&category_key, category, Some(self.default_category_ttl)).await?;
        self.redis_cache.set(&category_key, category, Some(self.default_category_ttl * 2)).await?;

        // Cache active categories list
        let active_categories_key = CacheKey::build("categories", "active");
        let mut active_categories: Vec<ProductCategory> = self.redis_cache
            .get(&active_categories_key)
            .await?
            .unwrap_or_default();

        if !active_categories.iter().any(|c| c.id == category.id) {
            active_categories.push(category.clone());
            self.redis_cache.set(&active_categories_key, &active_categories, Some(self.default_category_ttl)).await?;
        }

        Ok(())
    }

    async fn get_category_products(&self, category_id: &str, page: u32, limit: u32) -> Result<Vec<Product>, Box<dyn std::error::Error>> {
        println!("📂 Getting products for category: {} (page: {}, limit: {})", category_id, page, limit);

        // Get category product IDs
        let category_key = CacheKey::build("category_products", category_id);
        let product_ids: Vec<String> = self.redis_cache
            .get(&category_key)
            .await?
            .unwrap_or_default();

        // Paginate
        let start = (page - 1) * limit;
        let end = start + limit;
        let paginated_ids: Vec<String> = product_ids.into_iter()
            .skip(start as usize)
            .take(limit as usize)
            .collect();

        if paginated_ids.is_empty() {
            return Ok(vec![]);
        }

        // Batch get products
        let product_keys: Vec<String> = paginated_ids.iter()
            .map(|id| CacheKey::build("product", id))
            .collect();

        let mut products = Vec::new();
        let batch_results = self.redis_cache.mget::<Product>(product_keys).await?;

        for (_key, product_opt) in batch_results {
            if let Some(product) = product_opt {
                products.push(product);
            }
        }

        println!("✅ Found {} products for category {}", products.len(), category_id);
        Ok(products)
    }

    // SCENARIO 2: Shopping Cart Caching
    async fn cache_shopping_cart(&self, cart: &ShoppingCart) -> Result<(), Box<dyn std::error::Error>> {
        println!("🛒 Caching shopping cart for user: {}", cart.user_id);

        let cart_key = CacheKey::build("cart", &cart.user_id);

        // Cache cart with shorter TTL (carts change frequently)
        self.redis_cache.set(&cart_key, cart, Some(self.default_cart_ttl)).await?;

        // Also cache a lightweight summary for quick display
        let cart_summary = CartSummary {
            id: cart.id.clone(),
            user_id: cart.user_id.clone(),
            items_count: cart.items.len() as u32,
            total: cart.total,
            currency: cart.currency.clone(),
        };

        let summary_key = CacheKey::build("cart_summary", &cart.user_id);
        self.memory_cache.set(&summary_key, &cart_summary, Some(self.default_cart_ttl)).await?;

        Ok(())
    }

    async fn get_shopping_cart(&self, user_id: &str) -> Result<Option<ShoppingCart>, Box<dyn std::error::Error>> {
        let cart_key = CacheKey::build("cart", user_id);

        let cart: Option<ShoppingCart> = self.redis_cache.get(&cart_key).await?;

        match cart {
            Some(cart) => {
                println!("🛒 Found shopping cart for user: {} ({} items)", user_id, cart.items.len());
                Ok(Some(cart))
            }
            None => {
                println!("🛒 No shopping cart found for user: {}", user_id);
                Ok(None)
            }
        }
    }

    async fn update_cart_item(&self, user_id: &str, product_id: &str, quantity: u32) -> Result<(), Box<dyn std::error::Error>> {
        println!("🛒 Updating cart for user {}: product {} quantity {}", user_id, product_id, quantity);

        let cart_key = CacheKey::build("cart", user_id);
        let mut cart: Option<ShoppingCart> = self.redis_cache.get(&cart_key).await?;

        if let Some(mut cart) = cart {
            // Find and update the item
            if let Some(item) = cart.items.iter_mut().find(|i| i.product_id == product_id) {
                item.quantity = quantity;
                item.total_price = item.unit_price * quantity as f64;
            } else {
                // Item not found, need to fetch product info (in real implementation)
                println!("⚠️ Product {} not found in cart, would fetch from database", product_id);
            }

            // Recalculate totals
            cart.subtotal = cart.items.iter().map(|i| i.total_price).sum();
            cart.tax = cart.subtotal * 0.08; // 8% tax
            cart.total = cart.subtotal + cart.tax;
            cart.updated_at = Utc::now();

            // Recache the updated cart
            self.cache_shopping_cart(&cart).await?;
        } else {
            println!("❌ No cart found for user: {}", user_id);
        }

        Ok(())
    }

    // SCENARIO 3: User Profile and Preferences Caching
    async fn cache_user_profile(&self, user: &UserProfile) -> Result<(), Box<dyn std::error::Error>> {
        println!("👤 Caching user profile: {}", user.username);

        let profile_key = CacheKey::user(&user.id);

        // Cache full profile
        self.memory_cache.set(&profile_key, user, Some(1800)).await?; // 30 min
        self.redis_cache.set(&profile_key, user, Some(3600)).await?;   // 1 hour

        // Cache frequently accessed data separately
        let preferences_key = CacheKey::build("user_preferences", &user.id);
        self.memory_cache.set(&preferences_key, &user.preferences, Some(3600)).await?;

        let subscription_key = CacheKey::build("user_subscription", &user.id);
        self.memory_cache.set(&subscription_key, &user.subscription, Some(1800)).await?;

        Ok(())
    }

    async fn get_user_preferences(&self, user_id: &str) -> Result<Option<UserPreferences>, Box<dyn std::error::Error>> {
        let preferences_key = CacheKey::build("user_preferences", user_id);

        let preferences: Option<UserPreferences> = self.memory_cache.get(&preferences_key).await?;

        match preferences {
            Some(pref) => {
                println!("⚙️ Found preferences for user {}: {} theme", user_id, pref.theme);
                Ok(Some(pref))
            }
            None => {
                println!("⚙️ No preferences cached for user: {}", user_id);
                Ok(None)
            }
        }
    }

    // SCENARIO 4: API Rate Limiting
    async fn check_rate_limit(&self, client_ip: &str, endpoint: &str, limit: u32, window_seconds: u32) -> Result<RateLimitResult, Box<dyn std::error::Error>> {
        let rate_limit_key = format!("rate_limit:{}:{}", endpoint, client_ip);

        let mut rate_limit: Option<ApiRateLimit> = self.redis_cache.get(&rate_limit_key).await?;

        let now = Utc::now();

        if let Some(mut rl) = rate_limit {
            // Check if we're still within the current window
            let window_end = rl.window_start + chrono::Duration::seconds(rl.window_duration_seconds as i64);

            if now < window_end {
                // Same window, increment counter
                rl.requests_count += 1;

                if rl.requests_count > limit {
                    println!("🚫 Rate limit exceeded for {} on {} ({} > {})", client_ip, endpoint, rl.requests_count, limit);
                    return Ok(RateLimitResult {
                        allowed: false,
                        remaining: 0,
                        reset_time: window_end,
                        retry_after: window_end.timestamp() - now.timestamp(),
                    });
                }

                let remaining = limit - rl.requests_count;
                println!("✅ Rate limit check passed for {} on {} ({} remaining)", client_ip, endpoint, remaining);

                // Update the rate limit counter
                self.redis_cache.set(&rate_limit_key, &rl, Some(window_seconds)).await?;

                Ok(RateLimitResult {
                    allowed: true,
                    remaining,
                    reset_time: window_end,
                    retry_after: 0,
                })
            } else {
                // New window, reset counter
                let new_rl = ApiRateLimit {
                    client_ip: client_ip.to_string(),
                    endpoint: endpoint.to_string(),
                    requests_count: 1,
                    window_start: now,
                    window_duration_seconds: window_seconds,
                    limit,
                };

                self.redis_cache.set(&rate_limit_key, &new_rl, Some(window_seconds)).await?;

                Ok(RateLimitResult {
                    allowed: true,
                    remaining: limit - 1,
                    reset_time: now + chrono::Duration::seconds(window_seconds as i64),
                    retry_after: 0,
                })
            }
        } else {
            // First request in this window
            let new_rl = ApiRateLimit {
                client_ip: client_ip.to_string(),
                endpoint: endpoint.to_string(),
                requests_count: 1,
                window_start: now,
                window_duration_seconds: window_seconds,
                limit,
            };

            self.redis_cache.set(&rate_limit_key, &new_rl, Some(window_seconds)).await?;

            println!("✅ First request for {} on {}, starting rate limit window", client_ip, endpoint);

            Ok(RateLimitResult {
                allowed: true,
                remaining: limit - 1,
                reset_time: now + chrono::Duration::seconds(window_seconds as i64),
                retry_after: 0,
            })
        }
    }

    // SCENARIO 5: Session Management
    async fn create_session(&self, session_id: &str, user_id: Option<String>, ip_address: &str, user_agent: &str) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔐 Creating session: {} for user: {:?}", session_id, user_id);

        let session_data = SessionData {
            session_id: session_id.to_string(),
            user_id,
            ip_address: ip_address.to_string(),
            user_agent: user_agent.to_string(),
            last_activity: Utc::now(),
            data: HashMap::new(),
            is_authenticated: user_id.is_some(),
        };

        let session_key = CacheKey::session(session_id);
        self.redis_cache.set(&session_key, &session_data, Some(1800)).await?; // 30 min

        // If authenticated, add to user's active sessions
        if let Some(uid) = &session_data.user_id {
            let active_sessions_key = CacheKey::build("user_sessions", uid);
            let mut active_sessions: Vec<String> = self.redis_cache
                .get(&active_sessions_key)
                .await?
                .unwrap_or_default();

            active_sessions.push(session_id.to_string());

            // Keep only last 5 sessions per user
            if active_sessions.len() > 5 {
                active_sessions = active_sessions.split_off(active_sessions.len() - 5);
            }

            self.redis_cache.set(&active_sessions_key, &active_sessions, Some(3600)).await?;
        }

        Ok(())
    }

    async fn get_session(&self, session_id: &str) -> Result<Option<SessionData>, Box<dyn std::error::Error>> {
        let session_key = CacheKey::session(session_id);

        let mut session: Option<SessionData> = self.redis_cache.get(&session_key).await?;

        if let Some(mut session_data) = session {
            // Update last activity time
            session_data.last_activity = Utc::now();

            // Extend session TTL (sliding expiration)
            self.redis_cache.set(&session_key, &session_data, Some(1800)).await?;

            println!("🔐 Session {} found and refreshed", session_id);
            Ok(Some(session_data))
        } else {
            println!("🔐 Session {} not found or expired", session_id);
            Ok(None)
        }
    }

    async fn invalidate_user_sessions(&self, user_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔐 Invalidating all sessions for user: {}", user_id);

        let active_sessions_key = CacheKey::build("user_sessions", user_id);
        let active_sessions: Vec<String> = self.redis_cache
            .get(&active_sessions_key)
            .await?
            .unwrap_or_default();

        for session_id in active_sessions {
            let session_key = CacheKey::session(&session_id);
            self.redis_cache.delete(&session_key).await?;
            println!("🔐 Invalidated session: {}", session_id);
        }

        // Clear active sessions list
        self.redis_cache.delete(&active_sessions_key).await?;

        Ok(())
    }

    // Helper methods
    fn extract_search_keywords(&self, product: &Product) -> Vec<String> {
        let mut keywords = HashSet::new();

        // Extract words from product name
        for word in product.name.to_lowercase().split_whitespace() {
            if word.len() > 2 {
                keywords.insert(word.to_string());
            }
        }

        // Extract words from description
        for word in product.description.to_lowercase().split_whitespace() {
            if word.len() > 2 {
                keywords.insert(word.to_string());
            }
        }

        // Add category-specific keywords
        keywords.insert(product.category_id.clone());

        keywords.into_iter().collect()
    }

    // SCENARIO 6: Cache Invalidation Strategies
    async fn invalidate_product_cache(&self, product_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        println!("🗑️ Invalidating cache for product: {}", product_id);

        let product_key = CacheKey::build("product", product_id);

        // Remove from memory cache
        self.memory_cache.delete(&product_key).await?;

        // Remove from Redis cache
        self.redis_cache.delete(&product_key).await?;

        // Invalidate category caches that might contain this product
        // In a real implementation, you'd track which categories this product belongs to

        println!("✅ Product cache invalidated");
        Ok(())
    }

    async fn invalidate_user_cache(&self, user_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        println!("🗑️ Invalidating cache for user: {}", user_id);

        // Remove profile and related caches
        let profile_key = CacheKey::user(user_id);
        self.memory_cache.delete(&profile_key).await?;
        self.redis_cache.delete(&profile_key).await?;

        let preferences_key = CacheKey::build("user_preferences", user_id);
        self.memory_cache.delete(&preferences_key).await?;

        let subscription_key = CacheKey::build("user_subscription", user_id);
        self.memory_cache.delete(&subscription_key).await?;

        // Invalidate all user sessions
        self.invalidate_user_sessions(user_id).await?;

        println!("✅ User cache invalidated");
        Ok(())
    }

    // SCENARIO 7: Cache Warming for High Traffic Events
    async fn warm_up_for_flash_sale(&self, product_ids: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔥 Warming up cache for flash sale: {} products", product_ids.len());

        // Pre-load popular products into memory cache
        for product_id in &product_ids {
            let product_key = CacheKey::build("product", product_id);

            if let Some(product) = self.redis_cache.get::<Product>(&product_key).await? {
                // Cache in memory for faster access during sale
                self.memory_cache.set(&product_key, &product, Some(300)).await?; // 5 min
                println!("🚀 Pre-loaded product into memory cache: {}", product_id);
            }
        }

        // Pre-warm category pages
        let popular_categories = vec!["electronics", "clothing", "home", "sports"];
        for category in popular_categories {
            let category_key = CacheKey::build("category_products", category);
            let products: Vec<String> = self.redis_cache.get(&category_key).await?.unwrap_or_default();

            println!("📂 Pre-warmed category {} with {} products", category, products.len());
        }

        println!("✅ Flash sale cache warming completed");
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct CartSummary {
    id: String,
    user_id: String,
    items_count: u32,
    total: f64,
    currency: String,
}

#[derive(Debug, Clone)]
struct RateLimitResult {
    allowed: bool,
    remaining: u32,
    reset_time: DateTime<Utc>,
    retry_after: i64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Backbone Cache Real-World Scenarios ===\n");

    let cache_service = EcommerceCacheService::new("redis://localhost:6379").await
        .map_err(|e| println!("⚠️ Redis not available, some scenarios will be skipped: {}", e))?;

    // SCENARIO 1: E-commerce Product Catalog
    println!("🏪 SCENARIO 1: E-commerce Product Catalog Caching");
    println!("==================================================");

    // Create sample products
    let products = vec![
        Product {
            id: "prod_001".to_string(),
            name: "Wireless Bluetooth Headphones".to_string(),
            description: "Premium noise-cancelling wireless headphones with 30-hour battery life".to_string(),
            price: 199.99,
            category_id: "electronics".to_string(),
            inventory_count: 150,
            images: vec!["headphones_1.jpg".to_string(), "headphones_2.jpg".to_string()],
            attributes: {
                let mut attrs = HashMap::new();
                attrs.insert("brand".to_string(), "AudioTech".to_string());
                attrs.insert("color".to_string(), "Black".to_string());
                attrs.insert("weight".to_string(), "250g".to_string());
                attrs
            },
            created_at: Utc::now() - chrono::Duration::days(30),
            updated_at: Utc::now(),
        },
        Product {
            id: "prod_002".to_string(),
            name: "Organic Cotton T-Shirt".to_string(),
            description: "Comfortable and sustainable organic cotton t-shirt".to_string(),
            price: 29.99,
            category_id: "clothing".to_string(),
            inventory_count: 500,
            images: vec!["tshirt_blue.jpg".to_string(), "tshirt_white.jpg".to_string()],
            attributes: {
                let mut attrs = HashMap::new();
                attrs.insert("material".to_string(), "Organic Cotton".to_string());
                attrs.insert("sizes".to_string(), "S,M,L,XL".to_string());
                attrs
            },
            created_at: Utc::now() - chrono::Duration::days(15),
            updated_at: Utc::now(),
        },
    ];

    // Cache products
    for product in &products {
        cache_service.cache_product(product).await?;
    }

    // Test product retrieval
    for product in &products {
        let cached_product = cache_service.get_product(&product.id).await?;
        match cached_product {
            Some(p) => println!("✅ Retrieved cached product: {}", p.name),
            None => println!("❌ Product not found in cache: {}", product.id),
        }
    }

    // Create and cache categories
    let categories = vec![
        ProductCategory {
            id: "electronics".to_string(),
            name: "Electronics".to_string(),
            slug: "electronics".to_string(),
            description: Some("Latest electronic gadgets and devices".to_string()),
            parent_id: None,
            product_count: 245,
            is_active: true,
        },
        ProductCategory {
            id: "clothing".to_string(),
            name: "Clothing".to_string(),
            slug: "clothing".to_string(),
            description: Some("Fashionable and sustainable clothing".to_string()),
            parent_id: None,
            product_count: 890,
            is_active: true,
        },
    ];

    for category in &categories {
        cache_service.cache_product_category(category).await?;
    }

    // Test category product listing
    let category_products = cache_service.get_category_products("electronics", 1, 10).await?;
    println!("📂 Found {} products in electronics category", category_products.len());

    // SCENARIO 2: Shopping Cart Management
    println!("\n🛒 SCENARIO 2: Shopping Cart Management");
    println!("======================================");

    let shopping_cart = ShoppingCart {
        id: Uuid::new_v4().to_string(),
        user_id: "user_123".to_string(),
        items: vec![
            CartItem {
                product_id: "prod_001".to_string(),
                quantity: 1,
                unit_price: 199.99,
                total_price: 199.99,
            },
            CartItem {
                product_id: "prod_002".to_string(),
                quantity: 2,
                unit_price: 29.99,
                total_price: 59.98,
            },
        ],
        subtotal: 259.97,
        tax: 20.80,
        total: 280.77,
        currency: "USD".to_string(),
        created_at: Utc::now() - chrono::Duration::minutes(15),
        updated_at: Utc::now(),
    };

    cache_service.cache_shopping_cart(&shopping_cart).await?;

    // Retrieve cart
    let retrieved_cart = cache_service.get_shopping_cart("user_123").await?;
    match retrieved_cart {
        Some(cart) => {
            println!("✅ Retrieved shopping cart: ${:.2} ({} items)", cart.total, cart.items.len());

            // Update cart item
            cache_service.update_cart_item("user_123", "prod_002", 3).await?;
            println!("🛒 Updated cart item quantity");
        }
        None => println!("❌ Shopping cart not found"),
    }

    // SCENARIO 3: User Profile Caching
    println!("\n👤 SCENARIO 3: User Profile Caching");
    println!("==================================");

    let user_profile = UserProfile {
        id: "user_123".to_string(),
        username: "john_doe".to_string(),
        email: "john@example.com".to_string(),
        first_name: Some("John".to_string()),
        last_name: Some("Doe".to_string()),
        avatar_url: Some("https://example.com/avatars/john.jpg".to_string()),
        preferences: UserPreferences {
            theme: "dark".to_string(),
            language: "en".to_string(),
            currency: "USD".to_string(),
            notifications: NotificationSettings {
                email: true,
                push: false,
                sms: false,
                marketing: true,
            },
        },
        subscription: SubscriptionInfo {
            tier: "premium".to_string(),
            status: "active".to_string(),
            expires_at: Some(Utc::now() + chrono::Duration::days(30)),
            features: vec!["free_shipping".to_string(), "early_access".to_string(), "premium_support".to_string()],
        },
        stats: UserStats {
            orders_count: 23,
            total_spent: 1847.50,
            favorite_categories: vec!["electronics".to_string(), "clothing".to_string()],
            loyalty_points: 1847,
        },
    };

    cache_service.cache_user_profile(&user_profile).await?;

    // Test preference retrieval
    let preferences = cache_service.get_user_preferences("user_123").await?;
    match preferences {
        Some(pref) => println!("⚙️ User preferences: {} theme, {} language, {} notifications",
            pref.theme, pref.language, if pref.notifications.email { "enabled" } else { "disabled" }),
        None => println!("❌ User preferences not found"),
    }

    // SCENARIO 4: API Rate Limiting
    println!("\n🚦 SCENARIO 4: API Rate Limiting");
    println!("===============================");

    let client_ip = "192.168.1.100";
    let endpoint = "/api/v1/products";

    // Test rate limiting
    for i in 1..=5 {
        let result = cache_service.check_rate_limit(client_ip, endpoint, 10, 60).await?;
        println!("Request {}: {} ({} remaining)", i,
            if result.allowed { "✅ Allowed" } else { "🚫 Blocked" },
            result.remaining);
    }

    // Test with stricter limit
    println!("\nTesting with stricter rate limit (3 requests per minute):");
    for i in 1..=5 {
        let result = cache_service.check_rate_limit(client_ip, "/api/v1/search", 3, 60).await?;
        println!("Search request {}: {} ({} remaining, retry after: {}s)", i,
            if result.allowed { "✅ Allowed" } else { "🚫 Blocked" },
            result.remaining,
            result.retry_after);
    }

    // SCENARIO 5: Session Management
    println!("\n🔐 SCENARIO 5: Session Management");
    println!("=================================");

    let session_id = Uuid::new_v4().to_string();
    let ip_address = "203.0.113.1";
    let user_agent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";

    // Create session
    cache_service.create_session(&session_id, Some("user_123".to_string()), ip_address, user_agent).await?;

    // Retrieve session
    let session = cache_service.get_session(&session_id).await?;
    match session {
        Some(s) => println!("✅ Session found for user: {:?}, authenticated: {}",
            s.user_id, s.is_authenticated),
        None => println!("❌ Session not found"),
    }

    // Create anonymous session
    let anon_session_id = Uuid::new_v4().to_string();
    cache_service.create_session(&anon_session_id, None, "203.0.113.2", "curl/7.68.0").await?;

    // SCENARIO 6: Cache Invalidation
    println!("\n🗑️ SCENARIO 6: Cache Invalidation");
    println!("=================================");

    // Invalidate product cache
    cache_service.invalidate_product_cache("prod_001").await?;

    // Verify product is no longer in memory cache
    let cached_product = cache_service.memory_cache.get::<Product>(&CacheKey::build("product", "prod_001")).await?;
    println!("Product prod_001 in memory cache after invalidation: {}",
        if cached_product.is_some() { "✅ Found" } else { "❌ Not found" });

    // Invalidate user cache
    cache_service.invalidate_user_cache("user_123").await?;

    // SCENARIO 7: Flash Sale Cache Warming
    println!("\n🔥 SCENARIO 7: Flash Sale Cache Warming");
    println!("=====================================");

    let flash_sale_products = vec!["prod_001".to_string(), "prod_002".to_string()];
    cache_service.warm_up_for_flash_sale(flash_sale_products).await?;

    // SCENARIO 8: Search Functionality Caching
    println!("\n🔍 SCENARIO 8: Search Functionality Caching");
    println!("===========================================");

    // Simulate search queries
    let search_queries = vec!["bluetooth", "headphones", "cotton", "t-shirt"];

    for query in search_queries {
        let search_key = CacheKey::build("search", query);
        let search_results: Vec<String> = cache_service.redis_cache.get(&search_key).await?.unwrap_or_default();
        println!("🔍 Search '{}': {} products cached", query, search_results.len());
    }

    // SCENARIO 9: Analytics and Statistics
    println!("\n📊 SCENARIO 9: Cache Performance Statistics");
    println!("===========================================");

    let memory_stats = cache_service.memory_cache.stats().await?;
    let redis_stats = cache_service.redis_cache.stats().await?;

    println!("🧠 Memory Cache:");
    println!("   Total entries: {}", memory_stats.total_entries);
    println!("   Hit rate: {:.2}%", memory_stats.hit_rate * 100.0);
    println!("   Operations: {} hits, {} misses", memory_stats.hits, memory_stats.misses);

    println!("🔗 Redis Cache:");
    println!("   Total entries: {}", redis_stats.total_entries);
    println!("   Hit rate: {:.2}%", redis_stats.hit_rate * 100.0);
    println!("   Operations: {} hits, {} misses", redis_stats.hits, redis_stats.misses);

    if let Some(memory_usage) = redis_stats.memory_usage {
        println!("   Memory usage: {:.2} MB", memory_usage as f64 / 1024.0 / 1024.0);
    }

    println!("\n🎉 Real-World Scenarios Complete!");
    println!("==================================");
    println!("✅ E-commerce product catalog caching");
    println!("✅ Shopping cart management");
    println!("✅ User profile and preferences caching");
    println!("✅ API rate limiting");
    println!("✅ Session management");
    println!("✅ Cache invalidation strategies");
    println!("✅ Flash sale cache warming");
    println!("✅ Search functionality caching");
    println!("✅ Performance monitoring");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ecommerce_cache_creation() {
        let result = EcommerceCacheService::new("redis://invalid:6379").await;
        assert!(result.is_err()); // Should fail gracefully with invalid Redis
    }

    #[tokio::test]
    async fn test_product_caching() {
        let cache_service = EcommerceCacheService::new("redis://localhost:6379").await;

        if let Ok(service) = cache_service {
            let product = Product {
                id: "test_product".to_string(),
                name: "Test Product".to_string(),
                description: "Test Description".to_string(),
                price: 99.99,
                category_id: "test_category".to_string(),
                inventory_count: 10,
                images: vec![],
                attributes: HashMap::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };

            // Test caching
            let result = service.cache_product(&product).await;
            assert!(result.is_ok());

            // Test retrieval
            let retrieved = service.get_product("test_product").await.unwrap();
            assert!(retrieved.is_some());
            assert_eq!(retrieved.unwrap().name, "Test Product");
        }
    }

    #[tokio::test]
    async fn test_rate_limiting() {
        let cache_service = EcommerceCacheService::new("redis://localhost:6379").await;

        if let Ok(service) = cache_service {
            let client_ip = "127.0.0.1";
            let endpoint = "/test";

            // First request should be allowed
            let result1 = service.check_rate_limit(client_ip, endpoint, 2, 60).await.unwrap();
            assert!(result1.allowed);
            assert_eq!(result1.remaining, 1);

            // Second request should be allowed
            let result2 = service.check_rate_limit(client_ip, endpoint, 2, 60).await.unwrap();
            assert!(result2.allowed);
            assert_eq!(result2.remaining, 0);

            // Third request should be blocked
            let result3 = service.check_rate_limit(client_ip, endpoint, 2, 60).await.unwrap();
            assert!(!result3.allowed);
            assert_eq!(result3.remaining, 0);
        }
    }

    #[tokio::test]
    async fn test_session_management() {
        let cache_service = EcommerceCacheService::new("redis://localhost:6379").await;

        if let Ok(service) = cache_service {
            let session_id = "test_session";
            let user_id = "test_user";
            let ip = "127.0.0.1";
            let user_agent = "test-agent";

            // Create session
            let result = service.create_session(session_id, Some(user_id.to_string()), ip, user_agent).await;
            assert!(result.is_ok());

            // Retrieve session
            let session = service.get_session(session_id).await.unwrap();
            assert!(session.is_some());
            assert_eq!(session.unwrap().user_id, Some(user_id.to_string()));
        }
    }
}