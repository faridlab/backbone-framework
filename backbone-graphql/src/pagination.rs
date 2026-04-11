//! Pagination types for GraphQL queries

use async_graphql::*;
use serde::Serialize;

/// Pagination metadata
#[derive(Debug, Clone, SimpleObject, Serialize)]
pub struct PaginationInfo {
    pub total: i64,
    pub page: i32,
    pub limit: i32,
    pub total_pages: i32,
}

impl PaginationInfo {
    pub fn new(total: u64, page: u32, limit: u32) -> Self {
        let total = total as i64;
        let page = page as i32;
        let limit = limit as i32;
        let total_pages = if limit == 0 {
            0
        } else {
            ((total as f64) / (limit as f64)).ceil() as i32
        };
        Self {
            total,
            page,
            limit,
            total_pages,
        }
    }
}
