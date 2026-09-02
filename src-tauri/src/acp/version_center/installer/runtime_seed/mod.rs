use std::path::Path;

use sea_orm::DatabaseConnection;

use super::runtime_seed_manifest::RuntimeSeedManifest;
use super::state::acquire_writer_lock;
use crate::acp::version_center::capability;
use crate::app_error::AppCommandError;
use crate::web::event_bridge::EventEmitter;

mod codex;
mod tools;

const SEED_DIR: &str = "runtime-seed";
pub(super) const SEED_ARTIFACT_PREFIX: &str = "bundled-runtime-seed";
pub(super) const SEED_POLICY: &str = "recommended";

pub(crate) struct RuntimeSeedImport<'a> {
    pub conn: &'a DatabaseConnection,
    pub data_dir: &'a Path,
    pub resource_dir: &'a Path,
    pub task_id: &'a str,
    pub emitter: &'a EventEmitter,
}

pub(crate) async fn import_runtime_seed(
    request: RuntimeSeedImport<'_>,
) -> Result<(), AppCommandError> {
    if capability::current_target() == "windows" && capability::current_arch() == "x86" {
        tracing::info!("[runtime-seed] Windows x86 keeps the online Version Center path");
        return Ok(());
    }
    let seed_root = request.resource_dir.join(SEED_DIR);
    if !seed_root.join("manifest.json").is_file() {
        tracing::info!("[runtime-seed] bundled seed is unavailable; using Version Center");
        return Ok(());
    }
    let manifest = RuntimeSeedManifest::read(&seed_root)?;
    tools::import(&request, &seed_root, &manifest).await;
    codex::import(&request, &seed_root, &manifest).await;
    Ok(())
}

pub(crate) async fn import_runtime_seed_exclusive(
    request: RuntimeSeedImport<'_>,
) -> Result<(), AppCommandError> {
    let Some(_guard) = acquire_writer_lock(request.data_dir).await? else {
        tracing::info!("[runtime-seed] another initializer owns the writer lock; skipping seed");
        return Ok(());
    };
    import_runtime_seed(request).await
}

pub(super) fn version_at_least(active: Option<&str>, seed: &str) -> bool {
    let (Some(active), Ok(seed)) = (active, semver::Version::parse(seed)) else {
        return false;
    };
    semver::Version::parse(active).is_ok_and(|active| active >= seed)
}

pub(super) fn acp_error(error: crate::acp::error::AcpError) -> AppCommandError {
    AppCommandError::task_execution_failed(error.to_string())
}

pub(super) fn log_component_error(component: &str, phase: &str, error: &AppCommandError) {
    tracing::warn!(
        component,
        phase,
        error_code = ?error.code,
        error_message = %error.message,
        error_detail = error.detail.as_deref().unwrap_or(""),
        "[runtime-seed] bundled component rejected; continuing with Version Center"
    );
}
