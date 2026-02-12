//! Shared Application State
//!
//! Thread-safe state for tracking consolidations and sync status.

use dashmap::DashMap;
use parking_lot::RwLock;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Status of a consolidation claim
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStatus {
    /// Detected in beacon state
    Detected,
    /// Proof generated
    ProofBuilt,
    /// Transaction submitted
    Submitted,
    /// Transaction confirmed
    Confirmed,
    /// Claim failed
    Failed,
}

/// Record for a tracked consolidation
#[derive(Debug, Clone, Serialize)]
pub struct ConsolidationRecord {
    /// Source validator index
    pub source_index: u64,
    /// Target validator index
    pub target_index: u64,
    /// Epoch when first seen
    pub epoch_seen: u64,
    /// Current claim status
    pub status: ClaimStatus,
    /// Transaction hash if submitted
    pub tx_hash: Option<String>,
    /// Error message if failed
    pub error: Option<String>,
}

/// Shared application state
#[derive(Debug, Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

#[derive(Debug)]
struct AppStateInner {
    /// Current synced slot
    current_slot: AtomicU64,
    /// Current epoch
    current_epoch: AtomicU64,
    /// Head slot from beacon node
    head_slot: AtomicU64,
    /// Tracked consolidations by source index
    consolidations: DashMap<u64, ConsolidationRecord>,
    /// Service start time
    start_time: std::time::Instant,
    /// Last error message
    last_error: RwLock<Option<String>>,
}

impl AppState {
    /// Create new application state
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(AppStateInner {
                current_slot: AtomicU64::new(0),
                current_epoch: AtomicU64::new(0),
                head_slot: AtomicU64::new(0),
                consolidations: DashMap::new(),
                start_time: std::time::Instant::now(),
                last_error: RwLock::new(None),
            }),
        }
    }

    /// Get current synced slot
    #[must_use]
    pub fn current_slot(&self) -> u64 {
        self.inner.current_slot.load(Ordering::Relaxed)
    }

    /// Set current synced slot
    pub fn set_current_slot(&self, slot: u64) {
        self.inner.current_slot.store(slot, Ordering::Relaxed);
    }

    /// Get current epoch
    #[must_use]
    pub fn current_epoch(&self) -> u64 {
        self.inner.current_epoch.load(Ordering::Relaxed)
    }

    /// Set current epoch
    pub fn set_current_epoch(&self, epoch: u64) {
        self.inner.current_epoch.store(epoch, Ordering::Relaxed);
    }

    /// Get head slot
    #[must_use]
    pub fn head_slot(&self) -> u64 {
        self.inner.head_slot.load(Ordering::Relaxed)
    }

    /// Set head slot
    pub fn set_head_slot(&self, slot: u64) {
        self.inner.head_slot.store(slot, Ordering::Relaxed);
    }

    /// Get slots behind head
    #[must_use]
    pub fn slots_behind(&self) -> u64 {
        self.head_slot().saturating_sub(self.current_slot())
    }

    /// Check if service is healthy (within 64 slots of head)
    #[must_use]
    pub fn is_healthy(&self) -> bool {
        self.slots_behind() <= 64
    }

    /// Get uptime in seconds
    #[must_use]
    pub fn uptime_secs(&self) -> u64 {
        self.inner.start_time.elapsed().as_secs()
    }

    /// Add or update a consolidation record
    pub fn upsert_consolidation(&self, record: ConsolidationRecord) {
        self.inner
            .consolidations
            .insert(record.source_index, record);
    }

    /// Get consolidation by source index
    #[must_use]
    pub fn get_consolidation(&self, source_index: u64) -> Option<ConsolidationRecord> {
        self.inner.consolidations.get(&source_index).map(|r| r.clone())
    }

    /// Get all consolidations
    #[must_use]
    pub fn all_consolidations(&self) -> Vec<ConsolidationRecord> {
        self.inner
            .consolidations
            .iter()
            .map(|r| r.value().clone())
            .collect()
    }

    /// Get consolidation counts by status
    #[must_use]
    pub fn status_counts(&self) -> StatusCounts {
        let mut counts = StatusCounts::default();
        for entry in self.inner.consolidations.iter() {
            match entry.status {
                ClaimStatus::Detected => counts.detected += 1,
                ClaimStatus::ProofBuilt => counts.proof_built += 1,
                ClaimStatus::Submitted => counts.submitted += 1,
                ClaimStatus::Confirmed => counts.confirmed += 1,
                ClaimStatus::Failed => counts.failed += 1,
            }
        }
        counts
    }

    /// Set last error
    pub fn set_error(&self, error: Option<String>) {
        *self.inner.last_error.write() = error;
    }

    /// Get last error
    #[must_use]
    pub fn last_error(&self) -> Option<String> {
        self.inner.last_error.read().clone()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Counts of consolidations by status
#[derive(Debug, Default, Clone, Serialize)]
pub struct StatusCounts {
    pub detected: usize,
    pub proof_built: usize,
    pub submitted: usize,
    pub confirmed: usize,
    pub failed: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_basic() {
        let state = AppState::new();

        state.set_current_slot(100);
        state.set_head_slot(150);

        assert_eq!(state.current_slot(), 100);
        assert_eq!(state.head_slot(), 150);
        assert_eq!(state.slots_behind(), 50);
        assert!(state.is_healthy());
    }

    #[test]
    fn test_app_state_unhealthy() {
        let state = AppState::new();

        state.set_current_slot(100);
        state.set_head_slot(200);

        assert_eq!(state.slots_behind(), 100);
        assert!(!state.is_healthy());
    }

    #[test]
    fn test_consolidation_tracking() {
        let state = AppState::new();

        let record = ConsolidationRecord {
            source_index: 42,
            target_index: 100,
            epoch_seen: 500,
            status: ClaimStatus::Detected,
            tx_hash: None,
            error: None,
        };

        state.upsert_consolidation(record.clone());

        let retrieved = state.get_consolidation(42).unwrap();
        assert_eq!(retrieved.source_index, 42);
        assert_eq!(retrieved.status, ClaimStatus::Detected);

        let counts = state.status_counts();
        assert_eq!(counts.detected, 1);
    }
}
