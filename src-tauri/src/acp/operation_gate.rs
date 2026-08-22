use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};

use crate::acp::error::AcpError;

#[derive(Default)]
pub(crate) struct AgentOperationGate {
    lifecycle: Arc<RwLock<()>>,
    relaunch_claimed: Arc<AtomicBool>,
}

impl AgentOperationGate {
    pub(crate) async fn acquire_read(&self) -> Result<OwnedRwLockReadGuard<()>, AcpError> {
        if self.relaunch_claimed.load(Ordering::Acquire) {
            return Err(relaunching_error());
        }
        let guard = self.lifecycle.clone().read_owned().await;
        if self.relaunch_claimed.load(Ordering::Acquire) {
            return Err(relaunching_error());
        }
        Ok(guard)
    }

    pub(crate) fn try_claim_relaunch(&self) -> Result<RelaunchClaim, AcpError> {
        if self.relaunch_claimed.load(Ordering::Acquire) {
            return Err(AcpError::protocol(
                "Application restart is already scheduled",
            ));
        }
        let guard = self
            .lifecycle
            .clone()
            .try_write_owned()
            .map_err(|_| active_operation_error())?;
        self.relaunch_claimed.store(true, Ordering::Release);
        Ok(RelaunchClaim {
            _guard: guard,
            claimed: self.relaunch_claimed.clone(),
        })
    }
}

pub(crate) struct RelaunchClaim {
    _guard: OwnedRwLockWriteGuard<()>,
    claimed: Arc<AtomicBool>,
}

impl Drop for RelaunchClaim {
    fn drop(&mut self) {
        self.claimed.store(false, Ordering::Release);
    }
}

fn relaunching_error() -> AcpError {
    AcpError::protocol("ACP manager is preparing to restart; retry after relaunch")
}

fn active_operation_error() -> AcpError {
    AcpError::protocol("Active Agent operations must finish before restarting")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn active_reader_blocks_relaunch_claim() {
        let gate = AgentOperationGate::default();
        let _reader = gate.acquire_read().await.expect("reader");
        assert!(gate.try_claim_relaunch().is_err());
    }

    #[tokio::test]
    async fn dropped_claim_allows_operations_again() {
        let gate = AgentOperationGate::default();
        drop(gate.try_claim_relaunch().expect("claim"));
        assert!(gate.acquire_read().await.is_ok());
    }

    #[tokio::test]
    async fn committed_claim_rejects_new_operations() {
        let gate = AgentOperationGate::default();
        let _claim = gate.try_claim_relaunch().expect("claim");
        assert!(gate.acquire_read().await.is_err());
    }
}
