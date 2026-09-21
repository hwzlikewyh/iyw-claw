use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::LazyLock;

use sea_orm::DatabaseConnection;
use tokio::sync::Semaphore;

use super::managed_model_archive::{extract_archive, stage_legacy};
use super::managed_model_state::{
    clear_retry, commit_stage, create_stage, current_directory, installed_quick, installed_version,
    managed_root, record_failure,
};
use super::managed_model_types::{
    model_error, LEGACY_DIRECTORY, LEGACY_VERSION, MODEL_COMPONENT_ID, MODEL_ID,
};
use crate::acp::version_center::{
    download_resumable, verify_tool_file_signature, AgentPlatformClient, MemoryModelArtifact,
    MemoryModelOffer, MemoryModelQuery,
};
use crate::app_error::{AppCommandError, AppErrorCode};

const MAX_MODEL_ARCHIVE_BYTES: i64 = 512 * 1024 * 1024;

static INSTALL_LOCK: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(1));
static INSTALL_ACTIVE: AtomicBool = AtomicBool::new(false);

struct InstallActivity;

impl InstallActivity {
    fn begin() -> Self {
        INSTALL_ACTIVE.store(true, Ordering::Release);
        Self
    }
}

impl Drop for InstallActivity {
    fn drop(&mut self) {
        INSTALL_ACTIVE.store(false, Ordering::Release);
    }
}

impl super::UserMemoryService {
    pub(crate) async fn prepare_managed_model(
        &self,
        data_dir: &Path,
        channel: &str,
    ) -> Result<String, AppCommandError> {
        let version = prepare(&self.db, data_dir, self.resolved_root().ok(), channel).await?;
        self.schedule_semantic_refresh();
        Ok(version)
    }

    pub(crate) fn start_managed_model_retry(&self, data_dir: PathBuf) {
        super::managed_model_retry::start_retry_worker(self.clone(), data_dir);
    }
}

pub(super) fn installed(data_dir: &Path) -> bool {
    installed_quick(data_dir)
}

pub(super) fn downloading() -> bool {
    INSTALL_ACTIVE.load(Ordering::Acquire)
}

pub(super) fn model_directory(data_dir: &Path) -> Result<PathBuf, AppCommandError> {
    current_directory(data_dir)
}

pub(super) async fn prepare(
    conn: &DatabaseConnection,
    data_dir: &Path,
    legacy_root: Option<&Path>,
    channel: &str,
) -> Result<String, AppCommandError> {
    if cfg!(feature = "tauri-runtime") {
        return installed_version(data_dir).ok_or_else(|| AppCommandError::dependency_missing(
            "BGE 模型未安装或校验失败，请运行独立环境修复程序",
        ));
    }
    let _permit = INSTALL_LOCK
        .acquire()
        .await
        .map_err(|error| model_error(format!("Managed model lock failed: {error}")))?;
    let _activity = InstallActivity::begin();
    let mut desired_version = String::new();
    let result = prepare_locked(conn, data_dir, legacy_root, channel, &mut desired_version).await;
    match &result {
        Ok(version) => {
            clear_retry(conn).await?;
            tracing::info!(version, "[memory-model] managed model is ready");
        }
        Err(error) => {
            let state = record_failure(conn, &desired_version, error).await?;
            tracing::warn!(
                error_code = ?error.code,
                attempt = state.attempt_count,
                next_attempt_at = %state.next_attempt_at,
                "[memory-model] preparation deferred for background retry"
            );
            super::managed_model_retry::notify_retry();
        }
    }
    result
}

async fn prepare_locked(
    conn: &DatabaseConnection,
    data_dir: &Path,
    legacy_root: Option<&Path>,
    channel: &str,
    desired_version: &mut String,
) -> Result<String, AppCommandError> {
    if let Some(version) = installed_version(data_dir) {
        return Ok(version);
    }
    if let Some(version) = migrate_legacy(data_dir, legacy_root)? {
        return Ok(version);
    }
    let query = MemoryModelQuery {
        model_id: MODEL_ID,
        installed_version: "",
        channel,
    };
    let offer = AgentPlatformClient::resolve_memory_model(conn, query).await?;
    validate_offer(&offer)?;
    desired_version.clone_from(&offer.version);
    if offer.action == "keep" {
        current_directory(data_dir)?;
        return Ok(offer.version);
    }
    install_offer(conn, data_dir, query, &offer).await
}

fn migrate_legacy(
    data_dir: &Path,
    legacy_root: Option<&Path>,
) -> Result<Option<String>, AppCommandError> {
    let Some(source) = legacy_root.map(|root| root.join(LEGACY_DIRECTORY)) else {
        return Ok(None);
    };
    if !source.is_dir() {
        return Ok(None);
    }
    let stage = create_stage(data_dir)?;
    if let Err(error) = stage_legacy(&source, &stage) {
        let _ = std::fs::remove_dir_all(&stage);
        tracing::info!(code = ?error.code, "[memory-model] legacy model was not eligible for migration");
        return Ok(None);
    }
    commit_stage(data_dir, &stage, LEGACY_VERSION)?;
    tracing::info!("[memory-model] verified legacy model migrated into managed storage");
    Ok(Some(LEGACY_VERSION.to_string()))
}

async fn install_offer(
    conn: &DatabaseConnection,
    data_dir: &Path,
    query: MemoryModelQuery<'_>,
    offer: &MemoryModelOffer,
) -> Result<String, AppCommandError> {
    let archive = archive_path(data_dir, &offer.artifact)?;
    let artifact = download_artifact(conn, query, &offer.artifact, &archive).await?;
    let stage = create_stage(data_dir)?;
    let result = (|| {
        verify_tool_file_signature(&archive, &artifact.signature)?;
        extract_archive(&archive, &stage)?;
        commit_stage(data_dir, &stage, &offer.version)?;
        Ok(offer.version.clone())
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&stage);
        let _ = std::fs::remove_file(&archive);
    } else {
        let _ = std::fs::remove_file(&archive);
    }
    result
}

async fn download_artifact(
    conn: &DatabaseConnection,
    query: MemoryModelQuery<'_>,
    artifact: &MemoryModelArtifact,
    archive: &Path,
) -> Result<MemoryModelArtifact, AppCommandError> {
    match download_once(artifact, archive).await {
        Ok(()) => Ok(artifact.clone()),
        Err(error) if error.code == AppErrorCode::AuthenticationFailed => {
            let refreshed =
                AgentPlatformClient::refresh_memory_model_download(conn, query, artifact).await?;
            validate_refreshed_artifact(artifact, &refreshed)?;
            download_once(&refreshed, archive).await?;
            Ok(refreshed)
        }
        Err(error) => Err(error),
    }
}

async fn download_once(
    artifact: &MemoryModelArtifact,
    archive: &Path,
) -> Result<(), AppCommandError> {
    validate_artifact(artifact, true)?;
    download_resumable(
        &artifact.artifact_id,
        &artifact.url,
        archive,
        artifact.size,
        &artifact.sha256,
        None,
    )
    .await
}

fn validate_offer(offer: &MemoryModelOffer) -> Result<(), AppCommandError> {
    let valid_action = matches!(offer.action.as_str(), "install" | "update" | "keep");
    if offer.component_id != MODEL_COMPONENT_ID
        || offer.model_id != MODEL_ID
        || offer.display_name.trim().is_empty()
        || semver::Version::parse(&offer.version).is_err()
        || !valid_action
    {
        return Err(model_error(
            "Fusion returned an invalid managed model offer",
        ));
    }
    validate_artifact(&offer.artifact, offer.action != "keep")
}

fn validate_artifact(
    artifact: &MemoryModelArtifact,
    require_url: bool,
) -> Result<(), AppCommandError> {
    let ids_valid = artifact
        .version_id
        .parse::<i64>()
        .is_ok_and(|value| value > 0)
        && artifact
            .artifact_id
            .parse::<i64>()
            .is_ok_and(|value| value > 0);
    let digest_valid = artifact.sha256.len() == 64
        && artifact
            .sha256
            .chars()
            .all(|value| value.is_ascii_hexdigit());
    let url_valid = !require_url || valid_download_url(&artifact.url);
    if !ids_valid
        || artifact.package_kind != "zip"
        || !artifact.file_name.ends_with(".zip")
        || artifact.size <= 0
        || artifact.size > MAX_MODEL_ARCHIVE_BYTES
        || !digest_valid
        || !url_valid
    {
        return Err(model_error(
            "Fusion returned invalid managed model artifact metadata",
        ));
    }
    Ok(())
}

fn validate_refreshed_artifact(
    expected: &MemoryModelArtifact,
    refreshed: &MemoryModelArtifact,
) -> Result<(), AppCommandError> {
    validate_artifact(refreshed, true)?;
    let matches = expected.version_id == refreshed.version_id
        && expected.artifact_id == refreshed.artifact_id
        && expected.size == refreshed.size
        && expected.sha256.eq_ignore_ascii_case(&refreshed.sha256)
        && expected.signature == refreshed.signature;
    matches
        .then_some(())
        .ok_or_else(|| model_error("Refreshed managed model ticket changed artifact identity"))
}

fn valid_download_url(value: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(value) else {
        return false;
    };
    let secure = url.scheme() == "https"
        || ((cfg!(debug_assertions) || cfg!(feature = "test-gateway")) && url.scheme() == "http");
    secure && url.host_str().is_some() && url.username().is_empty() && url.password().is_none()
}

fn archive_path(
    data_dir: &Path,
    artifact: &MemoryModelArtifact,
) -> Result<PathBuf, AppCommandError> {
    validate_artifact(artifact, false)?;
    let downloads = managed_root(data_dir).join(".downloads");
    super::helpers::reject_symlink(&downloads)?;
    std::fs::create_dir_all(&downloads).map_err(AppCommandError::io)?;
    super::helpers::reject_symlink(&downloads)?;
    Ok(downloads.join(format!("{}.zip", artifact.artifact_id)))
}
