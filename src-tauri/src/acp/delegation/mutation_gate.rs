use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::{OwnedRwLockReadGuard, RwLock};
use tokio_util::sync::CancellationToken;

/// Linearizes irreversible work against launch-token revocation.
#[derive(Debug)]
pub struct MutationGate {
    open: AtomicBool,
    generation: AtomicU64,
    barrier: Arc<RwLock<()>>,
}

impl Default for MutationGate {
    fn default() -> Self {
        Self {
            open: AtomicBool::new(true),
            generation: AtomicU64::new(0),
            barrier: Arc::new(RwLock::new(())),
        }
    }
}

impl MutationGate {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub async fn acquire(&self, cancellation: &CancellationToken) -> Option<MutationLease> {
        let generation = self.generation.load(Ordering::Acquire);
        if cancellation.is_cancelled() || !self.open.load(Ordering::Acquire) {
            return None;
        }
        let guard = Arc::clone(&self.barrier).read_owned().await;
        if cancellation.is_cancelled()
            || !self.open.load(Ordering::Acquire)
            || self.generation.load(Ordering::Acquire) != generation
        {
            return None;
        }
        Some(MutationLease {
            _generation: generation,
            _guard: guard,
        })
    }

    /// Reject new mutations and wait until every mutation already admitted exits.
    pub async fn close(&self) {
        self.open.store(false, Ordering::Release);
        self.generation.fetch_add(1, Ordering::AcqRel);
        let _barrier = self.barrier.write().await;
    }
}

pub struct MutationLease {
    _generation: u64,
    _guard: OwnedRwLockReadGuard<()>,
}
