//! Generic bulk operations — create, update, delete multiple entities in one call.
//!
//! Generated code emits a type alias:
//!
//! ```rust,ignore
//! // Generated:
//! pub type StoredFileBulkService = GenericBulkService<
//!     StoredFile,
//!     CreateStoredFileDto,
//!     StoredFileService,
//! >;
//! ```
//!
//! Bulk operations run as individual transactions (one per item) by default.
//! Override `BulkStrategy` to wrap all items in a single transaction.

use async_trait::async_trait;
use std::marker::PhantomData;
use std::sync::Arc;

use crate::service::{ServiceError, ServiceResult};

// ─── Configuration ────────────────────────────────────────────────────────────

/// Controls how bulk operations behave on partial failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulkFailureMode {
    /// Continue processing all items and report per-item errors at the end.
    ContinueOnError,
    /// Abort on the first failure (no partial results).
    AbortOnFirstError,
}

impl Default for BulkFailureMode {
    fn default() -> Self {
        Self::ContinueOnError
    }
}

/// Configuration for a bulk operation.
#[derive(Debug, Clone)]
pub struct BulkOperationConfig {
    /// Maximum number of items per bulk request.
    pub max_batch_size: usize,
    /// How to handle item failures.
    pub failure_mode: BulkFailureMode,
}

impl Default for BulkOperationConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 100,
            failure_mode: BulkFailureMode::ContinueOnError,
        }
    }
}

// ─── Progress tracking ────────────────────────────────────────────────────────

/// Per-item outcome in a bulk operation.
#[derive(Debug, Clone)]
pub struct BulkItemResult<E> {
    pub index: usize,
    pub result: Result<E, String>,
}

impl<E> BulkItemResult<E> {
    pub fn ok(index: usize, entity: E) -> Self {
        Self {
            index,
            result: Ok(entity),
        }
    }

    pub fn err(index: usize, error: impl Into<String>) -> Self {
        Self {
            index,
            result: Err(error.into()),
        }
    }
}

/// Aggregated result of a completed bulk operation.
#[derive(Debug, Clone)]
pub struct BulkOperationResult<E> {
    pub succeeded: Vec<E>,
    pub failed: Vec<(usize, String)>,
    pub total: usize,
}

impl<E: Clone> BulkOperationResult<E> {
    pub fn new() -> Self {
        Self {
            succeeded: Vec::new(),
            failed: Vec::new(),
            total: 0,
        }
    }

    pub fn success_count(&self) -> usize {
        self.succeeded.len()
    }

    pub fn failure_count(&self) -> usize {
        self.failed.len()
    }

    pub fn is_fully_successful(&self) -> bool {
        self.failed.is_empty()
    }

    /// Flatten into an ordered list of per-item results, indexed by insertion order.
    ///
    /// Succeeded items are assigned consecutive indices starting at 0;
    /// failed items use the original input index from `(index, reason)`.
    pub fn into_item_results(self) -> Vec<BulkItemResult<E>> {
        let mut items: Vec<BulkItemResult<E>> = self
            .succeeded
            .into_iter()
            .enumerate()
            .map(|(i, e)| BulkItemResult::ok(i, e))
            .collect();

        for (index, reason) in self.failed {
            items.push(BulkItemResult::err(index, reason));
        }

        items.sort_by_key(|item| item.index);
        items
    }
}

impl<E: Clone> Default for BulkOperationResult<E> {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Progress tracking ────────────────────────────────────────────────────────

/// Tracks live progress of an in-flight bulk operation.
///
/// Useful for streaming progress updates or building progress bars.
#[derive(Debug, Clone, Default)]
pub struct BulkOperationProgress {
    /// Total number of items submitted.
    pub total: usize,
    /// Items processed so far (succeeded + failed).
    pub processed: usize,
    /// Items that completed successfully.
    pub succeeded: usize,
    /// Items that failed.
    pub failed: usize,
}

impl BulkOperationProgress {
    pub fn new(total: usize) -> Self {
        Self {
            total,
            processed: 0,
            succeeded: 0,
            failed: 0,
        }
    }

    pub fn record_success(&mut self) {
        self.processed += 1;
        self.succeeded += 1;
    }

    pub fn record_failure(&mut self) {
        self.processed += 1;
        self.failed += 1;
    }

    pub fn is_complete(&self) -> bool {
        self.processed >= self.total
    }

    pub fn success_rate(&self) -> f64 {
        if self.total == 0 {
            return 1.0;
        }
        self.succeeded as f64 / self.total as f64
    }
}

// ─── Service contract ─────────────────────────────────────────────────────────

/// The minimal contract needed from a service to power bulk operations.
#[async_trait]
pub trait BulkCapableService<E: Send + Sync + 'static, DTO: Send + Sync + 'static>:
    Send + Sync
{
    async fn create_one(&self, dto: DTO) -> ServiceResult<E>;
    async fn delete_one(&self, id: &str) -> ServiceResult<bool>;
}

// ─── GenericBulkService ───────────────────────────────────────────────────────

/// Generic bulk service over any entity+DTO+service triple.
///
/// `E`   — entity type
/// `DTO` — create DTO type
/// `S`   — underlying single-entity service (implements `BulkCapableService<E, DTO>`)
pub struct GenericBulkService<E, DTO, S> {
    service: Arc<S>,
    config: BulkOperationConfig,
    _phantom: PhantomData<(E, DTO)>,
}

impl<E, DTO, S> GenericBulkService<E, DTO, S>
where
    E: Send + Sync + Clone + 'static,
    DTO: Send + Sync + 'static,
    S: BulkCapableService<E, DTO>,
{
    pub fn new(service: Arc<S>) -> Self {
        Self {
            service,
            config: BulkOperationConfig::default(),
            _phantom: PhantomData,
        }
    }

    pub fn with_config(service: Arc<S>, config: BulkOperationConfig) -> Self {
        Self {
            service,
            config,
            _phantom: PhantomData,
        }
    }

    /// Create multiple entities.  Respects `failure_mode` from config.
    ///
    /// Returns `(BulkOperationResult, BulkOperationProgress)` so callers can
    /// inspect final progress stats alongside the per-item outcomes.
    pub async fn bulk_create(
        &self,
        items: Vec<DTO>,
    ) -> ServiceResult<(BulkOperationResult<E>, BulkOperationProgress)> {
        if items.len() > self.config.max_batch_size {
            return Err(ServiceError::Validation(format!(
                "bulk create exceeds maximum batch size of {}",
                self.config.max_batch_size
            )));
        }

        let total = items.len();
        let mut result = BulkOperationResult::new();
        result.total = total;
        let mut progress = BulkOperationProgress::new(total);

        for (index, dto) in items.into_iter().enumerate() {
            match self.service.create_one(dto).await {
                Ok(entity) => {
                    progress.record_success();
                    result.succeeded.push(entity);
                }
                Err(e) => {
                    progress.record_failure();
                    result.failed.push((index, e.to_string()));
                    if self.config.failure_mode == BulkFailureMode::AbortOnFirstError {
                        return Ok((result, progress));
                    }
                }
            }
        }

        Ok((result, progress))
    }

    /// Delete multiple entities by ID.
    pub async fn bulk_delete(
        &self,
        ids: Vec<String>,
    ) -> ServiceResult<(BulkOperationResult<()>, BulkOperationProgress)> {
        if ids.len() > self.config.max_batch_size {
            return Err(ServiceError::Validation(format!(
                "bulk delete exceeds maximum batch size of {}",
                self.config.max_batch_size
            )));
        }

        let total = ids.len();
        let mut result: BulkOperationResult<()> = BulkOperationResult::new();
        result.total = total;
        let mut progress = BulkOperationProgress::new(total);

        for (index, id) in ids.iter().enumerate() {
            match self.service.delete_one(id).await {
                Ok(_) => {
                    progress.record_success();
                    result.succeeded.push(());
                }
                Err(e) => {
                    progress.record_failure();
                    result.failed.push((index, e.to_string()));
                    if self.config.failure_mode == BulkFailureMode::AbortOnFirstError {
                        return Ok((result, progress));
                    }
                }
            }
        }

        Ok((result, progress))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    struct Item {
        id: String,
    }

    struct CreateItemDto {
        id: String,
    }

    struct FakeService {
        fail_ids: Vec<String>,
    }

    #[async_trait]
    impl BulkCapableService<Item, CreateItemDto> for FakeService {
        async fn create_one(&self, dto: CreateItemDto) -> ServiceResult<Item> {
            if self.fail_ids.contains(&dto.id) {
                Err(ServiceError::Internal("injected failure".into()))
            } else {
                Ok(Item { id: dto.id })
            }
        }

        async fn delete_one(&self, id: &str) -> ServiceResult<bool> {
            if self.fail_ids.contains(&id.to_string()) {
                Err(ServiceError::NotFound)
            } else {
                Ok(true)
            }
        }
    }

    #[tokio::test]
    async fn bulk_create_collects_errors_in_continue_mode() {
        let service = Arc::new(FakeService {
            fail_ids: vec!["bad".into()],
        });
        let bulk = GenericBulkService::new(service);

        let dtos = vec![
            CreateItemDto { id: "ok1".into() },
            CreateItemDto { id: "bad".into() },
            CreateItemDto { id: "ok2".into() },
        ];

        let (result, progress) = bulk.bulk_create(dtos).await.unwrap();
        assert_eq!(result.success_count(), 2);
        assert_eq!(result.failure_count(), 1);
        assert_eq!(progress.succeeded, 2);
        assert_eq!(progress.failed, 1);
    }

    #[tokio::test]
    async fn bulk_create_aborts_on_first_error() {
        let service = Arc::new(FakeService {
            fail_ids: vec!["bad".into()],
        });
        let config = BulkOperationConfig {
            max_batch_size: 100,
            failure_mode: BulkFailureMode::AbortOnFirstError,
        };
        let bulk = GenericBulkService::with_config(service, config);

        let dtos = vec![
            CreateItemDto { id: "bad".into() },
            CreateItemDto { id: "ok".into() },
        ];

        let (result, progress) = bulk.bulk_create(dtos).await.unwrap();
        // Aborted after first failure — second item never processed
        assert_eq!(result.success_count(), 0);
        assert_eq!(result.failure_count(), 1);
        assert!(progress.is_complete() == false); // aborted early
    }

    #[tokio::test]
    async fn bulk_create_rejects_oversized_batch() {
        let service = Arc::new(FakeService { fail_ids: vec![] });
        let config = BulkOperationConfig {
            max_batch_size: 2,
            failure_mode: BulkFailureMode::ContinueOnError,
        };
        let bulk = GenericBulkService::with_config(service, config);

        let dtos: Vec<_> = (0..3)
            .map(|i| CreateItemDto {
                id: i.to_string(),
            })
            .collect();

        assert!(bulk.bulk_create(dtos).await.is_err()); // batch size exceeded
    }
}
