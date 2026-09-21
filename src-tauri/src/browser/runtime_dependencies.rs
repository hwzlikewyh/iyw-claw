use tokio_util::sync::CancellationToken;

use super::super::engine::detect_engine;
use super::super::error::{BrowserError, BrowserErrorCode};
use super::super::sidecar;
use super::{BrowserCapability, BrowserRuntime, RuntimeLaunchDependencies, VerifiedDependencies};

impl BrowserRuntime {
    pub async fn prepare_for_start(
        &self,
        _cancellation: CancellationToken,
    ) -> Result<BrowserCapability, BrowserError> {
        Ok(self.resolve_dependencies().await?.capability())
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
    ) -> Result<RuntimeLaunchDependencies, BrowserError> {
        if cancellation.is_cancelled() {
            return Err(BrowserError::shutting_down());
        }
        let verified = self.resolve_dependencies().await?;
        let extension_dir = crate::commands::internet_tools::internet_tools_prepare_extension()
            .await
            .map(std::path::PathBuf::from)
            .map_err(|_| extension_unavailable())?;
        (extension_dir.join("manifest.json").is_file())
            .then_some(RuntimeLaunchDependencies {
                verified,
                extension_dir,
            })
            .ok_or_else(extension_unavailable)
    }
}

fn extension_unavailable() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserRuntimeUnavailable,
        "The bundled OpenCLI browser extension could not be prepared",
    )
    .retryable(true)
}
