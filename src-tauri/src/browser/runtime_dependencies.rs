use tokio_util::sync::CancellationToken;

use super::super::engine::detect_engine;
#[cfg(target_os = "windows")]
use super::super::engine_download;
use super::super::error::BrowserError;
#[cfg(target_os = "windows")]
use super::super::error::BrowserErrorCode;
use super::super::sidecar;
use super::{BrowserCapability, BrowserRuntime, VerifiedDependencies};

impl BrowserRuntime {
    pub async fn prepare_for_start(
        &self,
        cancellation: CancellationToken,
    ) -> Result<BrowserCapability, BrowserError> {
        Ok(self.prepare_dependencies(cancellation).await?.capability())
    }

    pub(super) async fn dependencies(&self) -> Result<VerifiedDependencies, BrowserError> {
        self.resolve_dependencies().await
    }

    pub(super) async fn resolve_dependencies(&self) -> Result<VerifiedDependencies, BrowserError> {
        if let Some(dependencies) = self.verified.lock().await.clone() {
            return Ok(dependencies);
        }
        let sidecar = sidecar::verify_sidecar().await?;
        let engine = detect_engine(&self.data_root).await?;
        let dependencies = VerifiedDependencies { sidecar, engine };
        *self.verified.lock().await = Some(dependencies.clone());
        Ok(dependencies)
    }

    pub(super) async fn prepare_dependencies(
        &self,
        cancellation: CancellationToken,
    ) -> Result<VerifiedDependencies, BrowserError> {
        match self.resolve_dependencies().await {
            Ok(dependencies) => Ok(dependencies),
            #[cfg(target_os = "windows")]
            Err(error) if error.code == BrowserErrorCode::BrowserEngineNotFound => {
                let sidecar = sidecar::verify_sidecar().await?;
                let engine =
                    engine_download::ensure_managed_engine(&self.data_root, cancellation).await?;
                let dependencies = VerifiedDependencies { sidecar, engine };
                *self.verified.lock().await = Some(dependencies.clone());
                Ok(dependencies)
            }
            Err(error) => Err(error),
        }
    }
}
