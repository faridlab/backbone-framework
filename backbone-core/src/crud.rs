//! CRUD operations and generic implementations

// Note: CrudRepository is defined in repository.rs to avoid duplication
// This file is kept for backwards compatibility but may be deprecated

/// Re-export CrudRepository from repository module for backwards compatibility
pub use crate::repository::CrudRepository;

// Placeholder implementations for submodules will be added in Priority 2
// pub mod entity;
// pub mod repository;
// pub mod http;
// pub mod grpc;