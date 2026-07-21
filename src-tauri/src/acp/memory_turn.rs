use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use sha2::{Digest, Sha256};

const ACTIVE_MASK: u64 = 1;
const NONCE_SHIFT: u32 = 1;
const SOURCE_ID_DOMAIN: &[u8] = b"iyw-claw:user-memory-source:v1\0";

/// Per-connection authority for candidate-memory provenance.
///
/// The low bit records whether a turn is active; the remaining bits hold the
/// monotonic nonce. Packing both values into one atomic prevents mismatched
/// nonce/active reads. The commit lease gate makes proposal writes linearize
/// with completion: completion clears active first, then waits for any already
/// authorized synchronous commit to finish before returning.
#[derive(Debug, Default)]
pub struct MemoryTurnTracker {
    state: AtomicU64,
    commit_leases: Mutex<usize>,
    commits_drained: Condvar,
}

impl MemoryTurnTracker {
    pub fn begin_accepted_turn(&self) -> u64 {
        let mut leases = self.lock_commit_leases();
        while *leases != 0 {
            leases = self.wait_for_commit_leases(leases);
        }
        let mut current = self.state.load(Ordering::Acquire);
        loop {
            let nonce = (current >> NONCE_SHIFT)
                .checked_add(1)
                .filter(|nonce| *nonce <= (u64::MAX >> NONCE_SHIFT))
                .expect("memory turn nonce exhausted");
            let next = (nonce << NONCE_SHIFT) | ACTIVE_MASK;
            match self.state.compare_exchange_weak(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return nonce,
                Err(actual) => current = actual,
            }
        }
    }

    pub fn complete_turn(&self) {
        let mut leases = self.lock_commit_leases();
        self.state.fetch_and(!ACTIVE_MASK, Ordering::AcqRel);
        while *leases != 0 {
            leases = self.wait_for_commit_leases(leases);
        }
    }

    pub fn active_nonce(&self) -> Option<u64> {
        let state = self.state.load(Ordering::Acquire);
        (state & ACTIVE_MASK != 0).then_some(state >> NONCE_SHIFT)
    }

    pub(crate) fn acquire_commit_lease(
        self: &Arc<Self>,
        expected_nonce: u64,
    ) -> Option<MemoryTurnCommitLease> {
        let mut leases = self.lock_commit_leases();
        if self.active_nonce() != Some(expected_nonce) {
            return None;
        }
        *leases = leases
            .checked_add(1)
            .expect("memory commit lease exhausted");
        Some(MemoryTurnCommitLease {
            tracker: self.clone(),
        })
    }

    fn release_commit_lease(&self) {
        let mut leases = self.lock_commit_leases();
        *leases = leases
            .checked_sub(1)
            .expect("memory commit lease underflow");
        if *leases == 0 {
            self.commits_drained.notify_all();
        }
    }

    fn lock_commit_leases(&self) -> MutexGuard<'_, usize> {
        self.commit_leases
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn wait_for_commit_leases<'a>(&self, leases: MutexGuard<'a, usize>) -> MutexGuard<'a, usize> {
        self.commits_drained
            .wait(leases)
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[derive(Debug)]
pub(crate) struct MemoryTurnCommitLease {
    tracker: Arc<MemoryTurnTracker>,
}

impl Drop for MemoryTurnCommitLease {
    fn drop(&mut self) {
        self.tracker.release_commit_lease();
    }
}

/// Derive a stable, non-secret provenance id without persisting the launch
/// token or exposing the raw connection id.
pub fn derive_opaque_source_id(launch_token: &str, connection_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(SOURCE_ID_DOMAIN);
    hash_len_prefixed(&mut hasher, launch_token.as_bytes());
    hash_len_prefixed(&mut hasher, connection_id.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn hash_len_prefixed(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value);
}
