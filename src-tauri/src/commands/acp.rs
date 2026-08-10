use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
#[cfg(feature = "tauri-runtime")]
use tauri::{Manager, State};

use crate::acp::agent_storage::AgentStoragePaths;
use crate::acp::binary_cache;
use crate::acp::error::AcpError;
use crate::acp::manager::ConnectionManager;
use crate::acp::npm_runtime;
use crate::acp::opencode_plugins::{self, PluginCheckSummary};
use crate::acp::preflight::{self, PreflightResult};
use crate::acp::provider_overlay::{
    model_gateway_base_url_for, patch_codex_toml, MANAGED_DEFAULT_MODEL,
};
use crate::acp::registry;
use crate::acp::types::{
    AcpAgentInfo, AgentSkillContent, AgentSkillFile, AgentSkillItem, AgentSkillLayout,
    AgentSkillLocation, AgentSkillScope, AgentSkillSyncMode, AgentSkillsListResult,
    ConfigStaleKind, ConnectionStatus,
};
#[cfg(feature = "tauri-runtime")]
use crate::acp::types::{ConnectionInfo, ForkResultInfo, PromptInputBlock};
use crate::acp::version_center::resolve_npm_agent_install;
use crate::commands::experts::{
    central_experts_dir, classify_link, create_link_raw, is_bundled_expert_id, ExpertLinkState,
};
use crate::db::service::agent_setting_service;
use crate::db::AppDatabase;
use crate::models::agent::AgentType;
use crate::web::event_bridge::EventEmitter;

const ACP_AGENTS_UPDATED_EVENT: &str = "app://acp-agents-updated";
const CODEX_MODEL_CATALOG_FILE: &str = "iyw-claw-models.json";
const CODEX_MODEL_CONTEXT_WINDOW: u64 = 128_000;

pub(crate) const MANAGED_AGENT_VERSION_ENV: &str = "IYW_CLAW_MANAGED_AGENT_VERSION";

#[derive(Serialize, Clone)]
#[serde(rename_all = "snake_case")]
struct AcpAgentsUpdatedEventPayload {
    reason: &'static str,
    agent_type: Option<AgentType>,
}

fn emit_acp_agents_updated(
    emitter: &EventEmitter,
    reason: &'static str,
    agent_type: Option<AgentType>,
) {
    crate::web::event_bridge::emit_event(
        emitter,
        ACP_AGENTS_UPDATED_EVENT,
        AcpAgentsUpdatedEventPayload { reason, agent_type },
    );
}

const AGENT_INSTALL_EVENT: &str = "app://agent-install";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AgentInstallEventKind {
    Started,
    Log,
    Progress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AgentInstallEvent {
    pub task_id: String,
    pub kind: AgentInstallEventKind,
    pub payload: String,
}

pub(crate) fn emit_managed_tool_progress(emitter: &EventEmitter, task_id: &str, percent: u8) {
    emit_agent_install_event(
        emitter,
        task_id,
        AgentInstallEventKind::Progress,
        percent.to_string(),
    );
}

fn emit_agent_install_event(
    emitter: &EventEmitter,
    task_id: &str,
    kind: AgentInstallEventKind,
    payload: impl Into<String>,
) {
    crate::web::event_bridge::emit_event(
        emitter,
        AGENT_INSTALL_EVENT,
        AgentInstallEvent {
            task_id: task_id.to_string(),
            kind,
            payload: payload.into(),
        },
    );
}

fn is_version_like(value: &str) -> bool {
    value.chars().any(|c| c.is_ascii_digit()) && value.contains('.')
}

fn normalize_version_candidate(value: &str) -> Option<String> {
    let normalized = value.trim().trim_start_matches('v');
    if is_version_like(normalized) {
        Some(normalized.to_string())
    } else {
        None
    }
}

fn version_from_package_spec(package: &str) -> Option<String> {
    let (_, maybe_version) = package.rsplit_once('@')?;
    let version = maybe_version.trim();
    if version.is_empty() || version.eq_ignore_ascii_case("latest") {
        return None;
    }
    normalize_version_candidate(version)
}

fn package_name_from_spec(package: &str) -> String {
    let normalized = package.trim();
    if normalized.is_empty() {
        return String::new();
    }

    if let Some(index) = normalized.rfind('@') {
        if index > 0 {
            let version_part = normalized[index + 1..].trim();
            if !version_part.is_empty() {
                return normalized[..index].to_string();
            }
        }
    }

    normalized.to_string()
}

/// Validate and normalize a user-supplied custom version for install.
///
/// Stricter than [`normalize_version_candidate`]: tolerates a leading `v`/`V`,
/// then requires the first character to be a digit and the rest to be drawn from
/// `[0-9A-Za-z.-+]` (covers semver pre-release/build metadata and calendar
/// versions like `2026.5.20`). This rejects npm dist-tags (`latest`, `next`) and
/// anything containing whitespace, `@`, or path separators, so the result is
/// safe to interpolate into an npm package spec (`name@<v>`) and to substitute
/// into a binary download URL. Returns the version without the leading `v`.
fn sanitize_custom_version(input: &str) -> Option<String> {
    let trimmed = input.trim();
    let normalized = trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed);
    let mut chars = normalized.chars();
    if !chars.next()?.is_ascii_digit() {
        return None;
    }
    // Require a dotted version (e.g. `1.2.3`) so the validator agrees with the
    // detection fallback `version_from_package_spec`, which needs a `.` — and so
    // a "custom version" is a concrete version rather than an npm range (`2`).
    if !normalized.contains('.') {
        return None;
    }
    let all_allowed = normalized
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'));
    all_allowed.then(|| normalized.to_string())
}

/// Build the versioned npm package spec for an agent.
///
/// `version_override` of `None` or all-whitespace yields the registry-pinned
/// `package` spec unchanged (current behavior). A non-empty override is
/// validated via [`sanitize_custom_version`] and combined with the registry
/// package *name* (its pinned version is dropped) to form `name@<version>`. An
/// override that fails validation is rejected with an error.
fn build_npm_install_spec(
    package: &str,
    version_override: Option<&str>,
) -> Result<String, AcpError> {
    match version_override {
        Some(raw) if !raw.trim().is_empty() => {
            let version = sanitize_custom_version(raw).ok_or_else(|| {
                AcpError::protocol(format!("invalid custom version: {}", raw.trim()))
            })?;
            Ok(format!("{}@{version}", package_name_from_spec(package)))
        }
        _ => Ok(package.to_string()),
    }
}

/// Substitute a custom version into a registry binary download URL by replacing
/// every occurrence of the registry version string. The registry version is
/// embedded in the GitHub release URL (the path tag, and for some agents the
/// asset filename), so a plain replace yields the URL for the requested version
/// — assuming the upstream release reuses the same asset-naming convention.
fn apply_custom_version_to_url(url: &str, registry_version: &str, custom_version: &str) -> String {
    url.replace(registry_version, custom_version)
}

/// Check whether an NPX agent command is spawnable.
/// Uses PATH first, then falls back to the current npm global prefix to handle
/// GUI environments that don't inherit the user's shell PATH.
pub(crate) fn is_cmd_available(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
    version: &str,
    cmd: &str,
) -> bool {
    resolve_npx_command(paths, agent_type, version, cmd).is_some()
        && (agent_type != AgentType::Pi
            || resolve_npx_command(paths, agent_type, version, "pi").is_some())
}

pub(crate) fn resolve_command_on_path(cmd: &str) -> Option<PathBuf> {
    which::which(cmd).ok()
}

fn active_agent_storage_paths() -> Result<AgentStoragePaths, AcpError> {
    AgentStoragePaths::active().ok_or_else(|| {
        AcpError::SdkNotInstalled(
            "Agent storage is not initialized. Choose a private storage directory in Agent Settings."
                .to_string(),
        )
    })
}

pub(crate) fn require_private_agent_storage_for_write() -> Result<AgentStoragePaths, AcpError> {
    let paths = active_agent_storage_paths()?;
    if !crate::acp::agent_storage::startup_profile_env_is_complete(&paths, |key| {
        std::env::var_os(key)
    }) {
        return Err(AcpError::SdkNotInstalled(
            "Agent profile environment is not active. Restart iyw-claw before changing Agent configuration."
                .to_string(),
        ));
    }
    Ok(paths)
}

/// Resolve the `uvx` (uv tool runner) executable used to launch Python ACP
/// agents (e.g. Hermes). Once private Agent storage is active, only the managed
/// uvx under that root is eligible. Legacy PATH/common-location discovery is
/// retained only before storage initialization.
pub(crate) fn resolve_uvx_command() -> Option<PathBuf> {
    if let Some(paths) = AgentStoragePaths::active() {
        return crate::acp::binary_cache::find_cached_uv_tool(&paths, "uvx");
    }
    if let Some(path) = resolve_command_on_path("uvx") {
        return Some(path);
    }
    let exe = if cfg!(windows) { "uvx.exe" } else { "uvx" };
    let home = home_dir_or_default();
    for dir in [
        home.join(".local").join("bin"),
        home.join(".cargo").join("bin"),
    ] {
        let cand = dir.join(exe);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

/// Whether a `Uvx` agent can actually be launched on this machine right now:
/// after private storage activation the managed `uvx` must exist; before
/// initialization the legacy system command remains visible to preflight only.
/// The connect gate (`verify_agent_installed`) and the Settings status/list
/// paths all use this so they agree on readiness. Note: the prepared-version
/// marker is deliberately NOT consulted here — it records what was fetched (for
/// the installed-version badge), not whether the launcher is currently present.
fn uvx_agent_launchable(system_cmd: Option<(&'static str, &'static [&'static str])>) -> bool {
    if AgentStoragePaths::active().is_some() {
        return resolve_uvx_command().is_some();
    }
    resolve_uvx_command().is_some()
        || system_cmd
            .map(|(c, _)| resolve_command_on_path(c).is_some())
            .unwrap_or(false)
}

/// The `uvx` flags that pin the interpreter for a `Uvx` agent, inserted before
/// `--from`. Returns `["--python", <ver>]` when the distribution sets a
/// `python` pin, else an empty vec. Centralizes the pin so every uvx invocation
/// (launch, prewarm, setup/model guidance) stays consistent.
pub(crate) fn uvx_python_args(python: Option<&str>) -> Vec<String> {
    match python {
        Some(ver) => vec!["--python".to_string(), ver.to_string()],
        None => Vec::new(),
    }
}

/// Pre-fetch a `Uvx` agent's pinned package into uvx's cache by running
/// `uvx --from <package> <cmd> --version`, so the first real connect doesn't
/// pay the download cost. Streams progress to the install event stream.
async fn prewarm_uvx_agent(
    agent_name: &str,
    package: &str,
    cmd: &str,
    python: Option<&str>,
    task_id: &str,
    emitter: &EventEmitter,
) -> Result<(), AcpError> {
    // uv must already be installed; provision it separately via the "Install
    // uv" preflight action. We deliberately do NOT auto-install it here so the
    // two steps stay separate — the Settings UI disables this agent-install
    // action until uv is ready, so a normal user never reaches this error.
    let paths = active_agent_storage_paths()?;
    let uvx = crate::acp::binary_cache::find_cached_uv_tool(&paths, "uvx").ok_or_else(|| {
        AcpError::SdkNotInstalled("uv is not installed; install the uv runtime first".to_string())
    })?;
    let python_args = uvx_python_args(python);
    let python_display = if python_args.is_empty() {
        String::new()
    } else {
        format!("{} ", python_args.join(" "))
    };
    emit_agent_install_event(
        emitter,
        task_id,
        AgentInstallEventKind::Log,
        format!("$ uvx {python_display}--from {package} {cmd} --version"),
    );
    let mut command = crate::process::tokio_command(&uvx);
    command.envs(binary_cache::uv_runtime_env(&paths));
    let output = command
        .args(&python_args)
        .arg("--from")
        .arg(package)
        .arg(cmd)
        .arg("--version")
        .output()
        .await
        .map_err(|e| AcpError::SpawnFailed(format!("failed to run uvx: {e}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    for line in stderr.lines().chain(stdout.lines()) {
        if !line.trim().is_empty() {
            emit_agent_install_event(
                emitter,
                task_id,
                AgentInstallEventKind::Log,
                line.to_string(),
            );
        }
    }
    if !output.status.success() {
        return Err(AcpError::protocol(format!(
            "uvx prepare for {agent_name} failed: {}",
            stderr.lines().last().unwrap_or("unknown error")
        )));
    }
    Ok(())
}

pub(crate) fn resolve_npx_command(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
    version: &str,
    cmd: &str,
) -> Option<PathBuf> {
    npm_runtime::resolve_private_npm_command(paths, agent_type, version, cmd)
}

/// Verify that the agent SDK / binary is installed and usable.
///
/// This is the pre-spawn guard used by the session-page connect path:
/// the session page must NEVER trigger a download or install, so if the
/// agent isn't ready we return `AcpError::SdkNotInstalled` immediately
/// and let the frontend prompt the user to install from Agent Settings.
///
/// For NPX agents: checks the command is spawnable in this process environment.
/// For Binary agents: checks platform support and that the binary is
/// already cached locally.
pub(crate) fn verify_agent_installed(
    agent_type: AgentType,
    runtime_env: &BTreeMap<String, String>,
) -> Result<(), AcpError> {
    let meta = registry::get_agent_meta(agent_type);
    match meta.distribution {
        registry::AgentDistribution::Npx { cmd, .. } => {
            let paths = active_agent_storage_paths()?;
            let version = runtime_env
                .get(MANAGED_AGENT_VERSION_ENV)
                .map(String::as_str)
                .filter(|value| !value.trim().is_empty());
            if !version.is_some_and(|version| is_cmd_available(&paths, agent_type, version, cmd)) {
                // INVARIANT: the substring "is not installed" is matched
                // verbatim by the frontend catch block in
                // `src/contexts/acp-connections-context.tsx` to surface a
                // localized install prompt. Do not change the wording.
                return Err(AcpError::SdkNotInstalled(format!(
                    "{} is not installed. Please install it in Agent Settings.",
                    meta.name
                )));
            }
            Ok(())
        }
        registry::AgentDistribution::Binary { cmd, platforms, .. } => {
            let paths = active_agent_storage_paths()?;
            let platform = registry::current_platform();
            if !platforms.iter().any(|p| p.platform == platform) {
                return Err(AcpError::PlatformNotSupported(format!(
                    "{} is not available on {platform}",
                    meta.name
                )));
            }
            // Accept any cached version — the Settings page will still
            // surface "upgrade available" for stale caches via its own
            // version-badge flow.
            if binary_cache::find_best_cached_binary_for_agent(&paths, agent_type, cmd)?.is_none() {
                // INVARIANT: see note above — "is not installed" is a
                // stable substring the frontend matches against.
                return Err(AcpError::SdkNotInstalled(format!(
                    "{} is not installed. Please install it in Agent Settings.",
                    meta.name
                )));
            }
            Ok(())
        }
        registry::AgentDistribution::Uvx { system_cmd, .. } => {
            // Launchable when uvx is resolvable (iyw-claw auto-provisions it on
            // install, so this holds post-prepare) or the agent's own CLI is on
            // PATH. Kept consistent with the Settings status/list paths via the
            // shared helper, so connect and the UI never disagree on readiness.
            if uvx_agent_launchable(system_cmd) {
                Ok(())
            } else {
                Err(AcpError::SdkNotInstalled(format!(
                    "{} is not installed. Please install it in Agent Settings.",
                    meta.name
                )))
            }
        }
    }
}

fn detect_local_version(agent_type: AgentType, recorded_version: Option<&str>) -> Option<String> {
    let meta = registry::get_agent_meta(agent_type);
    match meta.distribution {
        registry::AgentDistribution::Npx { cmd, .. } => {
            let paths = AgentStoragePaths::active()?;
            let version = recorded_version?.trim();
            is_cmd_available(&paths, agent_type, version, cmd).then_some(())?;
            Some(version.to_string())
        }
        registry::AgentDistribution::Binary { cmd, .. } => {
            let paths = AgentStoragePaths::active()?;
            binary_cache::detect_installed_version(&paths, agent_type, cmd)
                .ok()
                .flatten()
        }
        registry::AgentDistribution::Uvx { .. } => {
            let paths = AgentStoragePaths::active()?;
            binary_cache::uvx_prepared_version(&paths, agent_type)
        }
    }
}

fn private_npm_version_from_stdout(stdout: &[u8], package_name: &str) -> Option<String> {
    let document = serde_json::from_slice::<serde_json::Value>(stdout).ok()?;
    document
        .get("dependencies")?
        .get(package_name)?
        .get("version")?
        .as_str()
        .and_then(normalize_version_candidate)
}

async fn verify_private_npm_package_version(
    prefix: &Path,
    package_name: &str,
    expected_version: &str,
) -> Result<(), AcpError> {
    let output = crate::process::tokio_command("npm")
        .arg("list")
        .arg("--global")
        .arg("--prefix")
        .arg(prefix)
        .arg(package_name)
        .arg("--json")
        .arg("--depth=0")
        .output()
        .await
        .map_err(|e| AcpError::protocol(format!("verify private npm package failed: {e}")))?;
    let actual = private_npm_version_from_stdout(&output.stdout, package_name);
    if output.status.success() && actual.as_deref() == Some(expected_version) {
        return Ok(());
    }
    Err(AcpError::protocol(format!(
        "private npm version mismatch for {package_name}: expected {expected_version}, found {}",
        actual.as_deref().unwrap_or("missing")
    )))
}

/// Run an npm command with piped stdout/stderr, streaming each line as a log event.
/// Returns (success: bool, collected_stderr: String) so callers can inspect errors.
async fn run_npm_streaming(
    args: &[OsString],
    task_id: &str,
    emitter: &EventEmitter,
) -> Result<(bool, String), AcpError> {
    use tokio::io::{AsyncBufReadExt, BufReader};

    let mut cmd = crate::process::tokio_command("npm");
    for arg in args {
        cmd.arg(arg);
    }
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| AcpError::protocol(format!("failed to spawn npm: {e}")))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let emitter_clone = emitter.clone();
    let task_id_owned = task_id.to_string();

    let stdout_handle = tokio::spawn({
        let emitter = emitter_clone.clone();
        let task_id = task_id_owned.clone();
        async move {
            if let Some(out) = stdout {
                let reader = BufReader::new(out);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    emit_agent_install_event(&emitter, &task_id, AgentInstallEventKind::Log, &line);
                }
            }
        }
    });

    let stderr_handle = tokio::spawn({
        let emitter = emitter_clone;
        let task_id = task_id_owned;
        async move {
            let mut collected = String::new();
            if let Some(err) = stderr {
                let reader = BufReader::new(err);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    emit_agent_install_event(&emitter, &task_id, AgentInstallEventKind::Log, &line);
                    if !collected.is_empty() {
                        collected.push('\n');
                    }
                    collected.push_str(&line);
                }
            }
            collected
        }
    });

    let (_, stderr_result) = tokio::join!(stdout_handle, stderr_handle);
    let collected_stderr = stderr_result.unwrap_or_default();

    let status = child
        .wait()
        .await
        .map_err(|e| AcpError::protocol(format!("failed to wait for npm process: {e}")))?;

    Ok((status.success(), collected_stderr))
}

const MAX_MANAGED_NPM_METADATA_BYTES: u64 = 2 * 1024 * 1024;
const MAX_MANAGED_NPM_TARBALL_BYTES: u64 = 512 * 1024 * 1024;

enum ManagedNpmSourceError {
    Unavailable(AcpError),
    Rejected(AcpError),
}

#[derive(Deserialize)]
struct ManagedNpmMetadata {
    name: String,
    version: String,
    dist: ManagedNpmDist,
}

#[derive(Deserialize)]
struct ManagedNpmDist {
    tarball: String,
    integrity: String,
}

struct ManagedNpmTarball {
    _temp_dir: tempfile::TempDir,
    path: PathBuf,
}

enum ManagedNpmHasher {
    Sha256(sha2::Sha256),
    Sha384(sha2::Sha384),
    Sha512(sha2::Sha512),
}

impl ManagedNpmHasher {
    fn from_integrity(integrity: &str) -> Result<(Self, Vec<u8>), ManagedNpmSourceError> {
        use base64::Engine as _;

        let (algorithm, encoded) = integrity
            .split_once('-')
            .ok_or_else(|| managed_npm_rejected("managed npm integrity contract is invalid"))?;
        let expected = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| managed_npm_rejected("managed npm integrity digest is invalid"))?;
        let hasher = match algorithm {
            "sha256" => Self::Sha256(sha2::Sha256::default()),
            "sha384" => Self::Sha384(sha2::Sha384::default()),
            "sha512" => Self::Sha512(sha2::Sha512::default()),
            _ => return Err(managed_npm_rejected("managed npm integrity is unsupported")),
        };
        Ok((hasher, expected))
    }

    fn update(&mut self, bytes: &[u8]) {
        use sha2::Digest as _;

        match self {
            Self::Sha256(hasher) => hasher.update(bytes),
            Self::Sha384(hasher) => hasher.update(bytes),
            Self::Sha512(hasher) => hasher.update(bytes),
        }
    }

    fn finish(self) -> Vec<u8> {
        use sha2::Digest as _;

        match self {
            Self::Sha256(hasher) => hasher.finalize().to_vec(),
            Self::Sha384(hasher) => hasher.finalize().to_vec(),
            Self::Sha512(hasher) => hasher.finalize().to_vec(),
        }
    }
}

fn managed_npm_rejected(message: impl Into<String>) -> ManagedNpmSourceError {
    ManagedNpmSourceError::Rejected(AcpError::DownloadFailed(message.into()))
}

fn managed_npm_network_unavailable(error: &reqwest::Error) -> bool {
    if error.is_connect() || error.is_timeout() {
        return true;
    }
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        if let Some(error) = cause.downcast_ref::<io::Error>() {
            return matches!(
                error.kind(),
                io::ErrorKind::ConnectionRefused
                    | io::ErrorKind::ConnectionReset
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::NotConnected
                    | io::ErrorKind::BrokenPipe
                    | io::ErrorKind::TimedOut
            );
        }
        source = cause.source();
    }
    false
}

fn managed_npm_reqwest_error(context: &str, error: reqwest::Error) -> ManagedNpmSourceError {
    let message = format!("{context}: {error}");
    if managed_npm_network_unavailable(&error) {
        ManagedNpmSourceError::Unavailable(AcpError::DownloadFailed(message))
    } else {
        managed_npm_rejected(message)
    }
}

fn managed_npm_http_error(context: &str, status: reqwest::StatusCode) -> ManagedNpmSourceError {
    let error = AcpError::DownloadFailed(format!("{context}: HTTP {status}"));
    if matches!(
        status,
        reqwest::StatusCode::NOT_FOUND
            | reqwest::StatusCode::GONE
            | reqwest::StatusCode::REQUEST_TIMEOUT
            | reqwest::StatusCode::TOO_MANY_REQUESTS
    ) || status.is_server_error()
    {
        ManagedNpmSourceError::Unavailable(error)
    } else {
        ManagedNpmSourceError::Rejected(error)
    }
}

fn managed_npm_metadata_url(
    registry_url: &str,
    package_name: &str,
    version: &str,
) -> Result<reqwest::Url, ManagedNpmSourceError> {
    let mut url = reqwest::Url::parse(registry_url)
        .map_err(|error| managed_npm_rejected(format!("invalid managed npm registry: {error}")))?;
    let mut segments = url
        .path_segments_mut()
        .map_err(|_| managed_npm_rejected("managed npm registry cannot be a base URL"))?;
    segments.pop_if_empty().push(package_name).push(version);
    drop(segments);
    Ok(url)
}

fn validate_managed_tarball_url(
    registry_url: &str,
    tarball_url: &str,
) -> Result<reqwest::Url, ManagedNpmSourceError> {
    let registry = reqwest::Url::parse(registry_url)
        .map_err(|error| managed_npm_rejected(format!("invalid managed npm registry: {error}")))?;
    let tarball = reqwest::Url::parse(tarball_url).map_err(|error| {
        managed_npm_rejected(format!("invalid managed npm tarball URL: {error}"))
    })?;
    let same_origin = registry.scheme() == tarball.scheme()
        && registry.host_str() == tarball.host_str()
        && registry.port_or_known_default() == tarball.port_or_known_default();
    let safe = same_origin
        && tarball.username().is_empty()
        && tarball.password().is_none()
        && tarball.query().is_none()
        && tarball.fragment().is_none();
    if !safe {
        return Err(managed_npm_rejected(
            "managed npm registry returned an unsafe tarball URL",
        ));
    }
    Ok(tarball)
}

fn managed_npm_client() -> Result<reqwest::Client, ManagedNpmSourceError> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(15 * 60))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| managed_npm_rejected(format!("initialize npm client failed: {error}")))
}

async fn fetch_managed_npm_metadata(
    client: &reqwest::Client,
    package_name: &str,
    version: &str,
    registry_url: &str,
    expected_integrity: &str,
) -> Result<ManagedNpmDist, ManagedNpmSourceError> {
    let metadata_url = managed_npm_metadata_url(registry_url, package_name, version)?;
    let response = client
        .get(metadata_url)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|error| managed_npm_reqwest_error("managed npm metadata request failed", error))?;
    if !response.status().is_success() {
        return Err(managed_npm_http_error(
            "managed npm metadata request failed",
            response.status(),
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_MANAGED_NPM_METADATA_BYTES)
    {
        return Err(managed_npm_rejected("managed npm metadata is too large"));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| managed_npm_reqwest_error("read managed npm metadata failed", error))?;
    if bytes.len() as u64 > MAX_MANAGED_NPM_METADATA_BYTES {
        return Err(managed_npm_rejected("managed npm metadata is too large"));
    }
    let metadata: ManagedNpmMetadata = serde_json::from_slice(&bytes)
        .map_err(|error| managed_npm_rejected(format!("invalid managed npm metadata: {error}")))?;
    if metadata.name != package_name || metadata.version != version {
        return Err(managed_npm_rejected(
            "managed npm metadata version mismatch",
        ));
    }
    if metadata.dist.integrity.trim() != expected_integrity {
        return Err(managed_npm_rejected(
            "managed npm metadata integrity does not match the version center",
        ));
    }
    Ok(metadata.dist)
}

async fn request_managed_npm_tarball(
    client: &reqwest::Client,
    registry_url: &str,
    tarball_url: &str,
) -> Result<reqwest::Response, ManagedNpmSourceError> {
    let tarball_url = validate_managed_tarball_url(registry_url, tarball_url)?;
    let response =
        client.get(tarball_url).send().await.map_err(|error| {
            managed_npm_reqwest_error("managed npm tarball request failed", error)
        })?;
    if !response.status().is_success() {
        return Err(managed_npm_http_error(
            "managed npm tarball request failed",
            response.status(),
        ));
    }
    Ok(response)
}

async fn write_managed_npm_tarball(
    mut file: tokio::fs::File,
    response: reqwest::Response,
    expected_integrity: &str,
) -> Result<(), ManagedNpmSourceError> {
    use futures_util::StreamExt as _;
    use tokio::io::AsyncWriteExt as _;

    let content_length = response.content_length();
    if content_length.is_some_and(|length| length > MAX_MANAGED_NPM_TARBALL_BYTES) {
        return Err(managed_npm_rejected("managed npm tarball is too large"));
    }
    let (mut hasher, expected_digest) = ManagedNpmHasher::from_integrity(expected_integrity)?;
    let mut stream = response.bytes_stream();
    let mut downloaded = 0_u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk
            .map_err(|error| managed_npm_reqwest_error("download npm tarball failed", error))?;
        downloaded = downloaded
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| managed_npm_rejected("managed npm tarball size overflow"))?;
        if downloaded > MAX_MANAGED_NPM_TARBALL_BYTES {
            return Err(managed_npm_rejected("managed npm tarball is too large"));
        }
        file.write_all(&chunk)
            .await
            .map_err(|error| managed_npm_rejected(format!("write npm tarball failed: {error}")))?;
        hasher.update(&chunk);
    }
    file.flush()
        .await
        .map_err(|error| managed_npm_rejected(format!("persist npm tarball failed: {error}")))?;
    file.sync_all()
        .await
        .map_err(|error| managed_npm_rejected(format!("persist npm tarball failed: {error}")))?;
    if content_length.is_some_and(|length| length != downloaded) {
        return Err(managed_npm_rejected("managed npm tarball length mismatch"));
    }
    if hasher.finish() != expected_digest {
        return Err(managed_npm_rejected(
            "managed npm tarball integrity does not match the version center",
        ));
    }
    Ok(())
}

async fn persist_managed_npm_tarball(
    paths: &AgentStoragePaths,
    response: reqwest::Response,
    expected_integrity: &str,
) -> Result<ManagedNpmTarball, ManagedNpmSourceError> {
    tokio::fs::create_dir_all(paths.staging_dir())
        .await
        .map_err(|error| {
            managed_npm_rejected(format!("create npm staging root failed: {error}"))
        })?;
    let temp_dir = tempfile::Builder::new()
        .prefix("managed-npm-")
        .tempdir_in(paths.staging_dir())
        .map_err(|error| {
            managed_npm_rejected(format!("create npm tarball staging failed: {error}"))
        })?;
    let path = temp_dir.path().join("package.tgz");
    let file = tokio::fs::File::create(&path)
        .await
        .map_err(|error| managed_npm_rejected(format!("create npm tarball failed: {error}")))?;
    write_managed_npm_tarball(file, response, expected_integrity).await?;
    Ok(ManagedNpmTarball {
        _temp_dir: temp_dir,
        path,
    })
}

async fn fetch_managed_npm_tarball(
    paths: &AgentStoragePaths,
    package_name: &str,
    version: &str,
    registry_url: &str,
    expected_integrity: &str,
) -> Result<ManagedNpmTarball, ManagedNpmSourceError> {
    let client = managed_npm_client()?;
    let dist = fetch_managed_npm_metadata(
        &client,
        package_name,
        version,
        registry_url,
        expected_integrity,
    )
    .await?;
    let response = request_managed_npm_tarball(&client, registry_url, &dist.tarball).await?;
    persist_managed_npm_tarball(paths, response, expected_integrity).await
}

fn managed_npm_install_error(error: AcpError) -> ManagedNpmSourceError {
    let explicitly_unavailable = match &error {
        AcpError::Protocol(message) if message.contains("private npm install failed:") => {
            let message = message
                .strip_prefix("private npm install failed:")
                .unwrap_or(message)
                .to_ascii_lowercase();
            message.lines().any(|line| {
                let line = line.trim();
                let code = line
                    .strip_prefix("npm error code ")
                    .or_else(|| line.strip_prefix("npm err! code "));
                code.is_some_and(|code| {
                    matches!(
                        code.split_whitespace().next(),
                        Some(
                            "enetunreach"
                                | "ehostunreach"
                                | "econnrefused"
                                | "econnreset"
                                | "etimedout"
                                | "eai_again"
                                | "enotfound"
                                | "e408"
                                | "e429"
                                | "e500"
                                | "e502"
                                | "e503"
                                | "e504"
                        )
                    )
                })
            })
        }
        AcpError::DownloadFailed(message) => {
            message.starts_with("npm skipped the platform binary '")
        }
        _ => false,
    };
    if explicitly_unavailable {
        ManagedNpmSourceError::Unavailable(error)
    } else {
        ManagedNpmSourceError::Rejected(error)
    }
}

async fn install_managed_npm_package(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
    package_name: &str,
    version: &str,
    registry_url: &str,
    integrity: &str,
    required_commands: &[&str],
    task_id: &str,
    emitter: &EventEmitter,
) -> Result<PathBuf, ManagedNpmSourceError> {
    emit_agent_install_event(
        emitter,
        task_id,
        AgentInstallEventKind::Log,
        format!("Downloading exact managed npm archive for {package_name}@{version}"),
    );
    let tarball =
        fetch_managed_npm_tarball(paths, package_name, version, registry_url, integrity).await?;
    emit_agent_install_event(
        emitter,
        task_id,
        AgentInstallEventKind::Log,
        "Managed npm archive integrity verified",
    );
    let package_path = tarball
        .path
        .to_str()
        .ok_or_else(|| managed_npm_rejected("managed npm tarball path is not valid Unicode"))?;
    install_private_npm_package(
        paths,
        agent_type,
        version,
        &[package_path],
        Some(package_name),
        Some(registry_url),
        required_commands,
        task_id,
        emitter,
    )
    .await
    .map_err(managed_npm_install_error)
}

/// How many times a managed npm install is attempted before giving up. See the
/// retry comment in [`install_private_npm_package`].
const NPM_INSTALL_ATTEMPTS: u32 = 3;

/// One `npm install` attempt plus the checks that decide whether it produced a
/// *usable* tree.
///
/// npm exits 0 when an optional dependency fails, so exit status alone is not
/// evidence of success: the version check catches a wrong/missing top-level
/// package, and the platform audit catches a per-platform binary sub-package
/// that npm skipped after a failed download. Without the audit the install is
/// recorded as successful and the failure only surfaces when the agent is
/// launched (`Missing optional dependency @openai/codex-win32-x64`, the agent
/// process exiting 1 during ACP `initialize`).
async fn run_private_npm_install_attempt(
    staging: &Path,
    args: &[OsString],
    package_name: &str,
    version: &str,
    task_id: &str,
    emitter: &EventEmitter,
) -> Result<(), AcpError> {
    let (success, stderr) = run_npm_streaming(args, task_id, emitter).await?;
    if !success {
        let detail = stderr.trim();
        return Err(AcpError::protocol(if detail.is_empty() {
            "private npm install failed".to_string()
        } else {
            format!("private npm install failed: {detail}")
        }));
    }
    verify_private_npm_package_version(staging, package_name, version).await?;
    npm_runtime::verify_host_platform_optional_deps(staging)
}

async fn install_private_npm_package(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
    version: &str,
    packages: &[&str],
    expected_package_name: Option<&str>,
    registry_url: Option<&str>,
    required_commands: &[&str],
    task_id: &str,
    emitter: &EventEmitter,
) -> Result<PathBuf, AcpError> {
    // Fast path: if the installer bundled a pre-built npm prefix for this
    // agent+version, seed it directly without running npm. This avoids a
    // multi-hundred-MB network download on the first launch of a freshly
    // installed desktop app. Non-fatal: if the seed fails we fall through to
    // the regular npm install.
    if expected_package_name.is_none()
        && agent_type == AgentType::Codex
        && version == binary_cache::BUNDLED_CODEX_ACP_VERSION
    {
        if let Ok(exe) = std::env::current_exe() {
            match binary_cache::seed_bundled_codex_acp(paths, &exe) {
                Ok(true) => {
                    emit_agent_install_event(
                        emitter,
                        task_id,
                        AgentInstallEventKind::Log,
                        "Installed from bundled package (no network download)",
                    );
                    let prefix = npm_runtime::private_npm_prefix(paths, agent_type, version)?;
                    return Ok(prefix);
                }
                Ok(false) => {
                    // Already installed or no bundle — fall through to npm.
                }
                Err(error) => {
                    emit_agent_install_event(
                        emitter,
                        task_id,
                        AgentInstallEventKind::Log,
                        format!("Bundled install failed: {error}; falling back to npm"),
                    );
                }
            }
        }
    }

    let staging = npm_runtime::private_npm_staging_prefix(paths, agent_type);
    tokio::fs::create_dir_all(paths.staging_dir())
        .await
        .map_err(|e| AcpError::protocol(format!("create npm staging root failed: {e}")))?;
    tokio::fs::create_dir_all(paths.npm_cache_dir())
        .await
        .map_err(|e| AcpError::protocol(format!("create private npm cache failed: {e}")))?;
    let args = npm_runtime::private_npm_install_args_with_registry(
        &staging,
        &paths.npm_cache_dir(),
        packages,
        registry_url,
    )?;
    let package_name = expected_package_name
        .map(str::to_string)
        .or_else(|| {
            packages
                .first()
                .map(|package| package_name_from_spec(package))
        })
        .filter(|name| !name.is_empty())
        .ok_or_else(|| AcpError::protocol("private npm package name is empty"))?;
    let package_display = packages.join(" ");

    emit_agent_install_event(
        emitter,
        task_id,
        AgentInstallEventKind::Log,
        format!(
            "$ npm install --global --include=optional --prefix={} {package_display}",
            staging.display()
        ),
    );

    let result = async {
        // Agent packages carry per-platform binary sub-packages that unpack to
        // hundreds of MB (`@openai/codex` is ~390 MB), and a dropped connection
        // mid-download is common enough on mainland-China networks that a single
        // attempt is not enough. Retries are cheap: the shared npm cache means a
        // retry only refetches what actually failed.
        let mut attempt = 0;
        loop {
            attempt += 1;
            let outcome = run_private_npm_install_attempt(
                &staging,
                &args,
                &package_name,
                version,
                task_id,
                emitter,
            )
            .await;
            match outcome {
                Ok(()) => break,
                Err(error) if attempt < NPM_INSTALL_ATTEMPTS => {
                    emit_agent_install_event(
                        emitter,
                        task_id,
                        AgentInstallEventKind::Log,
                        format!(
                            "Install attempt {attempt}/{NPM_INSTALL_ATTEMPTS} failed: {error}; retrying"
                        ),
                    );
                    // A partially-populated prefix would make the next attempt's
                    // audit pass on stale files, so start each attempt clean.
                    let _ = tokio::fs::remove_dir_all(&staging).await;
                }
                Err(error) => return Err(error),
            }
        }
        npm_runtime::activate_private_npm_runtime(
            paths,
            agent_type,
            version,
            &staging,
            required_commands,
        )
    }
    .await;

    let _ = tokio::fs::remove_dir_all(&staging).await;
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkillStorageKind {
    SkillDirectoryOnly,
    SkillDirectoryOrMarkdownFile,
}

#[derive(Debug, Clone)]
pub(crate) struct SkillStorageSpec {
    pub kind: SkillStorageKind,
    pub global_dirs: Vec<PathBuf>,
    pub project_rel_dirs: Vec<&'static str>,
}

fn home_dir_or_default() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

fn user_shared_agent_skills_dir_for(
    private_storage_active: bool,
    home: PathBuf,
) -> Option<PathBuf> {
    (!private_storage_active).then(|| home.join(".agents").join("skills"))
}

fn with_user_shared_agent_skills(mut directories: Vec<PathBuf>) -> Vec<PathBuf> {
    let active = crate::acp::agent_storage::AgentStoragePaths::active().is_some();
    if let Some(shared) = user_shared_agent_skills_dir_for(active, home_dir_or_default()) {
        directories.push(shared);
    }
    directories
}

pub(crate) fn codex_home_dir() -> PathBuf {
    let configured = std::env::var("CODEX_HOME").ok().and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });

    match configured {
        Some(value) => {
            if value == "~" {
                home_dir_or_default()
            } else if let Some(remain) = value.strip_prefix("~/") {
                home_dir_or_default().join(remain)
            } else {
                PathBuf::from(value)
            }
        }
        None => home_dir_or_default().join(".codex"),
    }
}

/// Hermes config/data directory. Honors `HERMES_HOME`, defaults to `~/.hermes`.
/// Hermes self-manages credentials (`.env`), config (`config.yaml`), session
/// store (`state.db`), and skills (`skills/`) here.
pub(crate) fn hermes_home_dir() -> PathBuf {
    let configured = std::env::var("HERMES_HOME").ok().and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });

    match configured {
        Some(value) => {
            if value == "~" {
                home_dir_or_default()
            } else if let Some(remain) = value.strip_prefix("~/") {
                home_dir_or_default().join(remain)
            } else {
                PathBuf::from(value)
            }
        }
        None => home_dir_or_default().join(".hermes"),
    }
}

fn codex_config_toml_path() -> PathBuf {
    codex_home_dir().join("config.toml")
}

fn codex_auth_json_path() -> PathBuf {
    codex_home_dir().join("auth.json")
}

fn codex_model_catalog_path() -> PathBuf {
    codex_home_dir().join(CODEX_MODEL_CATALOG_FILE)
}

/// OpenCode reads config from `$XDG_CONFIG_HOME/opencode` (falling back to
/// `~/.config/opencode`) and credentials from `$XDG_DATA_HOME/opencode`
/// (falling back to `~/.local/share/opencode`) on every platform. iyw-claw must
/// write where OpenCode reads, so these reuse the same XDG resolution as
/// `opencode_plugins` (config) and `parsers::opencode` (data) — otherwise a
/// user with XDG dirs set would get credentials written where OpenCode never
/// looks, and iyw-claw's own plugin/connect paths would diverge.
fn opencode_config_dir() -> PathBuf {
    crate::parsers::profile_paths::opencode_config_dir()
}

fn opencode_primary_config_path() -> PathBuf {
    opencode_config_dir().join("opencode.json")
}

fn opencode_legacy_config_path() -> PathBuf {
    opencode_config_dir().join("config.json")
}

fn resolve_opencode_config_path() -> PathBuf {
    let primary = opencode_primary_config_path();
    if primary.exists() {
        return primary;
    }

    let legacy = opencode_legacy_config_path();
    if legacy.exists() {
        return legacy;
    }

    primary
}

fn opencode_auth_json_path() -> PathBuf {
    crate::parsers::opencode::resolve_opencode_base_dir().join("auth.json")
}

fn load_opencode_auth_json_raw() -> Option<String> {
    fs::read_to_string(opencode_auth_json_path()).ok()
}

// ---------------------------------------------------------------------------
// Cline config helpers
// ---------------------------------------------------------------------------

fn cline_data_dir() -> PathBuf {
    crate::parsers::profile_paths::cline_data_dir()
}

fn cline_global_state_path() -> PathBuf {
    cline_data_dir().join("globalState.json")
}

fn cline_secrets_path() -> PathBuf {
    cline_data_dir().join("secrets.json")
}

fn load_cline_secrets_json_raw() -> Option<String> {
    fs::read_to_string(cline_secrets_path()).ok()
}

/// Cline provider → secrets.json field name for the API key.
fn cline_api_key_field_for_provider(provider: &str) -> &'static str {
    match provider {
        "anthropic" => "apiKey",
        "openrouter" => "openRouterApiKey",
        "openai-native" => "openAiNativeApiKey",
        "openai" => "openAiApiKey",
        "gemini" => "geminiApiKey",
        "deepseek" => "deepSeekApiKey",
        "mistral" => "mistralApiKey",
        "xai" => "xaiApiKey",
        _ => "openAiApiKey",
    }
}

/// Cline provider → globalState model ID key suffix.
/// Providers in ProviderKeyMap use `actMode{Suffix}` / `planMode{Suffix}`,
/// others use `actModeApiModelId` / `planModeApiModelId`.
fn cline_model_id_keys_for_provider(provider: &str) -> (&'static str, &'static str) {
    match provider {
        "openrouter" | "cline" => ("actModeOpenRouterModelId", "planModeOpenRouterModelId"),
        "openai" => ("actModeOpenAiModelId", "planModeOpenAiModelId"),
        "ollama" => ("actModeOllamaModelId", "planModeOllamaModelId"),
        "lmstudio" => ("actModeLmStudioModelId", "planModeLmStudioModelId"),
        "litellm" => ("actModeLiteLlmModelId", "planModeLiteLlmModelId"),
        "requesty" => ("actModeRequestyModelId", "planModeRequestyModelId"),
        "groq" => ("actModeGroqModelId", "planModeGroqModelId"),
        _ => ("actModeApiModelId", "planModeApiModelId"),
    }
}

/// Read globalState.json + secrets.json and merge into a unified config JSON
/// with keys: apiProvider, model, apiKey, apiBaseUrl.
fn load_cline_local_config_json() -> Option<String> {
    let mut merged = serde_json::Map::new();

    if let Ok(raw) = fs::read_to_string(cline_global_state_path()) {
        if let Ok(state) = serde_json::from_str::<serde_json::Value>(&raw) {
            // Cline uses actModeApiProvider / planModeApiProvider (prefer actMode)
            let provider = state
                .get("actModeApiProvider")
                .or_else(|| state.get("planModeApiProvider"))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .unwrap_or("anthropic")
                .to_string();

            merged.insert(
                "apiProvider".to_string(),
                serde_json::Value::String(provider.clone()),
            );

            // Read model from provider-specific key
            let (act_key, _plan_key) = cline_model_id_keys_for_provider(&provider);
            if let Some(model_id) = state
                .get(act_key)
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
            {
                merged.insert(
                    "model".to_string(),
                    serde_json::Value::String(model_id.to_string()),
                );
            }

            // Read provider-specific baseUrl key
            let base_url_key = match provider.as_str() {
                "anthropic" => "anthropicBaseUrl",
                "gemini" => "geminiBaseUrl",
                "ollama" => "ollamaBaseUrl",
                "lmstudio" => "lmStudioBaseUrl",
                "litellm" => "liteLlmBaseUrl",
                "requesty" => "requestyBaseUrl",
                _ => "openAiBaseUrl",
            };
            if let Some(base_url) = state
                .get(base_url_key)
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
            {
                merged.insert(
                    "apiBaseUrl".to_string(),
                    serde_json::Value::String(base_url.to_string()),
                );
            }
        }
    }

    // Read API key from secrets.json based on provider
    if let Ok(raw) = fs::read_to_string(cline_secrets_path()) {
        if let Ok(secrets) = serde_json::from_str::<serde_json::Value>(&raw) {
            let provider = merged
                .get("apiProvider")
                .and_then(|v| v.as_str())
                .unwrap_or("anthropic");
            let key_field = cline_api_key_field_for_provider(provider);
            if let Some(api_key) = secrets
                .get(key_field)
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
            {
                merged.insert(
                    "apiKey".to_string(),
                    serde_json::Value::String(api_key.to_string()),
                );
            }
        }
    }

    if merged.is_empty() {
        return None;
    }
    serde_json::to_string_pretty(&serde_json::Value::Object(merged)).ok()
}

/// Split merged config back into globalState.json + secrets.json.
/// Writes `actModeApiProvider`, `planModeApiProvider`, provider-specific model keys,
/// `openAiBaseUrl`, and `welcomeViewCompleted` to globalState.json,
/// and the provider-specific API key to secrets.json.
fn persist_cline_local_config(config_patch_json: Option<&str>) -> Result<(), AcpError> {
    let Some(raw_patch) = config_patch_json else {
        return Ok(());
    };
    let runtime = serde_json::from_str::<AgentRuntimeConfig>(raw_patch)
        .map_err(|e| AcpError::protocol(format!("invalid config_json: {e}")))?;
    let patch = serde_json::from_str::<serde_json::Value>(raw_patch)
        .map_err(|e| AcpError::protocol(format!("invalid config_json: {e}")))?;

    let provider = patch
        .get("apiProvider")
        .and_then(|v| v.as_str())
        .unwrap_or("anthropic")
        .to_string();

    // --- Update globalState.json (merge) ---
    let gs_path = cline_global_state_path();
    let mut gs = if gs_path.exists() {
        match fs::read_to_string(&gs_path)
            .ok()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        {
            Some(existing) if existing.is_object() => existing,
            _ => serde_json::json!({}),
        }
    } else {
        serde_json::json!({})
    };
    let gs_obj = gs
        .as_object_mut()
        .ok_or_else(|| AcpError::protocol("globalState root must be object"))?;

    // Cline checks welcomeViewCompleted first in isAuthConfigured()
    gs_obj.insert(
        "welcomeViewCompleted".to_string(),
        serde_json::Value::Bool(true),
    );

    // Set both act/plan mode providers
    gs_obj.insert(
        "actModeApiProvider".to_string(),
        serde_json::Value::String(provider.clone()),
    );
    gs_obj.insert(
        "planModeApiProvider".to_string(),
        serde_json::Value::String(provider.clone()),
    );

    // Set provider-specific model ID keys
    let (act_model_key, plan_model_key) = cline_model_id_keys_for_provider(&provider);
    match trim_non_empty(runtime.model) {
        Some(model) => {
            gs_obj.insert(
                act_model_key.to_string(),
                serde_json::Value::String(model.clone()),
            );
            gs_obj.insert(plan_model_key.to_string(), serde_json::Value::String(model));
        }
        None => {
            gs_obj.remove(act_model_key);
            gs_obj.remove(plan_model_key);
        }
    }

    // Each provider uses its own baseUrl key in globalState
    let base_url_key = match provider.as_str() {
        "anthropic" => "anthropicBaseUrl",
        "gemini" => "geminiBaseUrl",
        "ollama" => "ollamaBaseUrl",
        "lmstudio" => "lmStudioBaseUrl",
        "litellm" => "liteLlmBaseUrl",
        "requesty" => "requestyBaseUrl",
        _ => "openAiBaseUrl",
    };
    match trim_non_empty(runtime.api_base_url) {
        Some(base_url) => {
            gs_obj.insert(
                base_url_key.to_string(),
                serde_json::Value::String(base_url),
            );
        }
        None => {
            gs_obj.remove(base_url_key);
        }
    }

    if let Some(parent) = gs_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AcpError::protocol(format!("create cline data directory failed: {e}")))?;
    }
    let serialized_gs = serde_json::to_string_pretty(&gs)
        .map_err(|e| AcpError::protocol(format!("serialize cline globalState failed: {e}")))?;
    fs::write(&gs_path, format!("{serialized_gs}\n"))
        .map_err(|e| AcpError::protocol(format!("write cline globalState failed: {e}")))?;

    // --- Update secrets.json ---
    let secrets_path = cline_secrets_path();
    let mut secrets = if secrets_path.exists() {
        match fs::read_to_string(&secrets_path)
            .ok()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        {
            Some(existing) if existing.is_object() => existing,
            _ => serde_json::json!({}),
        }
    } else {
        serde_json::json!({})
    };
    let secrets_obj = secrets
        .as_object_mut()
        .ok_or_else(|| AcpError::protocol("secrets root must be object"))?;

    let key_field = cline_api_key_field_for_provider(&provider);
    match trim_non_empty(runtime.api_key) {
        Some(api_key) => {
            secrets_obj.insert(key_field.to_string(), serde_json::Value::String(api_key));
        }
        None => {
            secrets_obj.remove(key_field);
        }
    }

    if let Some(parent) = secrets_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AcpError::protocol(format!("create cline data directory failed: {e}")))?;
    }
    let serialized_secrets = serde_json::to_string_pretty(&secrets)
        .map_err(|e| AcpError::protocol(format!("serialize cline secrets failed: {e}")))?;
    fs::write(&secrets_path, format!("{serialized_secrets}\n"))
        .map_err(|e| AcpError::protocol(format!("write cline secrets failed: {e}")))?;

    Ok(())
}

fn managed_codex_model_ids() -> Vec<String> {
    crate::acp::model_catalog::model_ids_for(AgentType::Codex)
        .iter()
        .map(|model| (*model).to_string())
        .collect()
}

fn load_codex_model_catalog_ids() -> Vec<String> {
    managed_codex_model_ids()
}

const CODEX_DEFAULT_BASE_INSTRUCTIONS: &str =
    "你是爱原物原助理。你和用户共享一个工作区，请持续协作，直到用户的目标真正完成。";

const CODEX_IMAGE_FALLBACK_INSTRUCTIONS: &str = r###"图片理解规则：
- 当上下文包含图片、图片路径、file URI、HTTPS URL、Data URI 或 Base64，但上游工具或网关没有返回足以回答当前问题的图片描述时，先确认当前可用工具中存在名称后缀为 `analyze_image` 的工具，再调用它。将图片来源传入 `source`，将需要确认的视觉信息写入 `question`。
- 如果上下文已经包含网关或上游返回的图片分析文本，优先使用该文本，不要重复调用；只有现有描述缺少回答所需细节时才追加调用。
- 如果没有可用的 `analyze_image` 工具，不要虚构工具调用或猜测图片内容，应明确说明当前无法分析图片。
- 不要向工具传入模型、provider 或凭据；视觉模型由系统选择。"###;

#[cfg(target_os = "windows")]
const CODEX_WINDOWS_BASE_INSTRUCTIONS: &str = concat!(
    r###"你是爱原物原助理。你和用户共享一个工作区，请持续协作，直到用户的目标真正完成。

Windows shell rules (PowerShell 7):
- 使用固定可执行文件 `C:\Program Files\PowerShell\7\pwsh.exe`，参数使用 `-NoProfile -Command`；不要使用 `cmd.exe` 或 Windows PowerShell 5.1 (`powershell.exe`)。
- 使用 PowerShell 原生命令和语法，不要假设 Bash/POSIX 语法。禁止 Bash 风格的 `\"` 转义。
- 当外层 PowerShell 调用 `-Command` 时，优先用单引号包住整个脚本，使 `$p`、`$i`、`$lines` 等变量只在内层脚本展开；无法使用外层单引号时，用反引号转义 `$`。避免多层引号，复杂命令拆成多个简单命令。
- `rg` 的 pattern 含 `|`、`(`、`)` 或反斜杠时，使用单引号包住 pattern，避免 `|` 被解析为 PowerShell 管道；不要用反斜杠拼接 PowerShell 引号。
- 用户和项目指令优先于这些默认规则，必须遵循更高优先级的约束。

带行号读取模板（外层也是 PowerShell 时，`-Command` 脚本用单引号保护）：
`& 'C:\Program Files\PowerShell\7\pwsh.exe' -NoProfile -Command '$p="models\file.py"; $lines=Get-Content -Path $p; for($i=1;$i -le 80;$i++){ "{0,4}: {1}" -f $i,$lines[$i-1] }'`

rg 模板（普通 pattern）：
`& 'C:\Program Files\PowerShell\7\pwsh.exe' -NoProfile -Command 'rg -n "simple_text" models configs docs'`

rg 模板（pattern 含 `|`、括号或反斜杠；用相邻单引号表示内层单引号）：
`& 'C:\Program Files\PowerShell\7\pwsh.exe' -NoProfile -Command 'rg -n ''audible|Audible|sound_prob|pred_sound_prob'' models configs docs'`
"###
);

#[cfg(not(target_os = "windows"))]
const CODEX_WINDOWS_BASE_INSTRUCTIONS: &str = CODEX_DEFAULT_BASE_INSTRUCTIONS;

// 当前 catalog/session 元数据没有指令代际字段；待共享 contract 提供字段后再接入，暂不自造 schema。
fn base_instructions_for(agent_type: AgentType) -> &'static str {
    if cfg!(target_os = "windows") && agent_type == AgentType::Codex {
        CODEX_WINDOWS_BASE_INSTRUCTIONS
    } else {
        CODEX_DEFAULT_BASE_INSTRUCTIONS
    }
}

fn codex_base_instructions_for_model(model: &str) -> String {
    let base = base_instructions_for(AgentType::Codex);
    let uses_vision_fallback =
        crate::acp::model_catalog::model_capabilities(model).is_some_and(|capability| {
            capability.image_input_mode == crate::acp::model_catalog::ImageInputMode::Fallback
        });
    if !uses_vision_fallback {
        return base.to_string();
    }
    format!("{base}\n\n{CODEX_IMAGE_FALLBACK_INSTRUCTIONS}")
}

fn codex_model_catalog_entry(model: &str, priority: usize) -> serde_json::Value {
    serde_json::json!({
        "slug": model,
        "display_name": model,
        "description": "Custom model managed by iyw-claw.",
        "default_reasoning_level": "high",
        "supported_reasoning_levels": [
            { "effort": "low", "description": "Fast responses with lighter reasoning" },
            { "effort": "medium", "description": "Balances speed and reasoning depth for everyday tasks" },
            { "effort": "high", "description": "Greater reasoning depth for complex problems" },
            { "effort": "xhigh", "description": "Extra high reasoning depth for complex problems" }
        ],
        "shell_type": "shell_command",
        "visibility": "list",
        "supported_in_api": true,
        "priority": priority,
        "base_instructions": codex_base_instructions_for_model(model),
        "include_skills_usage_instructions": true,
        "supports_reasoning_summaries": true,
        "support_verbosity": false,
        "apply_patch_tool_type": "freeform",
        "web_search_tool_type": "text",
        "truncation_policy": { "mode": "tokens", "limit": 10000 },
        "supports_parallel_tool_calls": true,
        // Enables Codex ACP transport; the online model catalog still gates UI exposure.
        "additional_speed_tiers": ["fast"],
        "context_window": CODEX_MODEL_CONTEXT_WINDOW,
        "effective_context_window_percent": 95,
        "experimental_supported_tools": []
    })
}

fn serialize_codex_model_catalog(model_ids: &[String]) -> Result<String, AcpError> {
    let models = model_ids
        .iter()
        .enumerate()
        .map(|(priority, model)| codex_model_catalog_entry(model, priority))
        .collect::<Vec<_>>();
    serde_json::to_string_pretty(&serde_json::json!({ "models": models }))
        .map(|raw| format!("{raw}\n"))
        .map_err(|e| AcpError::protocol(format!("serialize codex model catalog failed: {e}")))
}

fn toml_root_assignment_key(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('[') {
        return None;
    }
    trimmed.split_once('=').map(|(key, _)| key.trim())
}

fn patch_toml_root_string(raw_toml: &str, key: &str, value: &str) -> String {
    let newline = if raw_toml.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut lines = raw_toml
        .split(newline)
        .map(str::to_string)
        .collect::<Vec<_>>();
    let root_end = lines
        .iter()
        .position(|line| {
            let trimmed = line.trim();
            trimmed.starts_with('[') && trimmed.ends_with(']')
        })
        .unwrap_or(lines.len());
    let line = format!(
        "{key} = {}",
        serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
    );
    if let Some(index) = lines[..root_end]
        .iter()
        .position(|current| toml_root_assignment_key(current) == Some(key))
    {
        lines[index] = line;
    } else {
        let mut insert_at = root_end;
        while insert_at > 0 && lines[insert_at - 1].trim().is_empty() {
            insert_at -= 1;
        }
        lines.insert(insert_at, line);
    }
    lines.join(newline)
}

fn codex_model_ids_from_projection(raw: Option<&str>) -> Result<Option<Vec<String>>, AcpError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let value = serde_json::from_str::<serde_json::Value>(raw)
        .map_err(|e| AcpError::protocol(format!("invalid config_json: {e}")))?;
    let Some(models) = value.get("modelCatalog") else {
        return Ok(None);
    };
    serde_json::from_value::<Vec<String>>(models.clone())
        .map_err(|e| AcpError::protocol(format!("invalid codex modelCatalog: {e}")))?;
    Ok(Some(managed_codex_model_ids()))
}

fn load_codex_auth_json_raw() -> Option<String> {
    fs::read_to_string(codex_auth_json_path()).ok()
}

fn load_codex_config_toml_raw() -> Option<String> {
    fs::read_to_string(codex_config_toml_path()).ok()
}

/// Project codex `config.toml` text into the launch-relevant config map shared
/// by the settings read-back and the staleness fingerprint. Pure (no I/O) so it
/// is unit-testable; [`load_codex_local_config_json`] is the on-disk wrapper
/// that also folds in the api key from `auth.json`.
///
/// `apiBaseUrl` / `model` / `env` mirror back into the codex runtime env via
/// [`build_runtime_env_from_setting`] (they map to `OPENAI_*`); `modelProvider`
/// deliberately does NOT (it is not an `AgentRuntimeConfig` field). It is still
/// included so a provider switch is visible to the fingerprint even when the
/// resolved `base_url` is unchanged — two providers can share one endpoint yet
/// differ in `wire_api` / auth. Before codex-acp 1.0.1 this was caught only
/// incidentally by the injected `MODEL_PROVIDER` launch env; that injection is
/// gone now that resume reads `model_provider` from config.toml (#224), so the
/// fingerprint must carry the name itself. `requestHeaderToken` likewise keeps
/// running-session staleness aligned with the provider's static token header.
fn codex_config_projection_from_toml(raw_toml: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut merged = serde_json::Map::new();
    let Ok(value) = raw_toml.parse::<toml::Value>() else {
        return merged;
    };

    if let Some(model) = value
        .get("model")
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        merged.insert(
            "model".to_string(),
            serde_json::Value::String(model.to_string()),
        );
    }

    if let Some(model_catalog_path) = value
        .get("model_catalog_json")
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        merged.insert(
            "modelCatalogPath".to_string(),
            serde_json::Value::String(model_catalog_path.to_string()),
        );
    }

    let model_provider = value
        .get("model_provider")
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string);

    if let Some(provider) = &model_provider {
        merged.insert(
            "modelProvider".to_string(),
            serde_json::Value::String(provider.clone()),
        );
    }

    let mut api_base_url: Option<String> = None;
    if let Some(provider) = &model_provider {
        api_base_url = value
            .get("model_providers")
            .and_then(|table| table.get(provider.as_str()))
            .and_then(|table| table.get("base_url"))
            .and_then(|item| item.as_str())
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string);
    }
    if api_base_url.is_none() {
        api_base_url = value
            .get("model_providers")
            .and_then(|table| table.as_table())
            .and_then(|providers| {
                providers.values().find_map(|item| {
                    item.get("base_url")
                        .and_then(|base| base.as_str())
                        .map(str::trim)
                        .filter(|base| !base.is_empty())
                        .map(str::to_string)
                })
            });
    }
    if let Some(base_url) = api_base_url {
        merged.insert(
            "apiBaseUrl".to_string(),
            serde_json::Value::String(base_url),
        );
    }

    let mut request_header_token: Option<String> = None;
    if let Some(provider) = &model_provider {
        request_header_token = value
            .get("model_providers")
            .and_then(|table| table.get(provider.as_str()))
            .and_then(|table| table.get("http_headers"))
            .and_then(|headers| headers.get("token"))
            .and_then(toml::Value::as_str)
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .map(str::to_string);
    }
    if request_header_token.is_none() {
        request_header_token = value
            .get("model_providers")
            .and_then(toml::Value::as_table)
            .and_then(|providers| {
                providers.values().find_map(|provider| {
                    provider
                        .get("http_headers")
                        .and_then(|headers| headers.get("token"))
                        .and_then(toml::Value::as_str)
                        .map(str::trim)
                        .filter(|token| !token.is_empty())
                        .map(str::to_string)
                })
            });
    }
    if let Some(token) = request_header_token {
        merged.insert(
            "requestHeaderToken".to_string(),
            serde_json::Value::String(token),
        );
    }

    if let Some(env) = value.get("env").and_then(|item| item.as_table()) {
        let mut env_map = serde_json::Map::new();
        for (key, item) in env {
            let Some(raw) = item.as_str() else {
                continue;
            };
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            env_map.insert(
                key.to_string(),
                serde_json::Value::String(trimmed.to_string()),
            );
        }
        if !env_map.is_empty() {
            merged.insert("env".to_string(), serde_json::Value::Object(env_map));
        }
    }

    merged
}

fn load_codex_local_config_json() -> Option<String> {
    let mut merged = match fs::read_to_string(codex_config_toml_path()) {
        Ok(raw_toml) => codex_config_projection_from_toml(&raw_toml),
        Err(_) => serde_json::Map::new(),
    };

    if let Ok(raw_auth) = fs::read_to_string(codex_auth_json_path()) {
        if let Ok(auth) = serde_json::from_str::<serde_json::Value>(&raw_auth) {
            if let Some(api_key) = auth
                .get("OPENAI_API_KEY")
                .and_then(|item| item.as_str())
                .map(str::trim)
                .filter(|item| !item.is_empty())
            {
                merged.insert(
                    "apiKey".to_string(),
                    serde_json::Value::String(api_key.to_string()),
                );
            }
        }
    }

    let model_catalog = load_codex_model_catalog_ids();
    if !model_catalog.is_empty() {
        merged.insert(
            "modelCatalog".to_string(),
            serde_json::Value::Array(
                model_catalog
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
    }

    if merged.is_empty() {
        return None;
    }
    serde_json::to_string_pretty(&serde_json::Value::Object(merged)).ok()
}

fn persist_codex_local_config(config_patch_json: Option<&str>) -> Result<(), AcpError> {
    let Some(raw_patch) = config_patch_json else {
        return Ok(());
    };
    let runtime = serde_json::from_str::<AgentRuntimeConfig>(raw_patch)
        .map_err(|e| AcpError::protocol(format!("invalid config_json: {e}")))?;
    let AgentRuntimeConfig {
        api_base_url,
        api_key,
        model,
        env,
    } = runtime;

    let config_path = codex_config_toml_path();
    let mut toml_value = if config_path.exists() {
        match fs::read_to_string(&config_path)
            .ok()
            .and_then(|raw| raw.parse::<toml::Value>().ok())
        {
            Some(existing) if existing.is_table() => existing,
            _ => toml::Value::Table(toml::map::Map::new()),
        }
    } else {
        toml::Value::Table(toml::map::Map::new())
    };

    let table = toml_value
        .as_table_mut()
        .ok_or_else(|| AcpError::protocol("codex config root must be a TOML table"))?;

    match trim_non_empty(model) {
        Some(model) => {
            table.insert("model".to_string(), toml::Value::String(model));
        }
        None => {
            table.remove("model");
        }
    }

    let provider_name = table
        .get("model_provider")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "iyw-claw".to_string());
    table.insert(
        "model_provider".to_string(),
        toml::Value::String(provider_name.clone()),
    );

    let providers_item = table
        .entry("model_providers".to_string())
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    if !providers_item.is_table() {
        *providers_item = toml::Value::Table(toml::map::Map::new());
    }
    let providers = providers_item
        .as_table_mut()
        .ok_or_else(|| AcpError::protocol("invalid model_providers table"))?;
    let provider_item = providers
        .entry(provider_name.clone())
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    if !provider_item.is_table() {
        *provider_item = toml::Value::Table(toml::map::Map::new());
    }
    let provider_table = provider_item
        .as_table_mut()
        .ok_or_else(|| AcpError::protocol("invalid model provider table"))?;
    match trim_non_empty(api_base_url) {
        Some(base_url) => {
            provider_table.insert("base_url".to_string(), toml::Value::String(base_url));
        }
        None => {
            provider_table.remove("base_url");
        }
    }
    if provider_name == "iyw-claw" {
        provider_table.insert(
            "name".to_string(),
            toml::Value::String("iyw-claw".to_string()),
        );
        provider_table.insert(
            "wire_api".to_string(),
            toml::Value::String("responses".to_string()),
        );
        provider_table.insert(
            "requires_openai_auth".to_string(),
            toml::Value::Boolean(true),
        );
    }

    if env.is_empty() {
        table.remove("env");
    } else {
        let mut env_table = toml::map::Map::new();
        for (key, value) in env {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                continue;
            }
            env_table.insert(key, toml::Value::String(trimmed.to_string()));
        }
        if env_table.is_empty() {
            table.remove("env");
        } else {
            table.insert("env".to_string(), toml::Value::Table(env_table));
        }
    }

    let serialized_toml = toml::to_string_pretty(&toml_value)
        .map_err(|e| AcpError::protocol(format!("serialize codex toml failed: {e}")))?;
    let serialized_toml = patch_codex_toml(
        &serialized_toml,
        &model_gateway_base_url_for(AgentType::Codex),
    )
    .map_err(AcpError::protocol)?;
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            AcpError::protocol(format!("create codex config directory failed: {e}"))
        })?;
    }
    fs::write(&config_path, format!("{serialized_toml}\n"))
        .map_err(|e| AcpError::protocol(format!("write codex config failed: {e}")))?;

    let auth_path = codex_auth_json_path();
    let mut auth_value = if auth_path.exists() {
        match fs::read_to_string(&auth_path)
            .ok()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        {
            Some(existing) if existing.is_object() => existing,
            _ => serde_json::json!({}),
        }
    } else {
        serde_json::json!({})
    };
    let auth_obj = auth_value
        .as_object_mut()
        .ok_or_else(|| AcpError::protocol("codex auth root must be object"))?;
    match trim_non_empty(api_key) {
        Some(api_key) => {
            auth_obj.insert(
                "OPENAI_API_KEY".to_string(),
                serde_json::Value::String(api_key),
            );
        }
        None => {
            auth_obj.remove("OPENAI_API_KEY");
        }
    }
    let serialized_auth = serde_json::to_string_pretty(&auth_value)
        .map_err(|e| AcpError::protocol(format!("serialize codex auth failed: {e}")))?;
    if let Some(parent) = auth_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AcpError::protocol(format!("create codex auth directory failed: {e}")))?;
    }
    fs::write(&auth_path, format!("{serialized_auth}\n"))
        .map_err(|e| AcpError::protocol(format!("write codex auth failed: {e}")))?;

    ensure_codex_model_catalog()?;
    Ok(())
}

fn prepare_codex_config_files(
    raw_toml: Option<&str>,
    model_ids: Option<&[String]>,
) -> Result<(Option<String>, Option<String>), AcpError> {
    if raw_toml.is_none() && model_ids.is_none() {
        return Ok((None, None));
    }
    let mut toml_text = raw_toml
        .map(str::to_string)
        .unwrap_or_else(|| fs::read_to_string(codex_config_toml_path()).unwrap_or_default());
    let table = toml::from_str::<toml::Table>(&toml_text)
        .map_err(|e| AcpError::protocol(format!("invalid codex config.toml: {e}")))?;
    let selected_model = table
        .get("model")
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|model| {
            crate::acp::model_catalog::all_model_ids()
                .iter()
                .any(|id| id == model)
        })
        .unwrap_or(MANAGED_DEFAULT_MODEL)
        .to_string();
    toml_text = patch_toml_root_string(&toml_text, "model", &selected_model);
    if model_ids.is_some() {
        let catalog_path = codex_model_catalog_path();
        toml_text = patch_toml_root_string(
            &toml_text,
            "model_catalog_json",
            &catalog_path.to_string_lossy(),
        );
    }
    toml_text = patch_codex_toml(&toml_text, &model_gateway_base_url_for(AgentType::Codex))
        .map_err(AcpError::protocol)?;
    toml::from_str::<toml::Table>(&toml_text)
        .map_err(|e| AcpError::protocol(format!("invalid codex config.toml: {e}")))?;
    let catalog = model_ids
        .is_some()
        .then(|| serialize_codex_model_catalog(&managed_codex_model_ids()))
        .transpose()?;
    Ok((Some(toml_text), catalog))
}

fn persist_codex_native_config_files(
    codex_auth_json: Option<&str>,
    codex_config_toml: Option<&str>,
    _codex_model_ids: Option<&[String]>,
) -> Result<(), AcpError> {
    let managed_model_ids = managed_codex_model_ids();
    let (prepared_toml, prepared_catalog) =
        prepare_codex_config_files(codex_config_toml, Some(&managed_model_ids))?;
    if let Some(raw_catalog) = prepared_catalog {
        let path = codex_model_catalog_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| AcpError::protocol(format!("create codex directory failed: {e}")))?;
        }
        fs::write(&path, raw_catalog)
            .map_err(|e| AcpError::protocol(format!("write codex model catalog failed: {e}")))?;
    }

    if let Some(raw_toml) = prepared_toml {
        let path = codex_config_toml_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| AcpError::protocol(format!("create codex directory failed: {e}")))?;
        }
        fs::write(&path, raw_toml)
            .map_err(|e| AcpError::protocol(format!("write codex config.toml failed: {e}")))?;
    }

    if let Some(raw_auth) = codex_auth_json {
        let parsed = serde_json::from_str::<serde_json::Value>(raw_auth)
            .map_err(|e| AcpError::protocol(format!("invalid codex auth.json: {e}")))?;
        if !parsed.is_object() {
            return Err(AcpError::protocol(
                "invalid codex auth.json: root must be a JSON object",
            ));
        }
        let path = codex_auth_json_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| AcpError::protocol(format!("create codex directory failed: {e}")))?;
        }
        fs::write(&path, raw_auth)
            .map_err(|e| AcpError::protocol(format!("write codex auth.json failed: {e}")))?;
    }

    Ok(())
}

/// Reapply the managed provider and model catalog before every Codex launch.
fn ensure_codex_model_catalog() -> Result<(), AcpError> {
    let raw_toml = match fs::read_to_string(codex_config_toml_path()) {
        Ok(raw) => raw,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(AcpError::protocol(format!(
                "read codex config.toml failed: {error}"
            )))
        }
    };
    let model_ids = managed_codex_model_ids();
    persist_codex_native_config_files(None, Some(&raw_toml), Some(&model_ids))
}

fn persist_opencode_auth_json(raw_auth: &str) -> Result<(), AcpError> {
    let parsed = serde_json::from_str::<serde_json::Value>(raw_auth)
        .map_err(|e| AcpError::protocol(format!("invalid opencode auth.json: {e}")))?;
    if !parsed.is_object() {
        return Err(AcpError::protocol(
            "invalid opencode auth.json: root must be a JSON object",
        ));
    }
    let path = opencode_auth_json_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AcpError::protocol(format!("create opencode directory failed: {e}")))?;
    }
    fs::write(&path, format!("{raw_auth}\n"))
        .map_err(|e| AcpError::protocol(format!("write opencode auth.json failed: {e}")))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Kimi Code config helpers
//
// IMPORTANT — how `kimi acp` actually authenticates (reverse-engineered &
// empirically verified against @moonshot-ai/kimi-code 0.19.1):
//
// `kimi acp` gates EVERY `session/new` on an OAuth-style token: it calls
// `harnessIsAuthed`, which is true iff `~/.kimi-code/credentials/kimi-code.json`
// holds a token whose `access_token` is non-empty. It NEVER validates that token
// for the gate (no network, no signature check). API keys — whether injected via
// the `KIMI_MODEL_*` env family OR written into `config.toml` `[providers].api_key`
// — do NOT create this token, so on their own they yield `Authentication
// required`. The only advertised ACP auth method is a terminal device-code login
// (`kimi acp --login`), which requires a Kimi *subscription* account.
//
// To support plain API-key users, iyw-claw therefore manages BOTH halves:
//   1. `config.toml` — a iyw-claw-managed `[providers."iyw-claw"]` + `[models."iyw-claw-managed"]`
//      + `default_model` block that ROUTES INFERENCE to the user's API key
//      (any of the six native interface types: kimi / openai / openai_responses /
//      anthropic / google-genai / vertexai).
//   2. `credentials/kimi-code.json` — a synthetic gate token iyw-claw seeds so the
//      ACP session opens. It is purely local: because `default_model` points at
//      the API-key provider, the managed/OAuth endpoint is never called and this
//      token is never transmitted. It carries a `_iyw_claw_synthetic` marker so we
//      only ever remove OUR token, never a real login the user performed.
//
// The iyw-claw-managed block is keyed by the fixed names `iyw-claw` / `iyw-claw-managed`
// so it is recognizable and removable without disturbing any provider/model the
// user added by hand. The raw config.toml editor is the comment/format escape
// hatch. A stale `KIMI_MODEL_*` env override would silently win over config.toml,
// so every save also clears it.
// ---------------------------------------------------------------------------

const KIMI_MANAGED_PROVIDER: &str = "iyw-claw";
const KIMI_MANAGED_MODEL_ALIAS: &str = "iyw-claw-managed";
const KIMI_MODEL_API_KEY_ENV: &str = "KIMI_MODEL_API_KEY";
const KIMI_MODEL_BASE_URL_ENV: &str = "KIMI_MODEL_BASE_URL";
const KIMI_MODEL_NAME_ENV: &str = "KIMI_MODEL_NAME";
/// Sentinel `access_token` value (and `_iyw_claw_synthetic` marker) identifying the
/// gate token iyw-claw seeds, so we never clobber a real OAuth login.
const KIMI_SYNTHETIC_TOKEN_ACCESS: &str = "iyw-claw-local-gate";
/// Fallback context window for the managed model. Kimi's config schema **requires**
/// `[models.<alias>].max_context_size` to be a positive integer — omitting it makes
/// kimi discard the whole model block ("Ignored invalid config … models.iyw-claw-managed"),
/// which leaves `default_model` dangling and every prompt ends with no reply. So we
/// always write one, defaulting to the kimi-k2 256K window when the user leaves it blank.
const KIMI_DEFAULT_MAX_CONTEXT_SIZE: i64 = 262_144;
/// The six native provider `type` values Kimi accepts in `[providers.<name>]`.
const KIMI_INTERFACE_TYPES: &[&str] = &[
    "kimi",
    "openai",
    "openai_responses",
    "anthropic",
    "google-genai",
    "vertexai",
];

fn kimi_code_config_toml_path() -> PathBuf {
    crate::parsers::kimi_code::resolve_kimi_code_home_dir().join("config.toml")
}

/// The synthetic-gate-token file `kimi acp` checks to decide a session is
/// authenticated (`<KIMI_CODE_HOME>/credentials/kimi-code.json`).
fn kimi_code_credentials_token_path() -> PathBuf {
    crate::parsers::kimi_code::resolve_kimi_code_home_dir()
        .join("credentials")
        .join("kimi-code.json")
}

/// The `[providers.<name>].env` variable Kimi reads each interface type's API key
/// from when the user picks "env sub-table" auth. `None` for vertexai, whose
/// credentials come from GCP Application Default Credentials (no inline key).
fn kimi_provider_key_env_var(interface_type: &str) -> Option<&'static str> {
    match interface_type {
        "kimi" => Some("KIMI_API_KEY"),
        "openai" | "openai_responses" => Some("OPENAI_API_KEY"),
        "anthropic" => Some("ANTHROPIC_API_KEY"),
        "google-genai" => Some("GOOGLE_API_KEY"),
        _ => None,
    }
}

/// The resolved iyw-claw-managed provider/model block to write into config.toml.
struct KimiManagedSpec {
    interface_type: String,
    base_url: Option<String>,
    /// Direct `api_key` field (when the user picks "direct key" auth).
    api_key: Option<String>,
    /// `[providers.iyw-claw.env]` sub-table entries — the env-sub-table API key, or
    /// Vertex's `GOOGLE_CLOUD_PROJECT` / `GOOGLE_CLOUD_LOCATION`.
    env: BTreeMap<String, String>,
    model: String,
    max_context_size: Option<i64>,
}

/// Upsert (`Some`) or remove (`None`) the iyw-claw-managed `[providers.iyw-claw]` +
/// `[models.iyw-claw-managed]` block in a parsed config.toml document, preserving
/// every other section the user authored. Removal also resets `default_model`
/// only when it points at our managed alias.
fn apply_kimi_managed_block(
    toml_value: &mut toml::Value,
    spec: Option<&KimiManagedSpec>,
) -> Result<(), AcpError> {
    let table = toml_value
        .as_table_mut()
        .ok_or_else(|| AcpError::protocol("kimi config root must be a TOML table"))?;
    match spec {
        Some(spec) => {
            let providers = table
                .entry("providers".to_string())
                .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
            if !providers.is_table() {
                *providers = toml::Value::Table(toml::map::Map::new());
            }
            let providers = providers.as_table_mut().expect("providers set to table");
            let mut provider_table = toml::map::Map::new();
            provider_table.insert(
                "type".to_string(),
                toml::Value::String(spec.interface_type.clone()),
            );
            if let Some(url) = spec
                .base_url
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                provider_table.insert("base_url".to_string(), toml::Value::String(url.to_string()));
            }
            if let Some(key) = spec
                .api_key
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                provider_table.insert("api_key".to_string(), toml::Value::String(key.to_string()));
            }
            if !spec.env.is_empty() {
                let mut env_table = toml::map::Map::new();
                for (k, v) in &spec.env {
                    let trimmed = v.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    env_table.insert(k.clone(), toml::Value::String(trimmed.to_string()));
                }
                if !env_table.is_empty() {
                    provider_table.insert("env".to_string(), toml::Value::Table(env_table));
                }
            }
            providers.insert(
                KIMI_MANAGED_PROVIDER.to_string(),
                toml::Value::Table(provider_table),
            );

            let models = table
                .entry("models".to_string())
                .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
            if !models.is_table() {
                *models = toml::Value::Table(toml::map::Map::new());
            }
            let models = models.as_table_mut().expect("models set to table");
            let mut model_table = toml::map::Map::new();
            model_table.insert(
                "provider".to_string(),
                toml::Value::String(KIMI_MANAGED_PROVIDER.to_string()),
            );
            model_table.insert("model".to_string(), toml::Value::String(spec.model.clone()));
            // Always emit a positive `max_context_size`: kimi's schema requires it and
            // silently drops the entire model block otherwise (→ empty turns). Fall back
            // to the default window when the user did not specify one.
            let ctx = spec
                .max_context_size
                .filter(|c| *c > 0)
                .unwrap_or(KIMI_DEFAULT_MAX_CONTEXT_SIZE);
            model_table.insert("max_context_size".to_string(), toml::Value::Integer(ctx));
            models.insert(
                KIMI_MANAGED_MODEL_ALIAS.to_string(),
                toml::Value::Table(model_table),
            );

            table.insert(
                "default_model".to_string(),
                toml::Value::String(KIMI_MANAGED_MODEL_ALIAS.to_string()),
            );
        }
        None => {
            let providers_empty = if let Some(providers) = table
                .get_mut("providers")
                .and_then(toml::Value::as_table_mut)
            {
                providers.remove(KIMI_MANAGED_PROVIDER);
                providers.is_empty()
            } else {
                false
            };
            if providers_empty {
                table.remove("providers");
            }
            let models_empty =
                if let Some(models) = table.get_mut("models").and_then(toml::Value::as_table_mut) {
                    models.remove(KIMI_MANAGED_MODEL_ALIAS);
                    models.is_empty()
                } else {
                    false
                };
            if models_empty {
                table.remove("models");
            }
            if table.get("default_model").and_then(toml::Value::as_str)
                == Some(KIMI_MANAGED_MODEL_ALIAS)
            {
                table.remove("default_model");
            }
        }
    }
    Ok(())
}

/// Read-modify-write `config.toml`, upserting (`Some`) or clearing (`None`) the
/// iyw-claw-managed block. A clear on a non-existent file is a no-op (never creates
/// an empty file). Reuses the existing `toml` crate: data in other sections is
/// preserved; comments/formatting are not (the raw editor covers that).
fn mutate_kimi_config_toml(spec: Option<&KimiManagedSpec>) -> Result<(), AcpError> {
    let path = kimi_code_config_toml_path();
    if spec.is_none() && !path.exists() {
        return Ok(());
    }
    let mut toml_value = if path.exists() {
        match fs::read_to_string(&path)
            .ok()
            .and_then(|raw| raw.parse::<toml::Value>().ok())
        {
            Some(existing) if existing.is_table() => existing,
            _ => toml::Value::Table(toml::map::Map::new()),
        }
    } else {
        toml::Value::Table(toml::map::Map::new())
    };
    apply_kimi_managed_block(&mut toml_value, spec)?;
    let serialized = toml::to_string_pretty(&toml_value)
        .map_err(|e| AcpError::protocol(format!("serialize kimi config.toml failed: {e}")))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AcpError::protocol(format!("create kimi config directory failed: {e}")))?;
    }
    fs::write(&path, format!("{serialized}\n"))
        .map_err(|e| AcpError::protocol(format!("write kimi config.toml failed: {e}")))?;
    Ok(())
}

/// Read and parse a token file, if present and valid JSON.
fn read_kimi_token_at(path: &Path) -> Option<serde_json::Value> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

fn read_kimi_token() -> Option<serde_json::Value> {
    read_kimi_token_at(&kimi_code_credentials_token_path())
}

/// Whether a token document is iyw-claw's synthetic gate token (vs a real OAuth
/// login the user performed via `kimi login`). Matches either the sentinel
/// `access_token` or the explicit `_iyw_claw_synthetic` marker.
fn kimi_token_is_synthetic(token: &serde_json::Value) -> bool {
    token
        .get("_iyw_claw_synthetic")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
        || token
            .get("access_token")
            .and_then(serde_json::Value::as_str)
            == Some(KIMI_SYNTHETIC_TOKEN_ACCESS)
}

/// Whether a token document carries a non-empty `access_token` — i.e. would pass
/// `kimi acp`'s session gate.
fn kimi_token_has_access(token: &serde_json::Value) -> bool {
    token
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
}

/// Whether any usable credential (real or synthetic) is present.
fn kimi_credential_present() -> bool {
    read_kimi_token()
        .map(|t| kimi_token_has_access(&t))
        .unwrap_or(false)
}

/// Whether the present credential is iyw-claw's synthetic gate token.
fn kimi_credential_is_synthetic() -> bool {
    read_kimi_token()
        .map(|t| kimi_token_is_synthetic(&t))
        .unwrap_or(false)
}

/// Seed iyw-claw's synthetic gate token at `path` so `kimi acp` treats the session
/// as authenticated. No-op (preserves) when a REAL OAuth login token is already
/// present — that already satisfies the gate and must never be clobbered.
fn seed_kimi_synthetic_credential_at(path: &Path) -> Result<(), AcpError> {
    if let Some(existing) = read_kimi_token_at(path) {
        if kimi_token_has_access(&existing) && !kimi_token_is_synthetic(&existing) {
            return Ok(());
        }
    }
    let token = serde_json::json!({
        "access_token": KIMI_SYNTHETIC_TOKEN_ACCESS,
        "refresh_token": "",
        "expires_at": 9_999_999_999i64,
        "expires_in": 9_999_999i64,
        "scope": "",
        "token_type": "Bearer",
        "_iyw_claw_synthetic": true,
    });
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            AcpError::protocol(format!("create kimi credentials directory failed: {e}"))
        })?;
    }
    let body = serde_json::to_string_pretty(&token)
        .map_err(|e| AcpError::protocol(format!("serialize kimi credential failed: {e}")))?;
    fs::write(path, format!("{body}\n"))
        .map_err(|e| AcpError::protocol(format!("write kimi credential failed: {e}")))?;
    Ok(())
}

fn seed_kimi_synthetic_credential() -> Result<(), AcpError> {
    seed_kimi_synthetic_credential_at(&kimi_code_credentials_token_path())
}

/// Remove the gate token at `path` ONLY when it is iyw-claw's synthetic one —
/// leaving any real OAuth login the user performed untouched.
fn remove_kimi_synthetic_credential_if_ours_at(path: &Path) -> Result<(), AcpError> {
    match read_kimi_token_at(path) {
        Some(token) if kimi_token_is_synthetic(&token) => fs::remove_file(path)
            .map_err(|e| AcpError::protocol(format!("remove kimi credential failed: {e}"))),
        _ => Ok(()),
    }
}

fn remove_kimi_synthetic_credential_if_ours() -> Result<(), AcpError> {
    remove_kimi_synthetic_credential_if_ours_at(&kimi_code_credentials_token_path())
}

/// Project the iyw-claw-managed config.toml block into a flat JSON object for the
/// settings panel, plus the raw file text for the advanced editor. Uses keys
/// (`baseUrl` / `key` / `modelId`, never `apiBaseUrl` / `apiKey` / `model` /
/// `env`) that do NOT match `AgentRuntimeConfig`, so `build_runtime_env_from_setting`
/// never mirrors these file values back into the `KIMI_MODEL_*` runtime env.
fn project_kimi_managed_config(value: &toml::Value) -> serde_json::Map<String, serde_json::Value> {
    let mut merged = serde_json::Map::new();

    if let Some(provider) = value
        .get("providers")
        .and_then(|t| t.get(KIMI_MANAGED_PROVIDER))
        .and_then(toml::Value::as_table)
    {
        let interface_type = provider
            .get("type")
            .and_then(toml::Value::as_str)
            .map(str::to_string);
        if let Some(itype) = &interface_type {
            merged.insert(
                "interfaceType".to_string(),
                serde_json::Value::String(itype.clone()),
            );
        }
        if let Some(url) = provider
            .get("base_url")
            .and_then(toml::Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            merged.insert(
                "baseUrl".to_string(),
                serde_json::Value::String(url.to_string()),
            );
        }
        if let Some(key) = provider
            .get("api_key")
            .and_then(toml::Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            merged.insert(
                "key".to_string(),
                serde_json::Value::String(key.to_string()),
            );
            merged.insert(
                "authType".to_string(),
                serde_json::Value::String("api_key".to_string()),
            );
        }
        if let Some(env) = provider.get("env").and_then(toml::Value::as_table) {
            if let Some(project) = env
                .get("GOOGLE_CLOUD_PROJECT")
                .and_then(toml::Value::as_str)
            {
                merged.insert(
                    "vertexProject".to_string(),
                    serde_json::Value::String(project.to_string()),
                );
            }
            if let Some(location) = env
                .get("GOOGLE_CLOUD_LOCATION")
                .and_then(toml::Value::as_str)
            {
                merged.insert(
                    "vertexLocation".to_string(),
                    serde_json::Value::String(location.to_string()),
                );
            }
            if let Some(var) = interface_type
                .as_deref()
                .and_then(kimi_provider_key_env_var)
            {
                if let Some(key) = env
                    .get(var)
                    .and_then(toml::Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    merged.insert(
                        "key".to_string(),
                        serde_json::Value::String(key.to_string()),
                    );
                    merged.insert(
                        "authType".to_string(),
                        serde_json::Value::String("env".to_string()),
                    );
                }
            }
        }
    }
    if let Some(model) = value
        .get("models")
        .and_then(|t| t.get(KIMI_MANAGED_MODEL_ALIAS))
        .and_then(toml::Value::as_table)
    {
        if let Some(model_id) = model
            .get("model")
            .and_then(toml::Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            merged.insert(
                "modelId".to_string(),
                serde_json::Value::String(model_id.to_string()),
            );
        }
        if let Some(ctx) = model
            .get("max_context_size")
            .and_then(toml::Value::as_integer)
        {
            merged.insert(
                "maxContextSize".to_string(),
                serde_json::Value::Number(ctx.into()),
            );
        }
    }

    let has_managed = merged.contains_key("interfaceType");
    merged.insert(
        "hasManagedBlock".to_string(),
        serde_json::Value::Bool(has_managed),
    );
    merged
}

fn load_kimi_code_config_json() -> Option<String> {
    let raw = fs::read_to_string(kimi_code_config_toml_path()).ok();
    let mut merged = match raw
        .as_deref()
        .and_then(|text| text.parse::<toml::Value>().ok())
    {
        Some(value) => project_kimi_managed_config(&value),
        None => {
            let mut m = serde_json::Map::new();
            m.insert(
                "hasManagedBlock".to_string(),
                serde_json::Value::Bool(false),
            );
            m
        }
    };
    // Surface the gate-credential state so the panel can show whether `kimi acp`
    // is currently authenticated and whether that came from iyw-claw's synthetic
    // token or a real OAuth login.
    merged.insert(
        "credentialPresent".to_string(),
        serde_json::Value::Bool(kimi_credential_present()),
    );
    merged.insert(
        "credentialSynthetic".to_string(),
        serde_json::Value::Bool(kimi_credential_is_synthetic()),
    );
    if let Some(text) = raw {
        merged.insert("rawConfigToml".to_string(), serde_json::Value::String(text));
    }
    serde_json::to_string_pretty(&serde_json::Value::Object(merged)).ok()
}

/// Structured Kimi Code config update from the settings UI. `mode` is one of:
/// `apikey` — write the iyw-claw-managed `config.toml` provider/model block AND seed
/// the synthetic gate token, so the API key actually authenticates `kimi acp`;
/// `login` — clear the managed block + remove our synthetic token so a real OAuth
/// login governs; `raw` — write a verbatim config.toml then seed the gate token.
/// Every mode also clears any stale `KIMI_MODEL_*` env override (it would
/// silently win over config.toml).
#[derive(Debug, Clone)]
pub(crate) struct KimiCodeConfigUpdate {
    pub mode: String,
    pub interface_type: Option<String>,
    pub auth_type: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub max_context_size: Option<i64>,
    pub vertex_project: Option<String>,
    pub vertex_location: Option<String>,
    pub raw_config_toml: Option<String>,
}

/// Validate + resolve a `native`-mode update into the managed block to write.
fn build_kimi_managed_spec(update: &KimiCodeConfigUpdate) -> Result<KimiManagedSpec, AcpError> {
    let interface_type = update
        .interface_type
        .as_deref()
        .map(str::trim)
        .unwrap_or("");
    if !KIMI_INTERFACE_TYPES.contains(&interface_type) {
        return Err(AcpError::protocol(format!(
            "unknown kimi interface type: '{interface_type}'"
        )));
    }
    let model = update
        .model
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AcpError::protocol("kimi native config requires a model id"))?
        .to_string();
    let base_url = update
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if let Some(url) = &base_url {
        if url.contains(['\n', '\r']) {
            return Err(AcpError::protocol(
                "kimi base url must not contain newlines",
            ));
        }
    }

    let mut env: BTreeMap<String, String> = BTreeMap::new();
    let mut api_key: Option<String> = None;

    if interface_type == "vertexai" {
        // Vertex AI: no API key (GCP Application Default Credentials). Persist the
        // project/location into the provider env sub-table.
        if let Some(project) = update
            .vertex_project
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            env.insert("GOOGLE_CLOUD_PROJECT".to_string(), project.to_string());
        }
        if let Some(location) = update
            .vertex_location
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            env.insert("GOOGLE_CLOUD_LOCATION".to_string(), location.to_string());
        }
    } else if let Some(key) = update
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if key.contains(['\n', '\r']) {
            return Err(AcpError::protocol("kimi api key must not contain newlines"));
        }
        // "env" auth writes the key into the provider env sub-table under the
        // interface's canonical key var; otherwise it goes in the inline `api_key`.
        if update.auth_type.as_deref() == Some("env") {
            match kimi_provider_key_env_var(interface_type) {
                Some(var) => {
                    env.insert(var.to_string(), key.to_string());
                }
                None => api_key = Some(key.to_string()),
            }
        } else {
            api_key = Some(key.to_string());
        }
    }

    Ok(KimiManagedSpec {
        interface_type: interface_type.to_string(),
        base_url,
        api_key,
        env,
        model,
        max_context_size: update.max_context_size.filter(|c| *c > 0),
    })
}

/// Clear any `KIMI_MODEL_*` env override from the DB `env_json`, preserving every
/// other env key and the agent's enabled/provider state. `kimi acp` reads that
/// env family BEFORE config.toml, so a stale entry would silently override the
/// iyw-claw-managed provider; every save clears it to keep config.toml authoritative.
/// Ensures the settings row exists first. No-op fast path when nothing to clear.
async fn clear_kimi_model_env(db: &AppDatabase) -> Result<(), AcpError> {
    let default = agent_setting_service::AgentDefaultInput {
        agent_type: AgentType::KimiCode,
        registry_id: registry::registry_id_for(AgentType::KimiCode).to_string(),
        default_sort_order: i32::MAX / 2,
    };
    agent_setting_service::ensure_defaults(&db.conn, &[default])
        .await
        .map_err(|e| AcpError::protocol(e.to_string()))?;
    let setting = agent_setting_service::get_by_agent_type(&db.conn, AgentType::KimiCode)
        .await
        .map_err(|e| AcpError::protocol(e.to_string()))?;
    let enabled = setting.as_ref().map(|m| m.enabled).unwrap_or(true);
    let model_provider_id = setting.as_ref().and_then(|m| m.model_provider_id);
    let mut env: BTreeMap<String, String> = setting
        .and_then(|m| m.env_json)
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    let had = env.remove(KIMI_MODEL_BASE_URL_ENV).is_some()
        | env.remove(KIMI_MODEL_API_KEY_ENV).is_some()
        | env.remove(KIMI_MODEL_NAME_ENV).is_some();
    if !had {
        return Ok(());
    }
    let env_json = serde_json::to_string(&env)
        .map_err(|e| AcpError::protocol(format!("serialize kimi env failed: {e}")))?;
    agent_setting_service::update(
        &db.conn,
        AgentType::KimiCode,
        agent_setting_service::AgentSettingsUpdate {
            enabled,
            env_json: Some(env_json),
            model_provider_id,
        },
    )
    .await
    .map_err(|e| AcpError::protocol(e.to_string()))?;
    Ok(())
}

/// Apply a structured Kimi config update across both stores (DB `env_json` +
/// `~/.kimi-code/config.toml`), keeping exactly one authoritative. Validates the
/// whole request before any write, then writes config.toml first so an env-write
/// failure can never leave the file pointing at credentials that were rolled back.
pub(crate) async fn acp_update_kimi_code_config_core(
    update: KimiCodeConfigUpdate,
    db: &AppDatabase,
    emitter: &EventEmitter,
) -> Result<(), AcpError> {
    let _paths = require_private_agent_storage_for_write()?;
    enum FileAction {
        Managed(Option<KimiManagedSpec>),
        Raw(String),
    }
    // What to do with the synthetic gate token after the config write. `kimi acp`
    // won't open a session without it, so API-key/raw seed it; OAuth-login removes
    // only OUR token (never a real login).
    enum CredentialAction {
        Seed,
        RemoveIfOurs,
    }

    // ---- Plan + validate (no writes yet) ----
    let (file_action, credential_action) = match update.mode.trim() {
        "apikey" => (
            FileAction::Managed(Some(build_kimi_managed_spec(&update)?)),
            CredentialAction::Seed,
        ),
        "login" => (FileAction::Managed(None), CredentialAction::RemoveIfOurs),
        "raw" => {
            let raw = update.raw_config_toml.as_deref().unwrap_or("");
            toml::from_str::<toml::Table>(raw)
                .map_err(|e| AcpError::protocol(format!("invalid kimi config.toml: {e}")))?;
            (FileAction::Raw(raw.to_string()), CredentialAction::Seed)
        }
        other => {
            return Err(AcpError::protocol(format!(
                "unknown kimi config mode: '{other}'"
            )));
        }
    };

    // ---- Apply: config.toml, then the gate token, then clear the env override ----
    match file_action {
        FileAction::Managed(spec) => mutate_kimi_config_toml(spec.as_ref())?,
        FileAction::Raw(raw) => {
            let path = kimi_code_config_toml_path();
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    AcpError::protocol(format!("create kimi config directory failed: {e}"))
                })?;
            }
            fs::write(&path, raw)
                .map_err(|e| AcpError::protocol(format!("write kimi config.toml failed: {e}")))?;
        }
    }
    match credential_action {
        CredentialAction::Seed => seed_kimi_synthetic_credential()?,
        CredentialAction::RemoveIfOurs => remove_kimi_synthetic_credential_if_ours()?,
    }
    clear_kimi_model_env(db).await?;
    emit_acp_agents_updated(emitter, "config_updated", Some(AgentType::KimiCode));
    Ok(())
}

/// `acp_update_kimi_code_config_core` followed by a session staleness refresh.
/// Shared by the Tauri command and the web handler; returns the count of running
/// Kimi sessions left on stale (launch-time) config.
pub(crate) async fn acp_update_kimi_code_config_and_refresh(
    update: KimiCodeConfigUpdate,
    db: &AppDatabase,
    manager: &ConnectionManager,
    data_dir: &Path,
    emitter: &EventEmitter,
) -> Result<usize, AcpError> {
    acp_update_kimi_code_config_core(update, db, emitter).await?;
    Ok(refresh_config_staleness(
        manager,
        db,
        data_dir,
        &[AgentType::KimiCode],
        ConfigStaleKind::AgentConfig,
    )
    .await)
}

/// Validate an API key + endpoint by listing the account's models. GETs
/// `<base_url>/models` with the key as a Bearer token and returns the model ids
/// (OpenAI-compatible `{ "data": [{ "id": ... }] }`). Surfaces the provider's
/// own error message on failure. Lets the settings panel populate a model picker
/// and doubles as a one-click connection test — directly preventing the
/// "Not found the model ..." trap of typing a model the account can't access.
pub(crate) async fn acp_fetch_kimi_models_core(
    base_url: &str,
    api_key: &str,
) -> Result<Vec<String>, AcpError> {
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(AcpError::protocol("base URL is required to list models"));
    }
    let key = api_key.trim();
    if key.is_empty() {
        return Err(AcpError::protocol("API key is required to list models"));
    }
    let url = format!("{base}/models");
    let resp = reqwest::Client::new()
        .get(&url)
        .bearer_auth(key)
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| AcpError::protocol(format!("list models request failed: {e}")))?;
    let status = resp.status();
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AcpError::protocol(format!("list models returned invalid JSON: {e}")))?;
    if !status.is_success() {
        let msg = body
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("request rejected");
        return Err(AcpError::protocol(format!("{status}: {msg}")));
    }
    let mut ids: Vec<String> = body
        .get("data")
        .and_then(serde_json::Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    m.get("id")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default();
    ids.sort();
    ids.dedup();
    Ok(ids)
}

// ---------------------------------------------------------------------------
// Pi config helpers
//
// pi (the self-extensible coding agent, reached over ACP via `pi-acp`) reads its
// model selection from `~/.pi/agent/settings.json` (`defaultProvider`,
// `defaultModel`, `defaultThinkingLevel` — plain strings) and its API keys from
// `~/.pi/agent/auth.json` (`{ "<provider>": { "type": "api_key", "key": ... } }`).
// iyw-claw manages both NATIVE files directly (merge-writes that preserve every
// other key), mirroring how it manages Codex's `auth.json`/`config.toml`. The
// agent dir honors `PI_CODING_AGENT_DIR` so a custom pi install can be targeted.
// ---------------------------------------------------------------------------

/// Resolve pi's coding-agent dir: `PI_CODING_AGENT_DIR` if set (trimmed,
/// non-empty), else `~/.pi/agent` (mirrors `codex_home_dir`/`resolve_kimi_*`).
fn pi_agent_dir() -> PathBuf {
    crate::parsers::profile_paths::pi_agent_dir()
}

fn pi_settings_json_path() -> PathBuf {
    pi_agent_dir().join("settings.json")
}

fn pi_auth_json_path() -> PathBuf {
    pi_agent_dir().join("auth.json")
}

fn pi_models_json_path() -> PathBuf {
    pi_agent_dir().join("models.json")
}

/// Like [`pi_agent_dir`], but resolves `PI_CODING_AGENT_DIR` from a per-agent
/// `runtime_env` map first (the BYO-pi override path) before falling back to the
/// process env / `~/.pi/agent`. Launch-time trust seeding only has the per-agent
/// env (the override never lands in iyw-claw's own process env), so it must consult
/// `runtime_env` to target the same agent dir pi-acp will spawn pi against.
fn pi_agent_dir_for_env(runtime_env: &BTreeMap<String, String>) -> PathBuf {
    match runtime_env
        .get("PI_CODING_AGENT_DIR")
        .map(|raw| raw.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        Some(value) => PathBuf::from(value),
        None => pi_agent_dir(),
    }
}

/// Per-agent `env_json` key gating launch-time workspace-trust seeding for pi.
/// Absent or any value other than `"0"` ⇒ enabled (default on); `"0"` disables.
pub(crate) const PI_TRUST_WORKSPACE_ENV: &str = "PI_ACP_TRUST_WORKSPACE";

/// Seed pi's `trust.json` so the workspace iyw-claw is launching pi into is trusted.
///
/// pi stores trust as a flat `{ "<canonical-dir>": true|false|null }` map and the
/// nearest-ancestor entry decides whether it loads a project's local `.pi/*`
/// config and `.agents/skills`. This gates ONLY config/skill loading, never tool
/// execution — iyw-claw has already authorized full execution in `cwd` by connecting
/// an agent there, so trusting the same folder for config loading is consistent
/// and removes a redundant, mid-connection trust prompt.
///
/// Guarantees: scoped (only `cwd`, never machine-wide), additive-only (never
/// writes `false` or removes entries), idempotent (any existing entry for `cwd` —
/// including a user's explicit `false`/`null` set in pi — is left untouched), and
/// crash-safe for pi's file (a present-but-unparseable `trust.json` is never
/// clobbered). Best-effort: every failure is logged at debug and swallowed so
/// trust seeding can never block a connect. Honors `PI_CODING_AGENT_DIR` via
/// `runtime_env`.
pub(crate) fn seed_pi_workspace_trust(cwd: &Path, runtime_env: &BTreeMap<String, String>) {
    // Default on: only an explicit "0" disables.
    if runtime_env
        .get(PI_TRUST_WORKSPACE_ENV)
        .is_some_and(|v| v.trim() == "0")
    {
        return;
    }
    // pi keys trust by the realpath of the directory; mirror `realpathSync` with
    // `fs::canonicalize`. A non-canonicalizable cwd can't be matched anyway.
    let canonical = match fs::canonicalize(cwd) {
        Ok(p) => p,
        Err(e) => {
            tracing::debug!("[pi] trust seed skipped: canonicalize {cwd:?} failed: {e}");
            return;
        }
    };
    let key = canonical.to_string_lossy().to_string();
    let path = pi_agent_dir_for_env(runtime_env).join("trust.json");

    // Read pi's file strictly: a missing file is fine (we create one), but a file
    // that exists yet doesn't parse to a JSON object must NOT be overwritten —
    // that would destroy decisions iyw-claw can't see.
    let mut obj = match fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(serde_json::Value::Object(map)) => map,
            _ => {
                tracing::debug!("[pi] trust seed skipped: {path:?} is not a JSON object");
                return;
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::Map::new(),
        Err(e) => {
            tracing::debug!("[pi] trust seed skipped: read {path:?} failed: {e}");
            return;
        }
    };

    // Idempotent + respect any decision the user already made for this folder.
    if obj.contains_key(&key) {
        return;
    }
    obj.insert(key, serde_json::Value::Bool(true));
    if let Err(e) = write_json_object_pretty(&path, &obj) {
        tracing::debug!("[pi] trust seed write failed for {path:?}: {e}");
    }
}

/// Structured Pi config update from the settings UI. Writes pi's native files:
/// `settings.json` always (provider/model/thinking level), and `auth.json` only
/// when an API key is supplied (merge-preserving other providers).
#[derive(Debug, Clone)]
pub(crate) struct PiConfigUpdate {
    pub provider: String,
    pub model: String,
    pub thinking_level: Option<String>,
    pub api_key: Option<String>,
    /// When set (non-empty), `provider` is a custom / self-hosted provider: its
    /// definition is merge-written to `models.json` (`baseUrl` + `api`, with the
    /// chosen `model` folded into the provider's `models` array). `None` leaves
    /// `models.json` untouched (built-in provider).
    pub custom_base_url: Option<String>,
    /// Wire protocol for the custom provider (defaults to `openai-completions`).
    /// Ignored when `custom_base_url` is `None`.
    pub custom_api: Option<String>,
}

/// Read a JSON file into an owned object map, returning an empty map when the
/// file is absent, unreadable, or does not parse to a JSON object. Pi's native
/// files are small and iyw-claw-owned; corruption shouldn't abort a save (we
/// re-author the managed keys and preserve whatever else parses).
fn read_json_object_or_empty(path: &Path) -> serde_json::Map<String, serde_json::Value> {
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|value| match value {
            serde_json::Value::Object(map) => Some(map),
            _ => None,
        })
        .unwrap_or_default()
}

/// Pretty-print a JSON object (with a trailing newline) to `path`, creating the
/// parent directory if needed.
fn write_json_object_pretty(
    path: &Path,
    obj: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), AcpError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AcpError::protocol(format!("create pi config directory failed: {e}")))?;
    }
    let mut text = serde_json::to_string_pretty(&serde_json::Value::Object(obj.clone()))
        .map_err(|e| AcpError::protocol(format!("serialize pi config failed: {e}")))?;
    text.push('\n');
    fs::write(path, text)
        .map_err(|e| AcpError::protocol(format!("write pi config failed: {e}")))?;
    Ok(())
}

/// Apply a structured Pi config update to pi's native files. Validates the whole
/// request before any write: provider/model must be non-empty after trim and the
/// API key must not contain newlines (it lands verbatim in a JSON string). Writes
/// `settings.json` first (merge-preserving), then `auth.json` only when an API
/// key is supplied. Ensures the settings row exists, then emits the agents-updated
/// event so the settings panel refreshes.
pub(crate) async fn acp_update_pi_config_core(
    update: PiConfigUpdate,
    db: &AppDatabase,
    emitter: &EventEmitter,
) -> Result<(), AcpError> {
    let _paths = require_private_agent_storage_for_write()?;
    // ---- Validate (no writes yet) ----
    let provider = update.provider.trim();
    if provider.is_empty() {
        return Err(AcpError::protocol("pi provider is required"));
    }
    let model = update.model.trim();
    if model.is_empty() {
        return Err(AcpError::protocol("pi model is required"));
    }
    let thinking_level = update
        .thinking_level
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let api_key = update
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(key) = api_key {
        if key.contains('\n') || key.contains('\r') {
            return Err(AcpError::protocol(
                "pi API key must not contain line breaks",
            ));
        }
    }

    // Ensure the settings row exists (mirrors the kimi flow) so the agent shows
    // up as configured/enabled in the DB-backed settings list.
    let default = agent_setting_service::AgentDefaultInput {
        agent_type: AgentType::Pi,
        registry_id: registry::registry_id_for(AgentType::Pi).to_string(),
        default_sort_order: i32::MAX / 2,
    };
    agent_setting_service::ensure_defaults(&db.conn, &[default])
        .await
        .map_err(|e| AcpError::protocol(e.to_string()))?;

    // ---- settings.json: merge-write provider/model/thinking level ----
    let settings_path = pi_settings_json_path();
    let mut settings = read_json_object_or_empty(&settings_path);
    settings.insert(
        "defaultProvider".to_string(),
        serde_json::Value::String(provider.to_string()),
    );
    settings.insert(
        "defaultModel".to_string(),
        serde_json::Value::String(model.to_string()),
    );
    if let Some(level) = thinking_level {
        settings.insert(
            "defaultThinkingLevel".to_string(),
            serde_json::Value::String(level.to_string()),
        );
    }
    write_json_object_pretty(&settings_path, &settings)?;

    // ---- auth.json: merge-write the provider credential (only when given) ----
    if let Some(key) = api_key {
        let auth_path = pi_auth_json_path();
        let mut auth = read_json_object_or_empty(&auth_path);
        let mut entry = serde_json::Map::new();
        entry.insert(
            "type".to_string(),
            serde_json::Value::String("api_key".to_string()),
        );
        entry.insert(
            "key".to_string(),
            serde_json::Value::String(key.to_string()),
        );
        auth.insert(provider.to_string(), serde_json::Value::Object(entry));
        write_json_object_pretty(&auth_path, &auth)?;
    }

    // ---- models.json: define the custom provider (only when a base URL is
    // given). Built-in providers leave this file untouched. Merge-preserving:
    // `baseUrl`/`api` are overwritten from the form, but any other fields the
    // user hand-tuned (headers/compat/modelOverrides) and previously-defined
    // models are kept; the chosen model is folded into the `models` array. ----
    let custom_base_url = update
        .custom_base_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(base_url) = custom_base_url {
        let custom_api = update
            .custom_api
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("openai-completions");
        let models_path = pi_models_json_path();
        let mut models_doc = read_json_object_or_empty(&models_path);
        let mut providers = match models_doc.remove("providers") {
            Some(serde_json::Value::Object(map)) => map,
            _ => serde_json::Map::new(),
        };
        let mut entry = match providers.remove(provider) {
            Some(serde_json::Value::Object(map)) => map,
            _ => serde_json::Map::new(),
        };
        entry.insert(
            "baseUrl".to_string(),
            serde_json::Value::String(base_url.to_string()),
        );
        entry.insert(
            "api".to_string(),
            serde_json::Value::String(custom_api.to_string()),
        );
        let mut models_arr = match entry.remove("models") {
            Some(serde_json::Value::Array(arr)) => arr,
            _ => Vec::new(),
        };
        let already = models_arr
            .iter()
            .any(|m| m.get("id").and_then(serde_json::Value::as_str) == Some(model));
        if !already {
            let mut model_obj = serde_json::Map::new();
            model_obj.insert(
                "id".to_string(),
                serde_json::Value::String(model.to_string()),
            );
            model_obj.insert(
                "name".to_string(),
                serde_json::Value::String(model.to_string()),
            );
            models_arr.push(serde_json::Value::Object(model_obj));
        }
        entry.insert("models".to_string(), serde_json::Value::Array(models_arr));
        providers.insert(provider.to_string(), serde_json::Value::Object(entry));
        models_doc.insert(
            "providers".to_string(),
            serde_json::Value::Object(providers),
        );
        write_json_object_pretty(&models_path, &models_doc)?;
    }

    emit_acp_agents_updated(emitter, "config_updated", Some(AgentType::Pi));
    Ok(())
}

/// Projection of pi's current native config for the settings panel: the three
/// `settings.json` model keys plus the provider names present in `auth.json`
/// (sorted). Missing files surface as all-`None` / empty.
/// A custom / self-hosted provider defined in `models.json`, projected for the
/// settings panel so it can rehydrate the custom-provider form (and detect that
/// the current `defaultProvider` is a custom one).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PiCustomProvider {
    pub id: String,
    pub base_url: String,
    pub api: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PiConfigProjection {
    pub default_provider: Option<String>,
    pub default_model: Option<String>,
    pub default_thinking_level: Option<String>,
    pub auth_providers: Vec<String>,
    pub custom_providers: Vec<PiCustomProvider>,
}

/// Read pi's native files into a `PiConfigProjection`. Never errors: absent or
/// malformed files yield `None` / an empty provider list (the panel treats that
/// as "not configured yet").
pub(crate) fn load_pi_config_core() -> PiConfigProjection {
    let settings = read_json_object_or_empty(&pi_settings_json_path());
    let string_key = |key: &str| {
        settings
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    };
    let mut auth_providers: Vec<String> = read_json_object_or_empty(&pi_auth_json_path())
        .keys()
        .cloned()
        .collect();
    auth_providers.sort();
    let mut custom_providers: Vec<PiCustomProvider> =
        read_json_object_or_empty(&pi_models_json_path())
            .get("providers")
            .and_then(serde_json::Value::as_object)
            .map(|providers| {
                providers
                    .iter()
                    .map(|(id, entry)| PiCustomProvider {
                        id: id.clone(),
                        base_url: entry
                            .get("baseUrl")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        api: entry
                            .get("api")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("openai-completions")
                            .to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();
    custom_providers.sort_by(|a, b| a.id.cmp(&b.id));
    PiConfigProjection {
        default_provider: string_key("defaultProvider"),
        default_model: string_key("defaultModel"),
        default_thinking_level: string_key("defaultThinkingLevel"),
        auth_providers,
        custom_providers,
    }
}

/// Result of validating a user-supplied custom pi binary (BYO-pi). `found=false`
/// with `resolved_path=None` is a normal result (not an error) — the panel shows
/// "not found" rather than surfacing an exception.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PiCommandValidation {
    pub found: bool,
    pub resolved_path: Option<String>,
    pub version: Option<String>,
}

/// Best-effort check that `resolved` looks executable on unix (any execute bit
/// set). On non-unix we already know it exists; treat that as good enough.
#[cfg(unix)]
fn pi_path_is_executable(resolved: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(resolved)
        .map(|meta| meta.is_file() && (meta.permissions().mode() & 0o111 != 0))
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn pi_path_is_executable(resolved: &Path) -> bool {
    resolved.is_file()
}

/// Resolve a user-supplied pi command. A value containing a path separator (or an
/// absolute path) is treated as a path and checked on disk; a bare name is looked
/// up on `PATH` via the `which` crate. On success, best-effort `--version` is run
/// (failures tolerated → `version=None`). Never errors on a not-found / probe
/// failure: returns `found=false` (or `version=None`) instead.
/// Resolve a pi command to an executable path: a value containing a path
/// separator (or an absolute path) is checked on disk; a bare name is looked up
/// on `PATH` via the `which` crate. Returns `None` when it can't be resolved to
/// an executable. Shared by the BYO-pi validate command and the launch preflight
/// ([`crate::acp::connection`]) so both agree on what "pi is resolvable" means —
/// and both see the same `PATH` the spawned pi-acp process inherits.
pub(crate) fn resolve_pi_command_path(command: &str) -> Option<PathBuf> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return None;
    }

    let looks_like_path = trimmed.contains(std::path::MAIN_SEPARATOR)
        || trimmed.contains('/')
        || Path::new(trimmed).is_absolute();

    if looks_like_path {
        let candidate = Path::new(trimmed);
        if pi_path_is_executable(candidate) {
            // Canonicalize to an absolute path; fall back to the raw path if the
            // FS rejects canonicalization (e.g. permissions) but it is executable.
            Some(fs::canonicalize(candidate).unwrap_or_else(|_| candidate.to_path_buf()))
        } else {
            None
        }
    } else {
        which::which(trimmed).ok()
    }
}

pub(crate) fn acp_validate_pi_command_core(command: String) -> PiCommandValidation {
    let Some(resolved_path) = resolve_pi_command_path(&command) else {
        return PiCommandValidation {
            found: false,
            resolved_path: None,
            version: None,
        };
    };

    let version = probe_pi_version(&resolved_path);
    PiCommandValidation {
        found: true,
        resolved_path: Some(resolved_path.to_string_lossy().into_owned()),
        version,
    }
}

/// Best-effort `<resolved> --version`, returning the trimmed first stdout line.
/// Any failure (spawn error, non-zero exit, empty output) → `None`; never panics
/// and never blocks indefinitely (`Command::output` waits for the short-lived
/// `--version` child to exit on its own).
fn probe_pi_version(resolved: &Path) -> Option<String> {
    let output = std::process::Command::new(resolved)
        .arg("--version")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
}

// ---------------------------------------------------------------------------
// Hermes config helpers
//
// Hermes self-manages credentials in `~/.hermes/.env` (secrets) and general
// settings in `~/.hermes/config.yaml` (the `model:` section), reading them with
// its own runtime resolver. iyw-claw manages those two files directly — mirroring
// how it manages Codex's `auth.json` + `config.toml` — rather than injecting
// process env. The provider choice drives the linkage: it selects which `.env`
// var holds the API key and which `model.provider` / `model.base_url` go into
// config.yaml.
// ---------------------------------------------------------------------------

fn hermes_env_path() -> PathBuf {
    hermes_home_dir().join(".env")
}

pub(crate) fn hermes_config_yaml_path() -> PathBuf {
    hermes_home_dir().join("config.yaml")
}

/// A managed Hermes provider: the config.yaml `model.provider` value (its `id`)
/// and the `.env` variable that carries its API key. `key_env_var` is the
/// variable Hermes' own setup writes first (mirrors `auth.py` PROVIDER_REGISTRY
/// priority order); it is empty for OAuth providers (credentials set via the
/// terminal `--setup` flow) and AWS Bedrock (resolved from the AWS SDK chain).
/// `needs_base_url` marks providers whose endpoint is user-supplied (the
/// OpenAI-compatible `openai-api` path). The frontend mirror owns the auth-kind
/// UI flag.
struct HermesProvider {
    id: &'static str,
    key_env_var: &'static str,
    needs_base_url: bool,
    /// The `.env` variable Hermes reads for a user-supplied endpoint URL. When
    /// set (only `openai-api` today), iyw-claw mirrors the structured base URL into
    /// both this var and config.yaml `model.base_url`, because Hermes' own
    /// resolution paths disagree on which one wins — keeping them in sync makes
    /// the saved endpoint authoritative under either path.
    base_url_env_var: &'static str,
}

/// Curated subset of Hermes providers iyw-claw edits via structured fields, keyed
/// by the canonical `model.provider` id and `.env` key var from Hermes'
/// `hermes_cli/auth.py` PROVIDER_REGISTRY (the single source of truth its own
/// setup uses). The long tail and any exotic credential layout go through the
/// raw config.yaml escape hatch and the terminal `--setup` flow.
const HERMES_PROVIDERS: &[HermesProvider] = &[
    // API-key providers — `key_env_var` is the first env var Hermes' own
    // setup writes (auth.py PROVIDER_REGISTRY priority order).
    HermesProvider {
        id: "openrouter",
        key_env_var: "OPENROUTER_API_KEY",
        needs_base_url: false,
        base_url_env_var: "OPENROUTER_BASE_URL",
    },
    HermesProvider {
        id: "openai-api",
        key_env_var: "OPENAI_API_KEY",
        needs_base_url: true,
        base_url_env_var: "OPENAI_BASE_URL",
    },
    // User-supplied OpenAI-compatible endpoint. Unlike every other provider,
    // `custom` carries BOTH its key and endpoint INLINE in config.yaml
    // (`model.api_key` / `model.base_url`) and reads no `.env` var — verified
    // against a working 0.16.0 config and `hermes_cli/auth.py`, where `custom`
    // is a canonical provider. Empty key/base-url env vars keep the `.env`
    // writer and the panel projection away; `plan_hermes_write` /
    // `project_hermes_key_and_base` special-case the inline key via
    // `hermes_inlines_api_key`.
    HermesProvider {
        id: "custom",
        key_env_var: "",
        needs_base_url: true,
        base_url_env_var: "",
    },
    HermesProvider {
        id: "anthropic",
        key_env_var: "ANTHROPIC_API_KEY",
        needs_base_url: false,
        base_url_env_var: "ANTHROPIC_BASE_URL",
    },
    HermesProvider {
        id: "gemini",
        key_env_var: "GOOGLE_API_KEY",
        needs_base_url: false,
        base_url_env_var: "GEMINI_BASE_URL",
    },
    HermesProvider {
        id: "deepseek",
        key_env_var: "DEEPSEEK_API_KEY",
        needs_base_url: false,
        base_url_env_var: "DEEPSEEK_BASE_URL",
    },
    HermesProvider {
        id: "xai",
        key_env_var: "XAI_API_KEY",
        needs_base_url: false,
        base_url_env_var: "XAI_BASE_URL",
    },
    HermesProvider {
        id: "zai",
        key_env_var: "GLM_API_KEY",
        needs_base_url: false,
        base_url_env_var: "GLM_BASE_URL",
    },
    HermesProvider {
        id: "minimax",
        key_env_var: "MINIMAX_API_KEY",
        needs_base_url: false,
        base_url_env_var: "MINIMAX_BASE_URL",
    },
    HermesProvider {
        id: "minimax-cn",
        key_env_var: "MINIMAX_CN_API_KEY",
        needs_base_url: false,
        base_url_env_var: "MINIMAX_CN_BASE_URL",
    },
    HermesProvider {
        id: "kimi-coding",
        key_env_var: "KIMI_API_KEY",
        needs_base_url: false,
        base_url_env_var: "KIMI_BASE_URL",
    },
    HermesProvider {
        id: "kimi-coding-cn",
        key_env_var: "KIMI_CN_API_KEY",
        needs_base_url: false,
        base_url_env_var: "",
    },
    HermesProvider {
        id: "nvidia",
        key_env_var: "NVIDIA_API_KEY",
        needs_base_url: false,
        base_url_env_var: "NVIDIA_BASE_URL",
    },
    HermesProvider {
        id: "alibaba",
        key_env_var: "DASHSCOPE_API_KEY",
        needs_base_url: false,
        base_url_env_var: "DASHSCOPE_BASE_URL",
    },
    HermesProvider {
        id: "alibaba-coding-plan",
        key_env_var: "ALIBABA_CODING_PLAN_API_KEY",
        needs_base_url: false,
        base_url_env_var: "ALIBABA_CODING_PLAN_BASE_URL",
    },
    HermesProvider {
        id: "copilot",
        key_env_var: "COPILOT_GITHUB_TOKEN",
        needs_base_url: false,
        base_url_env_var: "COPILOT_API_BASE_URL",
    },
    HermesProvider {
        id: "lmstudio",
        key_env_var: "LM_API_KEY",
        needs_base_url: true,
        base_url_env_var: "LM_BASE_URL",
    },
    HermesProvider {
        id: "azure-foundry",
        key_env_var: "AZURE_FOUNDRY_API_KEY",
        needs_base_url: true,
        base_url_env_var: "AZURE_FOUNDRY_BASE_URL",
    },
    HermesProvider {
        id: "stepfun",
        key_env_var: "STEPFUN_API_KEY",
        needs_base_url: false,
        base_url_env_var: "STEPFUN_BASE_URL",
    },
    HermesProvider {
        id: "arcee",
        key_env_var: "ARCEEAI_API_KEY",
        needs_base_url: false,
        base_url_env_var: "ARCEE_BASE_URL",
    },
    HermesProvider {
        id: "gmi",
        key_env_var: "GMI_API_KEY",
        needs_base_url: false,
        base_url_env_var: "GMI_BASE_URL",
    },
    HermesProvider {
        id: "huggingface",
        key_env_var: "HF_TOKEN",
        needs_base_url: false,
        base_url_env_var: "HF_BASE_URL",
    },
    HermesProvider {
        id: "kilocode",
        key_env_var: "KILOCODE_API_KEY",
        needs_base_url: false,
        base_url_env_var: "KILOCODE_BASE_URL",
    },
    HermesProvider {
        id: "opencode-zen",
        key_env_var: "OPENCODE_ZEN_API_KEY",
        needs_base_url: false,
        base_url_env_var: "OPENCODE_ZEN_BASE_URL",
    },
    HermesProvider {
        id: "opencode-go",
        key_env_var: "OPENCODE_GO_API_KEY",
        needs_base_url: false,
        base_url_env_var: "OPENCODE_GO_BASE_URL",
    },
    HermesProvider {
        id: "xiaomi",
        key_env_var: "XIAOMI_API_KEY",
        needs_base_url: false,
        base_url_env_var: "XIAOMI_BASE_URL",
    },
    HermesProvider {
        id: "tencent-tokenhub",
        key_env_var: "TOKENHUB_API_KEY",
        needs_base_url: false,
        base_url_env_var: "TOKENHUB_BASE_URL",
    },
    HermesProvider {
        id: "ollama-cloud",
        key_env_var: "OLLAMA_API_KEY",
        needs_base_url: false,
        base_url_env_var: "OLLAMA_BASE_URL",
    },
    HermesProvider {
        id: "novita",
        key_env_var: "NOVITA_API_KEY",
        needs_base_url: false,
        base_url_env_var: "NOVITA_BASE_URL",
    },
    // OAuth / external-process providers — credentials set via the terminal
    // `--setup` flow; no `.env` key var.
    HermesProvider {
        id: "nous",
        key_env_var: "",
        needs_base_url: false,
        base_url_env_var: "",
    },
    HermesProvider {
        id: "openai-codex",
        key_env_var: "",
        needs_base_url: false,
        base_url_env_var: "",
    },
    HermesProvider {
        id: "minimax-oauth",
        key_env_var: "",
        needs_base_url: false,
        base_url_env_var: "",
    },
    HermesProvider {
        id: "xai-oauth",
        key_env_var: "",
        needs_base_url: false,
        base_url_env_var: "",
    },
    HermesProvider {
        id: "qwen-oauth",
        key_env_var: "",
        needs_base_url: false,
        base_url_env_var: "",
    },
    HermesProvider {
        id: "google-gemini-cli",
        key_env_var: "",
        needs_base_url: false,
        base_url_env_var: "",
    },
    HermesProvider {
        id: "copilot-acp",
        key_env_var: "",
        needs_base_url: false,
        base_url_env_var: "",
    },
    // AWS Bedrock — credentials from the AWS SDK chain.
    HermesProvider {
        id: "bedrock",
        key_env_var: "",
        needs_base_url: false,
        base_url_env_var: "",
    },
];

fn hermes_provider(id: &str) -> Option<&'static HermesProvider> {
    HERMES_PROVIDERS.iter().find(|p| p.id == id)
}

/// Whether a provider stores its API key INLINE in config.yaml `model.api_key`
/// rather than in `~/.hermes/.env`. Only `custom` (the user-supplied
/// OpenAI-compatible endpoint) works this way in Hermes 0.16.0: its registry
/// entry has no `.env` key var, so the key rides in the `model:` section next to
/// `base_url`. Drives both the structured write (`plan_hermes_write`) and the
/// panel projection (`project_hermes_key_and_base`).
fn hermes_inlines_api_key(provider: &str) -> bool {
    provider == "custom"
}

/// Parse simple `KEY=value` lines from a dotenv file. Ignores blank lines and
/// `#` comments, tolerates a leading `export `, and strips one layer of
/// surrounding single/double quotes from the value. Last occurrence wins.
fn parse_env_file(raw: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let body = trimmed.strip_prefix("export ").unwrap_or(trimmed);
        let Some((key, value)) = body.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value);
        map.insert(key.to_string(), value.to_string());
    }
    map
}

/// Update `KEY=value` entries in a dotenv file while preserving comments, blank
/// lines, ordering, and unrelated keys. The first occurrence of an updated key
/// is replaced in place; any later duplicates of that key are dropped (so a
/// last-occurrence-wins reader can't surface a stale shadowing line). Missing
/// keys are appended — including with an empty value (`KEY=`): Hermes loads
/// `~/.hermes/.env` with override semantics, so an explicit empty line both
/// clears a stored credential AND masks an inherited process-env value of the
/// same name (e.g. a stale `OPENAI_API_KEY` exported in the shell).
fn patch_env_text(existing: &str, updates: &[(&str, &str)]) -> String {
    let mut applied = vec![false; updates.len()];
    let mut out_lines: Vec<String> = Vec::new();

    for line in existing.lines() {
        let trimmed = line.trim_start();
        let line_key = if trimmed.starts_with('#') {
            None
        } else {
            let body = trimmed.strip_prefix("export ").unwrap_or(trimmed);
            body.split_once('=').map(|(k, _)| k.trim())
        };
        if let Some(line_key) = line_key {
            if let Some(i) = updates.iter().position(|(key, _)| line_key == *key) {
                if applied[i] {
                    // Drop later duplicates of a key we already rewrote.
                    continue;
                }
                out_lines.push(format!("{}={}", updates[i].0, updates[i].1));
                applied[i] = true;
                continue;
            }
        }
        out_lines.push(line.to_string());
    }

    for (i, (key, value)) in updates.iter().enumerate() {
        // Append a missing key, including an empty `KEY=` — an explicit empty
        // line is what masks an inherited process-env value under Hermes' dotenv
        // override loading, not just a no-op cleanup.
        if !applied[i] {
            out_lines.push(format!("{key}={value}"));
        }
    }

    let mut result = out_lines.join("\n");
    if !result.is_empty() {
        result.push('\n');
    }
    result
}

fn yaml_str(value: &serde_yaml::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Read `model.provider` from an existing config.yaml document, if present. Used
/// to tell an out-of-band base URL for the *current* provider (keep) apart from a
/// stale one left by a provider the user just switched away from (clear).
fn existing_hermes_model_provider(existing: Option<&str>) -> Option<String> {
    let raw = existing?;
    let value: serde_yaml::Value = serde_yaml::from_str(raw).ok()?;
    value.get("model").and_then(|m| yaml_str(m, "provider"))
}

/// How `merge_hermes_model_config` should treat the `model.base_url` field.
enum BaseUrlWrite<'a> {
    /// Write this endpoint, or remove the field when the value is empty/blank.
    /// Used for providers whose base URL is user-editable in the panel.
    Set(&'a str),
    /// Leave any existing `model.base_url` untouched. Used for providers whose
    /// endpoint is not exposed in the structured fields, so a base URL set
    /// out-of-band (a proxy/Azure endpoint, etc.) survives a structured save.
    Preserve,
}

/// How `merge_hermes_model_config` should treat the inline `model.api_key`
/// (and the companion `model.api_mode`), which only the `custom` provider uses.
enum InlineApiKeyWrite<'a> {
    /// Inline-key provider (`custom`): write `key` (or remove the field when
    /// blank — a keyless local server). `scrub_mode` clears a stale
    /// `model.api_mode`: `true` when switching TO custom from a different
    /// provider (the prior mode must not bleed in), `false` on a custom→custom
    /// re-save so a user's raw-editor `api_mode` (e.g. `anthropic_messages` for
    /// an Anthropic-compatible proxy) survives a structured save.
    Set { key: &'a str, scrub_mode: bool },
    /// Non-inline provider (keyed/OAuth/AWS): scrub any stale inline
    /// `model.api_key` / `model.api_mode` left over from a previous `custom`
    /// endpoint so it can't bleed into the newly selected provider — mirroring
    /// Hermes' own `auth.py` cleanup on a provider switch.
    Clear,
}

/// Set `model.{provider,default,base_url}` in a Hermes config.yaml document,
/// preserving every other top-level key. `default` is only written when a
/// non-empty model is given; `base_url` follows the `BaseUrlWrite` action and
/// the inline `model.api_key` follows the `InlineApiKeyWrite` action.
fn merge_hermes_model_config(
    existing: Option<&str>,
    provider: &str,
    model: &str,
    base_url: BaseUrlWrite<'_>,
    inline_api_key: InlineApiKeyWrite<'_>,
) -> Result<String, AcpError> {
    use serde_yaml::{Mapping, Value};
    let mut root: Value = match existing {
        Some(raw) if !raw.trim().is_empty() => serde_yaml::from_str(raw)
            .map_err(|e| AcpError::protocol(format!("invalid hermes config.yaml: {e}")))?,
        _ => Value::Mapping(Mapping::new()),
    };
    if !root.is_mapping() {
        root = Value::Mapping(Mapping::new());
    }
    let root_map = root.as_mapping_mut().expect("root is a mapping");

    let model_key = Value::String("model".to_string());
    if !root_map
        .get(&model_key)
        .map(Value::is_mapping)
        .unwrap_or(false)
    {
        root_map.insert(model_key.clone(), Value::Mapping(Mapping::new()));
    }
    let model_map = root_map
        .get_mut(&model_key)
        .and_then(Value::as_mapping_mut)
        .expect("model is a mapping");

    model_map.insert(
        Value::String("provider".to_string()),
        Value::String(provider.to_string()),
    );
    if !model.is_empty() {
        model_map.insert(
            Value::String("default".to_string()),
            Value::String(model.to_string()),
        );
    }
    match base_url {
        BaseUrlWrite::Set(url) if !url.trim().is_empty() => {
            model_map.insert(
                Value::String("base_url".to_string()),
                Value::String(url.trim().to_string()),
            );
        }
        BaseUrlWrite::Set(_) => {
            model_map.remove(Value::String("base_url".to_string()));
        }
        // Preserve: leave whatever `model.base_url` is already there.
        BaseUrlWrite::Preserve => {}
    }
    match inline_api_key {
        InlineApiKeyWrite::Set { key, scrub_mode } => {
            if key.trim().is_empty() {
                // Blank key on an inline provider → keyless local server.
                model_map.remove(Value::String("api_key".to_string()));
            } else {
                model_map.insert(
                    Value::String("api_key".to_string()),
                    Value::String(key.trim().to_string()),
                );
            }
            // Switching TO custom scrubs a stale mode; a custom→custom re-save
            // leaves a user's raw-editor `api_mode` untouched.
            if scrub_mode {
                model_map.remove(Value::String("api_mode".to_string()));
            }
        }
        // Non-inline provider: scrub a stale inline key/mode from a prior `custom`.
        InlineApiKeyWrite::Clear => {
            model_map.remove(Value::String("api_key".to_string()));
            model_map.remove(Value::String("api_mode".to_string()));
        }
    }

    serde_yaml::to_string(&root)
        .map_err(|e| AcpError::protocol(format!("serialize hermes config.yaml failed: {e}")))
}

/// Quote a single argv token for the current platform's shell, only when it
/// contains characters that would otherwise be reparsed (so simple tokens stay
/// readable). POSIX uses single quotes; Windows wraps in double quotes.
fn shell_quote_arg(arg: &str) -> String {
    shell_quote_arg_for(arg, cfg!(windows))
}

/// Platform-parameterized core of [`shell_quote_arg`], so both the POSIX and
/// Windows quoting rules are unit-testable on any host.
///
/// The backslash forces quoting on POSIX (it is the shell escape char) but NOT
/// on Windows, where it is just the path separator. Force-quoting a plain
/// Windows path like `C:\…\uvx.exe` makes the rendered command *begin* with a
/// double-quoted string: `cmd.exe` runs that fine, but PowerShell parses a
/// leading quoted string as a string *expression* (invoking it would need the
/// `&` call operator) and dies with "Unexpected token" on the next argument —
/// uvx never runs. Leaving a space-free path unquoted keeps it a bare command
/// token that runs in both `cmd` and PowerShell. (A path that contains spaces
/// still must be quoted; such a copied command stays PowerShell-incompatible and
/// needs a leading `&` when pasted there.)
fn shell_quote_arg_for(arg: &str, windows: bool) -> String {
    // Metacharacters that force quoting. Backslash is POSIX-only: on Windows it
    // is the path separator and quoting on its account is what breaks PowerShell.
    let special: &str = if windows {
        "[](){}'\"$&;|<>*?`!#~"
    } else {
        "[](){}'\"$&;|<>*?`\\!#~"
    };
    let needs_quoting = arg.is_empty()
        || arg
            .chars()
            .any(|c| c.is_whitespace() || special.contains(c));
    if !needs_quoting {
        return arg.to_string();
    }
    if windows {
        format!("\"{}\"", arg.replace('"', "\\\""))
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

fn shell_join(argv: &[String]) -> String {
    argv.iter()
        .map(|a| shell_quote_arg(a))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(any(feature = "tauri-runtime", test))]
fn with_private_uv_shell_env(command: &str, paths: &AgentStoragePaths, windows: bool) -> String {
    let env = binary_cache::uv_runtime_env(paths);
    if windows {
        let assignments = env
            .into_iter()
            .map(|(key, value)| format!("set \"{key}={}\"", value.display()))
            .collect::<Vec<_>>()
            .join(" && ");
        format!("{assignments} && {command}")
    } else {
        let assignments = env
            .into_iter()
            .map(|(key, value)| {
                format!(
                    "{key}={}",
                    shell_quote_arg_for(&value.to_string_lossy(), false)
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        format!("{assignments} {command}")
    }
}

/// The argv for Hermes's `--setup` and `model` flows. Private storage uses only
/// the managed uvx recipe; the system Hermes CLI is considered only before
/// initialization. Returned as argv vectors for display or terminal execution.
fn hermes_setup_argvs() -> (Vec<String>, Vec<String>) {
    let meta = registry::get_agent_meta(AgentType::Hermes);
    if let registry::AgentDistribution::Uvx {
        package,
        cmd,
        python,
        system_cmd,
        ..
    } = meta.distribution
    {
        if AgentStoragePaths::active().is_none() {
            if let Some((sys, _)) = system_cmd {
                if resolve_command_on_path(sys).is_some() {
                    return (
                        vec![sys.to_string(), "acp".to_string(), "--setup".to_string()],
                        vec![sys.to_string(), "model".to_string()],
                    );
                }
            }
        }
        let uvx = resolve_uvx_command()
            .or_else(|| {
                AgentStoragePaths::active()
                    .map(|paths| binary_cache::managed_uv_tool_path(&paths, "uvx"))
            })
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "uvx".to_string());
        let python_args = uvx_python_args(python);
        // `uvx [--python <ver>] --from <package> <tail...>` — the pin must
        // precede `--from`, matching the launch/prewarm invocations.
        let build = |tail: &[&str]| -> Vec<String> {
            let mut argv = vec![uvx.clone()];
            argv.extend(python_args.iter().cloned());
            argv.push("--from".to_string());
            argv.push(package.to_string());
            argv.extend(tail.iter().map(|s| s.to_string()));
            argv
        };
        return (build(&[cmd, "--setup"]), build(&["hermes", "model"]));
    }
    // Unreachable: Hermes is always a Uvx distribution.
    (
        vec![
            "uvx".to_string(),
            "--python".to_string(),
            "3.13".to_string(),
            "--from".to_string(),
            "hermes-agent[acp,mcp]==0.16.0".to_string(),
            "hermes-acp".to_string(),
            "--setup".to_string(),
        ],
        vec![
            "uvx".to_string(),
            "--python".to_string(),
            "3.13".to_string(),
            "--from".to_string(),
            "hermes-agent[acp,mcp]==0.16.0".to_string(),
            "hermes".to_string(),
            "model".to_string(),
        ],
    )
}

/// Build the displayed/runnable `(setup, model)` shell commands for the Hermes
/// setup guidance, shell-quoted for the current platform.
fn hermes_setup_commands() -> (String, String) {
    let (setup, model) = hermes_setup_argvs();
    (shell_join(&setup), shell_join(&model))
}

/// Read `~/.hermes/.env` + `config.yaml` and project them into the normalized
/// JSON the settings UI binds to: `{provider, model, baseUrl, apiKey,
/// hermesHome, setupCommand, modelCommand}`. Only the active provider's single
/// key var is surfaced — never the rest of `.env`.
/// Project the active provider's API key and endpoint URL for the settings UI.
/// For inline-key providers (`custom`) the key comes from config.yaml's
/// `model.api_key`; for every other keyed provider it is read from the
/// provider's `.env` key var. The base URL prefers config.yaml's
/// `model.base_url` and falls back to the provider's base-URL env var — so an
/// endpoint that lives only in `.env` (e.g. a bare `OPENAI_BASE_URL` with no
/// YAML `base_url`) still shows in the panel and isn't cleared on the next save.
/// Empty stored values are treated as absent. Unknown providers map to nothing
/// here (their key var is undiscoverable; the raw editor governs).
fn project_hermes_key_and_base(
    provider: &str,
    env_map: &BTreeMap<String, String>,
    yaml_base_url: Option<&str>,
    yaml_api_key: Option<&str>,
) -> (Option<String>, Option<String>) {
    let meta = hermes_provider(provider);
    let api_key = if hermes_inlines_api_key(provider) {
        yaml_api_key
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    } else {
        meta.filter(|p| !p.key_env_var.is_empty())
            .and_then(|p| env_map.get(p.key_env_var))
            .filter(|v| !v.is_empty())
            .map(|v| v.to_string())
    };
    let base_url = yaml_base_url.map(str::to_string).or_else(|| {
        meta.filter(|p| !p.base_url_env_var.is_empty())
            .and_then(|p| env_map.get(p.base_url_env_var))
            .filter(|v| !v.is_empty())
            .map(|v| v.to_string())
    });
    (api_key, base_url)
}

fn load_hermes_local_config_json() -> Option<String> {
    let env_map = fs::read_to_string(hermes_env_path())
        .ok()
        .map(|raw| parse_env_file(&raw))
        .unwrap_or_default();

    let mut provider: Option<String> = None;
    let mut model: Option<String> = None;
    let mut yaml_base_url: Option<String> = None;
    let mut yaml_api_key: Option<String> = None;
    if let Ok(raw_yaml) = fs::read_to_string(hermes_config_yaml_path()) {
        if let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&raw_yaml) {
            if let Some(model_section) = value.get("model") {
                provider = yaml_str(model_section, "provider");
                model = yaml_str(model_section, "default");
                yaml_base_url = yaml_str(model_section, "base_url");
                yaml_api_key = yaml_str(model_section, "api_key");
            }
        }
    }

    let (api_key, base_url) = match provider.as_deref() {
        Some(p) => project_hermes_key_and_base(
            p,
            &env_map,
            yaml_base_url.as_deref(),
            yaml_api_key.as_deref(),
        ),
        None => (None, yaml_base_url),
    };

    let (setup_command, model_command) = hermes_setup_commands();

    let mut merged = serde_json::Map::new();
    if let Some(value) = provider {
        merged.insert("provider".to_string(), serde_json::Value::String(value));
    }
    if let Some(value) = model {
        merged.insert("model".to_string(), serde_json::Value::String(value));
    }
    if let Some(value) = base_url {
        merged.insert("baseUrl".to_string(), serde_json::Value::String(value));
    }
    if let Some(value) = api_key {
        merged.insert("apiKey".to_string(), serde_json::Value::String(value));
    }
    merged.insert(
        "hermesHome".to_string(),
        serde_json::Value::String(hermes_home_dir().display().to_string()),
    );
    merged.insert(
        "setupCommand".to_string(),
        serde_json::Value::String(setup_command),
    );
    merged.insert(
        "modelCommand".to_string(),
        serde_json::Value::String(model_command),
    );

    serde_json::to_string_pretty(&serde_json::Value::Object(merged)).ok()
}

/// Structured Hermes config update from the settings UI.
#[derive(Debug, Clone)]
pub(crate) struct HermesConfigUpdate {
    pub provider: String,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    /// When present, the raw config.yaml is validated and written verbatim
    /// (advanced mode), bypassing the structured `model:` merge.
    pub raw_config_yaml: Option<String>,
}

/// Whether to skip tightening Hermes file/dir permissions, mirroring the opt-outs
/// Hermes itself honors: containerized / managed deployments (Docker/Podman/LXC/
/// Kubernetes volume mounts with mapped UIDs, etc.) where forcing `0700`/`0600`
/// breaks the multi-process access model. Mirrors Hermes 0.16.0 `_is_container`:
/// the `HERMES_CONTAINER` / `HERMES_SKIP_CHMOD` env opt-outs, the Docker
/// (`/.dockerenv`) and Podman (`/run/.containerenv`) markers, and a
/// docker/lxc/kubepods marker in `/proc/1/cgroup`.
#[cfg(unix)]
fn hermes_skip_chmod() -> bool {
    // Match Hermes' Python truthiness (`os.environ.get(...)` — an empty value is
    // falsy): only a NON-EMPTY opt-out enables skip, so a blank `HERMES_SKIP_CHMOD=`
    // does not (and iyw-claw still performs the 0644→0600 repair Hermes would).
    let truthy = |key: &str| std::env::var(key).map(|v| !v.is_empty()).unwrap_or(false);
    if truthy("HERMES_CONTAINER")
        || truthy("HERMES_SKIP_CHMOD")
        || Path::new("/.dockerenv").exists()
        || Path::new("/run/.containerenv").exists()
    {
        return true;
    }
    fs::read_to_string("/proc/1/cgroup")
        .map(|cgroup| {
            cgroup.contains("docker") || cgroup.contains("lxc") || cgroup.contains("kubepods")
        })
        .unwrap_or(false)
}

/// Parse a `HERMES_HOME_MODE` value (octal, e.g. `0701` for web-server traversal
/// layouts), falling back to owner-only `0700`. Accepts an optional `0o` prefix.
#[cfg(unix)]
fn parse_hermes_home_mode(raw: Option<&str>) -> u32 {
    raw.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.strip_prefix("0o").unwrap_or(s))
        .and_then(|s| u32::from_str_radix(s, 8).ok())
        .filter(|m| *m != 0)
        .unwrap_or(0o700)
}

/// Create the Hermes home directory if needed. On Unix, tighten it to
/// `HERMES_HOME_MODE` (or `0700`) **only when iyw-claw just created it** and Hermes
/// itself would chmod (not a container/managed deployment). An existing
/// `HERMES_HOME` is left untouched — it may be a NixOS-managed `0750`, a
/// UID-mapped Docker volume, or otherwise deliberately group-accessible, and
/// revoking that would break other Hermes users/processes.
pub(crate) fn ensure_hermes_home_secure(home: &Path) -> Result<(), AcpError> {
    #[cfg(unix)]
    let preexisting = home.exists();
    fs::create_dir_all(home)
        .map_err(|e| AcpError::protocol(format!("create hermes directory failed: {e}")))?;
    #[cfg(unix)]
    if !preexisting && !hermes_skip_chmod() {
        use std::os::unix::fs::PermissionsExt;
        let mode = parse_hermes_home_mode(std::env::var("HERMES_HOME_MODE").ok().as_deref());
        // Best-effort: a chmod hiccup must not block saving the config.
        let _ = fs::set_permissions(home, fs::Permissions::from_mode(mode));
    }
    Ok(())
}

/// Write a Hermes secret file (`.env` / `config.yaml`).
///
/// A brand-new secret — a path whose resolved target does not exist yet, whether
/// `path` itself is absent or a symlink to a missing target — is created
/// owner-only (`0600` on Unix) so it is never world-readable under the process
/// umask, the one real exposure for a first-time iyw-claw-driven setup. An EXISTING
/// target is written through in place, which preserves everything that identifies
/// it: its inode, mode, owner/group, POSIX ACL and xattrs, and any symlink (a
/// dotfile-manager or secret-manager `~/.hermes/.env` keeps pointing at its real
/// target). This deliberately favors preserving a managed/linked layout over an
/// atomic temp+rename replace — a rename would drop the symlink and the inode's
/// owner/ACL/xattrs, and on Windows would swap the file's security descriptor for
/// the parent directory's. It matches Hermes' own model (config.py `_secure_dir`
/// is a Windows no-op; file chmod is Unix-only) and the prior baseline. A crash
/// during the brief write window is recoverable by re-saving. `label` names the
/// file for error messages.
pub(crate) fn write_hermes_secret_file(
    path: &Path,
    contents: &str,
    label: &str,
) -> Result<(), AcpError> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        // `metadata` FOLLOWS symlinks, so this is true when the resolved target
        // does not exist yet — a genuinely fresh path OR a symlink whose target
        // is missing (e.g. `~/.hermes/.env -> /vault/hermes.env`). Creating with
        // `O_CREAT` likewise follows the symlink, so the new secret lands at the
        // real target with owner-only `0600` instead of the umask default
        // (`0644`). An existing resolved target is written through in place below.
        if fs::metadata(path).is_err() {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(path)
                .map_err(|e| AcpError::protocol(format!("create hermes {label} failed: {e}")))?;
            return file
                .write_all(contents.as_bytes())
                .map_err(|e| AcpError::protocol(format!("write hermes {label} failed: {e}")));
        }
    }
    // Existing target (or non-Unix): write through in place, preserving the
    // target's identity (inode, owner/group, ACL, xattrs, and any symlink).
    fs::write(path, contents)
        .map_err(|e| AcpError::protocol(format!("write hermes {label} failed: {e}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Repair an accidentally WORLD-accessible secret (e.g. a `0644` left by an
        // older iyw-claw build or by the pre-fix dangling-symlink path) back to
        // owner-only `0600`: a world-readable API key is a leak, and tightening it
        // to `0640` would still expose it to a broad group like `staff`. A file
        // with no "other" bits — including a deliberately group-shared managed
        // `0640` — is left untouched, and the container/managed chmod opt-out is
        // honored. Best-effort: never fail the save on a chmod hiccup.
        if !hermes_skip_chmod() {
            if let Ok(meta) = fs::metadata(path) {
                if meta.permissions().mode() & 0o007 != 0 {
                    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
                }
            }
        }
    }
    Ok(())
}

/// Write a Hermes config update to `~/.hermes/.env` (the active provider's API
/// key) and `~/.hermes/config.yaml` (the `model:` section, or a verbatim raw
/// document in advanced mode).
pub(crate) fn acp_update_hermes_config_core(
    update: HermesConfigUpdate,
    emitter: &EventEmitter,
) -> Result<(), AcpError> {
    let _paths = require_private_agent_storage_for_write()?;
    let HermesConfigUpdate {
        provider,
        api_key,
        model,
        base_url,
        raw_config_yaml,
    } = update;

    let home = hermes_home_dir();
    ensure_hermes_home_secure(&home)?;

    // Build + validate everything BEFORE any write, so an invalid document or a
    // crafted key never half-applies (the secret in particular).
    let config_path = hermes_config_yaml_path();
    let existing = if raw_config_yaml.is_none() {
        fs::read_to_string(&config_path).ok()
    } else {
        None
    };
    let model_trimmed = model.as_deref().map(str::trim).unwrap_or_default();
    let (config_yaml, env_updates) = plan_hermes_write(
        &provider,
        api_key.as_deref(),
        model_trimmed,
        base_url.as_deref(),
        raw_config_yaml.as_deref(),
        existing.as_deref(),
    )?;

    // Write config.yaml first, then `.env` — a config-write failure must never
    // leave the stored credential changed. Both are owner-only (they can carry
    // secrets: the `.env` key, and a raw config.yaml in advanced mode).
    write_hermes_secret_file(&config_path, &config_yaml, "config.yaml")?;
    if !env_updates.is_empty() {
        let env_path = hermes_env_path();
        let existing_env = fs::read_to_string(&env_path).unwrap_or_default();
        let updates: Vec<(&str, &str)> =
            env_updates.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let patched = patch_env_text(&existing_env, &updates);
        write_hermes_secret_file(&env_path, &patched, ".env")?;
    }

    emit_acp_agents_updated(emitter, "config_updated", Some(AgentType::Hermes));
    Ok(())
}

/// The result of planning a Hermes save: the `config.yaml` content to write and
/// the ordered list of `.env` `(var name, value)` updates to apply (empty when
/// nothing in `.env` changes).
type HermesWritePlan = (String, Vec<(&'static str, String)>);

/// Pure decision logic for a Hermes config save: compute the config.yaml content
/// to write and the `.env` `(key_var, value)` updates. Validation happens here
/// (no I/O) so a bad request fails before anything is written.
///
/// Raw mode is enforced server-side to never touch `.env` (the API contract is
/// not left to the caller's payload). OAuth/AWS providers carry no key var, so
/// they never produce a key update. Keyed providers update their API key (a
/// blank key leaves the stored secret untouched); providers with a base-URL env
/// var also mirror the structured endpoint URL there. Embedded newlines in the
/// key or base URL are rejected.
fn plan_hermes_write(
    provider: &str,
    api_key: Option<&str>,
    model: &str,
    base_url: Option<&str>,
    raw_config_yaml: Option<&str>,
    existing_config: Option<&str>,
) -> Result<HermesWritePlan, AcpError> {
    // The provider the existing config.yaml was on (None for a first save / raw
    // mode). Drives base-URL preservation and stale-`.env`-credential cleanup.
    let previous_provider = existing_hermes_model_provider(existing_config);

    let config_yaml = if let Some(raw) = raw_config_yaml {
        serde_yaml::from_str::<serde_yaml::Value>(raw)
            .map_err(|e| AcpError::protocol(format!("invalid hermes config.yaml: {e}")))?;
        raw.to_string()
    } else {
        // Structured mode only handles providers in the curated table. The
        // `custom` provider IS handled (its key/endpoint live inline in
        // config.yaml — see `hermes_inlines_api_key`), but unknown ids (the
        // legacy `openai` pseudo-provider, user-defined `custom:` slugs, or
        // anything outside the table) have no credential layout iyw-claw can map —
        // reject them and steer the user to the raw config.yaml editor, which
        // stays the escape hatch.
        let meta = hermes_provider(provider).ok_or_else(|| {
            AcpError::protocol(format!(
                "unknown hermes provider '{provider}'; edit ~/.hermes/config.yaml directly"
            ))
        })?;
        // Decide what happens to `model.base_url`:
        // - User-editable endpoint (openai-api/lmstudio/azure-foundry) → write the
        //   field's value, or clear it when blank.
        // - Endpoint not exposed in the panel, and the provider is UNCHANGED →
        //   preserve an out-of-band base URL (proxy/Azure) the user set elsewhere.
        // - Endpoint not exposed, but the provider just CHANGED → clear the stale
        //   base URL left over from the previous provider (it must not carry over).
        let base = if meta.needs_base_url {
            BaseUrlWrite::Set(base_url.unwrap_or(""))
        } else if previous_provider.as_deref() == Some(provider) {
            BaseUrlWrite::Preserve
        } else {
            BaseUrlWrite::Set("")
        };
        // Inline key — `custom` only. The key rides in `model.api_key`; every
        // other provider gets `Clear` so a stale inline key from a previous
        // `custom` endpoint never bleeds into the new provider. A blank inline
        // key drops the field (keyless local server).
        let inline_api_key = if hermes_inlines_api_key(provider) {
            let key = api_key.map(str::trim).unwrap_or_default();
            if key.contains(['\n', '\r']) {
                return Err(AcpError::protocol(
                    "hermes api key must not contain newlines",
                ));
            }
            // Scrub a stale `api_mode` only when switching TO custom from a
            // different provider; a custom→custom re-save preserves it.
            let scrub_mode = previous_provider.as_deref() != Some(provider);
            InlineApiKeyWrite::Set { key, scrub_mode }
        } else {
            InlineApiKeyWrite::Clear
        };
        merge_hermes_model_config(existing_config, provider, model, base, inline_api_key)?
    };

    // Raw mode edits config.yaml only; never `.env`.
    let mut env_updates: Vec<(&'static str, String)> = Vec::new();
    if raw_config_yaml.is_none() {
        let meta = hermes_provider(provider);
        // API key — keyed providers only. A blank key leaves the stored secret
        // untouched (so switching providers can't wipe it).
        if let Some(meta) = meta.filter(|p| !p.key_env_var.is_empty()) {
            if let Some(key) = api_key.map(str::trim).filter(|k| !k.is_empty()) {
                if key.contains(['\n', '\r']) {
                    return Err(AcpError::protocol(
                        "hermes api key must not contain newlines",
                    ));
                }
                env_updates.push((meta.key_env_var, key.to_string()));
            }
        }
        // Endpoint URL — mirror the structured base URL into the provider's
        // base-URL env var so `.env` and config.yaml `model.base_url` agree
        // under either of Hermes' resolution paths. An empty value clears a
        // stale override.
        if let Some(meta) = meta.filter(|p| p.needs_base_url && !p.base_url_env_var.is_empty()) {
            let base = base_url.map(str::trim).unwrap_or_default();
            if base.contains(['\n', '\r']) {
                return Err(AcpError::protocol(
                    "hermes base url must not contain newlines",
                ));
            }
            env_updates.push((meta.base_url_env_var, base.to_string()));
        }
        // Neutralize only vars that can actually BLEED INTO the selected
        // provider's runtime path — never blanket-wipe the previous provider's
        // own credential (a valid ANTHROPIC_API_KEY must survive an anthropic→zai
        // switch; zai won't read it). The one documented cross-provider fallback
        // in hermes 0.16.0: openrouter (being OpenAI-API compatible) falls back to
        // OPENAI_API_KEY and treats OPENAI_BASE_URL as an endpoint override. So
        // when saving openrouter, write an explicit empty `OPENAI_API_KEY=` /
        // `OPENAI_BASE_URL=` — appended even if absent from `.env`, since under
        // Hermes' dotenv override loading only that masks a stale value inherited
        // from the process environment.
        if provider == "openrouter" {
            for var in ["OPENAI_API_KEY", "OPENAI_BASE_URL"] {
                if !env_updates.iter().any(|(k, _)| *k == var) {
                    env_updates.push((var, String::new()));
                }
            }
        }
    }

    Ok((config_yaml, env_updates))
}

/// Compare two base URLs for equality, ignoring a trailing slash — every Hermes
/// endpoint rstrips `/`, so `https://x/v1` and `https://x/v1/` are the same host
/// and must not churn a managed/symlinked `.env` on every launch. Used for the
/// reconcile decision ONLY; the value written to `.env` stays verbatim.
fn base_url_eq(a: &str, b: &str) -> bool {
    a.trim_end_matches('/') == b.trim_end_matches('/')
}

/// Decide how to reconcile the active provider's base-URL `.env` variable with
/// `config.yaml`'s `model.base_url`, so Hermes' auxiliary credential path —
/// `auth.py::resolve_api_key_provider_credentials`, which reads the endpoint
/// ONLY from the provider's `<X>_BASE_URL` env var — resolves the SAME endpoint
/// as the main loop, which reads `config.yaml model.base_url`. Hermes' own
/// `hermes model`/`hermes setup` writes `model.base_url` but never the `.env`
/// var, so auxiliary tasks (title generation, compression, …) silently fall
/// back to the provider's registry-default host and 401 against the wrong
/// endpoint. The settings panel already mirrors both on save; this covers
/// configs authored outside iyw-claw.
///
/// Scope is the single ACTIVE provider's own base-URL var, never another
/// provider's. Returns `Some((env_var, value))` to write — `value` is the
/// verbatim `model.base_url`, or `""` to clear a stale override that would
/// otherwise bleed into the auxiliary path — or `None` for a no-op. Unknown /
/// legacy providers and ones with no base-URL var (OAuth, Bedrock,
/// kimi-coding-cn) map to `None`.
fn plan_hermes_base_url_reconcile(
    provider: &str,
    yaml_base_url: Option<&str>,
    current_env_value: Option<&str>,
) -> Option<(&'static str, String)> {
    let meta = hermes_provider(provider).filter(|p| !p.base_url_env_var.is_empty())?;
    let desired = yaml_base_url.map(str::trim).unwrap_or_default();
    // A base URL carrying an embedded newline would let `patch_env_text` emit an
    // extra `.env` line — injecting ANOTHER provider's var and breaking the
    // single-active-var invariant. config.yaml is the user's own file, but skip
    // rather than corrupt `.env` (the panel's `plan_hermes_write` rejects
    // newlines the same way). A blank-after-trim value still falls through to the
    // empty/clear path below.
    if desired.contains(['\n', '\r']) {
        return None;
    }
    let current = current_env_value.unwrap_or_default();
    if desired.is_empty() {
        // No endpoint in config.yaml. Clear a stale, non-empty override so it
        // can't shadow the registry default in the auxiliary path; leave an
        // absent/empty var alone (don't append a redundant `KEY=`).
        if current.is_empty() {
            return None;
        }
        return Some((meta.base_url_env_var, String::new()));
    }
    if base_url_eq(desired, current) {
        return None;
    }
    Some((meta.base_url_env_var, desired.to_string()))
}

/// Reconcile `~/.hermes/.env`'s base-URL variable with `config.yaml`'s
/// `model.base_url` for the active provider, right before launching Hermes, so
/// auxiliary tasks and the main loop hit the same endpoint (see
/// `plan_hermes_base_url_reconcile`). Best-effort: a failure here must never
/// block a launch, so the result is logged and swallowed.
///
/// Note: for `openai-api` this sets `OPENAI_BASE_URL`, which makes Hermes log a
/// one-time "OPENAI_BASE_URL is set but provider is not custom" warning. That is
/// a false positive — `OPENAI_BASE_URL` IS the correct base-URL var for
/// `openai-api` — so do not "fix" it by dropping the var.
/// Resolve the Hermes home a launch with `runtime_env` will actually use, so
/// reconcile patches the same `.env` the launched process reads.
///
/// When the agent's `env_json` sets `HERMES_HOME` it lands in `runtime_env`,
/// which `merge_agent_env` gives highest precedence — so it *replaces* the
/// parent's value in the child. We must resolve that override exactly as the
/// launched Hermes' own `get_hermes_home` does: trim it; a non-empty value is
/// used VERBATIM (`Path(val)` — Hermes does NOT expand `~`); a blank value falls
/// back to the default `~/.hermes` (it does NOT re-inherit the parent). With no
/// override the child inherits the parent env, so defer to `hermes_home_dir()`
/// (iyw-claw's existing resolution, shared with the settings panel).
fn hermes_home_for_launch(runtime_env: &BTreeMap<String, String>) -> PathBuf {
    match runtime_env.get("HERMES_HOME") {
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                home_dir_or_default().join(".hermes")
            } else {
                PathBuf::from(trimmed)
            }
        }
        None => hermes_home_dir(),
    }
}

pub(crate) fn reconcile_hermes_runtime_env(runtime_env: &BTreeMap<String, String>) {
    if let Err(err) = reconcile_hermes_runtime_env_in(&hermes_home_for_launch(runtime_env)) {
        tracing::warn!("[ACP][Hermes] base_url reconcile skipped: {err}");
    }
}

/// Inner reconcile keyed on an explicit home dir (so tests drive a tempdir
/// without mutating `HERMES_HOME`). No-ops when `config.yaml` is absent — it
/// must never create `~/.hermes`; a config written later goes through the panel
/// (which already mirrors the base URL) or a subsequent launch.
fn reconcile_hermes_runtime_env_in(home: &Path) -> Result<(), AcpError> {
    let config_path = home.join("config.yaml");
    let Ok(raw_yaml) = fs::read_to_string(&config_path) else {
        return Ok(());
    };
    let value: serde_yaml::Value = serde_yaml::from_str(&raw_yaml)
        .map_err(|e| AcpError::protocol(format!("parse hermes config.yaml: {e}")))?;
    let Some(model_section) = value.get("model") else {
        return Ok(());
    };
    let Some(provider) = yaml_str(model_section, "provider") else {
        return Ok(());
    };
    let yaml_base_url = yaml_str(model_section, "base_url");

    let env_path = home.join(".env");
    // Only a MISSING `.env` is an empty baseline. An existing-but-unreadable file
    // (non-UTF-8, permission-denied, …) must abort the reconcile — patching from
    // an empty baseline would rewrite `.env` with just the base-URL line and drop
    // the user's API keys and comments. A dangling symlink reads as NotFound and
    // is correctly created fresh (0600) by `write_hermes_secret_file`.
    let existing_env = match fs::read_to_string(&env_path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(AcpError::protocol(format!("read hermes .env: {e}"))),
    };
    let env_map = parse_env_file(&existing_env);
    let current = hermes_provider(&provider)
        .filter(|p| !p.base_url_env_var.is_empty())
        .and_then(|p| env_map.get(p.base_url_env_var))
        .map(String::as_str);

    let Some((var, val)) =
        plan_hermes_base_url_reconcile(&provider, yaml_base_url.as_deref(), current)
    else {
        return Ok(());
    };

    let patched = patch_env_text(&existing_env, &[(var, val.as_str())]);
    write_hermes_secret_file(&env_path, &patched, ".env")
}

fn agent_local_config_path(agent_type: AgentType) -> Option<PathBuf> {
    match agent_type {
        AgentType::ClaudeCode => Some(crate::parsers::profile_paths::claude_settings_path()),
        AgentType::Gemini => Some(crate::parsers::profile_paths::gemini_settings_path()),
        AgentType::OpenCode => Some(resolve_opencode_config_path()),
        AgentType::Cline => Some(cline_global_state_path()),
        // Kimi Code's native config is `~/.kimi-code/config.toml`. Exposing the
        // path lights up "open config file" + staleness tracking; the actual
        // load/persist are special-cased below (TOML, not the generic JSON path).
        AgentType::KimiCode => Some(kimi_code_config_toml_path()),
        _ => None,
    }
}

pub(crate) fn load_agent_local_config_json(agent_type: AgentType) -> Option<String> {
    if agent_type == AgentType::Codex {
        return load_codex_local_config_json();
    }
    if agent_type == AgentType::Cline {
        return load_cline_local_config_json();
    }
    if agent_type == AgentType::KimiCode {
        return load_kimi_code_config_json();
    }

    let path = agent_local_config_path(agent_type)?;
    if !path.exists() {
        return None;
    }

    let raw = fs::read_to_string(path).ok()?;
    let parsed = serde_json::from_str::<serde_json::Value>(&raw).ok()?;
    if !parsed.is_object() {
        return None;
    }
    serde_json::to_string_pretty(&parsed).ok()
}

/// Read the raw Grok native config used by the settings panel and ACP launch.
/// Grok stores this file as TOML rather than JSON, so it needs its own
/// fingerprint input instead of going through `load_agent_local_config_json`.
fn load_grok_config_toml_raw() -> Option<String> {
    fs::read_to_string(crate::parsers::grok::resolve_grok_home_dir().join("config.toml")).ok()
}

fn merge_json_values(base: &mut serde_json::Value, patch: &serde_json::Value) {
    if let (Some(base_obj), Some(patch_obj)) = (base.as_object_mut(), patch.as_object()) {
        for (key, patch_value) in patch_obj {
            if patch_value.is_null() {
                // null in patch means "remove this key"
                base_obj.remove(key);
                continue;
            }
            match base_obj.get_mut(key) {
                Some(base_value) => merge_json_values(base_value, patch_value),
                None => {
                    base_obj.insert(key.clone(), patch_value.clone());
                }
            }
        }
        return;
    }

    *base = patch.clone();
}

fn persist_agent_local_config_json(
    agent_type: AgentType,
    config_patch_json: Option<&str>,
) -> Result<(), AcpError> {
    if agent_type == AgentType::Codex {
        return persist_codex_local_config(config_patch_json);
    }
    if agent_type == AgentType::Cline {
        return persist_cline_local_config(config_patch_json);
    }
    if agent_type == AgentType::KimiCode {
        // Kimi's config.toml is written exclusively through the dedicated
        // `acp_update_kimi_code_config` command (structured/raw modes). The
        // generic JSON-merge persist must never touch it (it would write JSON
        // into a TOML file).
        return Ok(());
    }

    let Some(path) = agent_local_config_path(agent_type) else {
        return Ok(());
    };
    let Some(raw_patch) = config_patch_json else {
        return Ok(());
    };

    let patch = serde_json::from_str::<serde_json::Value>(raw_patch)
        .map_err(|e| AcpError::protocol(format!("invalid config_json: {e}")))?;
    if !patch.is_object() {
        return Err(AcpError::protocol(
            "invalid config_json: root must be a JSON object",
        ));
    }

    if agent_type == AgentType::OpenCode {
        let serialized = serde_json::to_string_pretty(&patch)
            .map_err(|e| AcpError::protocol(format!("serialize config_json failed: {e}")))?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| AcpError::protocol(format!("create config directory failed: {e}")))?;
        }
        fs::write(&path, format!("{serialized}\n"))
            .map_err(|e| AcpError::protocol(format!("write local config failed: {e}")))?;
        return Ok(());
    }

    let mut base = if path.exists() {
        match fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        {
            Some(existing) if existing.is_object() => existing,
            _ => serde_json::json!({}),
        }
    } else {
        serde_json::json!({})
    };

    merge_json_values(&mut base, &patch);
    let serialized = serde_json::to_string_pretty(&base)
        .map_err(|e| AcpError::protocol(format!("serialize config_json failed: {e}")))?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AcpError::protocol(format!("create config directory failed: {e}")))?;
    }
    fs::write(&path, format!("{serialized}\n"))
        .map_err(|e| AcpError::protocol(format!("write local config failed: {e}")))?;

    Ok(())
}

pub(crate) fn skill_storage_spec(agent_type: AgentType) -> Option<SkillStorageSpec> {
    match agent_type {
        AgentType::ClaudeCode => Some(SkillStorageSpec {
            kind: SkillStorageKind::SkillDirectoryOnly,
            global_dirs: vec![crate::parsers::claude::resolve_claude_config_dir().join("skills")],
            project_rel_dirs: vec![".claude/skills"],
        }),
        AgentType::Codex => Some(SkillStorageSpec {
            kind: SkillStorageKind::SkillDirectoryOrMarkdownFile,
            global_dirs: with_user_shared_agent_skills(vec![
                codex_home_dir().join("skills"),
                // `.system` is where Codex CLI stores its own bundled
                // skills (imagegen, skill-creator, etc.). The directory
                // name is a Codex convention, not a stable contract —
                // if Codex renames it we'll silently stop listing them.
                // `is_read_only_skill_path` mirrors this path to prevent
                // edit/delete from clobbering CLI assets.
                codex_home_dir().join("skills").join(".system"),
            ]),
            project_rel_dirs: vec![".codex/skills", ".agents/skills"],
        }),
        AgentType::OpenCode => Some(SkillStorageSpec {
            kind: SkillStorageKind::SkillDirectoryOnly,
            // OpenCode is a "universal" agent for the `skills` CLI (its
            // skillsDir is `.agents/skills`): a global `skills add` writes the
            // real skill into the shared `~/.agents/skills` store and does NOT
            // create a `~/.config/opencode/skills` symlink. OpenCode reads both
            // locations, so probe both — otherwise CLI-installed skills are
            // invisible here and in Settings → Skills.
            global_dirs: with_user_shared_agent_skills(vec![
                crate::parsers::profile_paths::opencode_config_dir().join("skills"),
            ]),
            project_rel_dirs: vec![".agents/skills", ".opencode/skills"],
        }),
        AgentType::Gemini => Some(SkillStorageSpec {
            kind: SkillStorageKind::SkillDirectoryOnly,
            global_dirs: with_user_shared_agent_skills(vec![
                crate::parsers::gemini::resolve_gemini_base_dir().join("skills"),
            ]),
            project_rel_dirs: vec![".gemini/skills", ".agents/skills"],
        }),
        AgentType::OpenClaw => Some(SkillStorageSpec {
            kind: SkillStorageKind::SkillDirectoryOnly,
            global_dirs: vec![crate::parsers::profile_paths::openclaw_state_dir().join("skills")],
            project_rel_dirs: vec!["skills"],
        }),
        AgentType::Cline => Some(SkillStorageSpec {
            kind: SkillStorageKind::SkillDirectoryOnly,
            global_dirs: with_user_shared_agent_skills(vec![
                crate::parsers::profile_paths::cline_skills_dir(),
            ]),
            project_rel_dirs: vec![
                ".agents/skills",
                ".cline/skills",
                ".clinerules/skills",
                ".claude/skills",
            ],
        }),
        AgentType::Hermes => Some(SkillStorageSpec {
            kind: SkillStorageKind::SkillDirectoryOnly,
            global_dirs: vec![hermes_home_dir().join("skills")],
            project_rel_dirs: vec![],
        }),
        // CodeBuddy is a Claude Code derivative: same `skills` directory
        // layout, under `~/.codebuddy` instead of `~/.claude`.
        AgentType::CodeBuddy => Some(SkillStorageSpec {
            kind: SkillStorageKind::SkillDirectoryOnly,
            global_dirs: vec![
                crate::parsers::codebuddy::resolve_codebuddy_config_dir().join("skills")
            ],
            project_rel_dirs: vec![".codebuddy/skills"],
        }),
        // Kimi Code reads skills from `<KIMI_CODE_HOME>/skills/` (default
        // `~/.kimi-code/skills/`) and project-local `<root>/.kimi-code/skills/`.
        AgentType::KimiCode => Some(SkillStorageSpec {
            kind: SkillStorageKind::SkillDirectoryOnly,
            global_dirs: vec![
                crate::parsers::kimi_code::resolve_kimi_code_home_dir().join("skills")
            ],
            project_rel_dirs: vec![".kimi-code/skills"],
        }),
        // pi auto-loads skills from `~/.pi/agent/skills` and the shared
        // `~/.agents/skills` store (both global), plus project-local
        // `.pi/skills` / `.agents/skills` once the workspace is trusted (iyw-claw
        // seeds that trust on connect). `~/.pi/agent/skills` additionally
        // accepts standalone `.md` files, so this mirrors Codex's spec shape.
        // The pi-native dir comes first so toggling pi links into its own dir
        // without cross-agent side effects on the shared store.
        AgentType::Pi => Some(SkillStorageSpec {
            kind: SkillStorageKind::SkillDirectoryOrMarkdownFile,
            global_dirs: with_user_shared_agent_skills(vec![pi_agent_dir().join("skills")]),
            project_rel_dirs: vec![".pi/skills", ".agents/skills"],
        }),
        AgentType::Grok => Some(SkillStorageSpec {
            kind: SkillStorageKind::SkillDirectoryOnly,
            global_dirs: vec![crate::parsers::grok::resolve_grok_home_dir().join("skills")],
            project_rel_dirs: vec![".grok/skills"],
        }),
    }
}

fn scope_rank(scope: AgentSkillScope) -> u8 {
    match scope {
        AgentSkillScope::Global => 0,
        AgentSkillScope::Project => 1,
    }
}

pub(crate) fn validate_skill_id(raw: &str) -> Result<String, AcpError> {
    let id = raw.trim();
    if id.is_empty() {
        return Err(AcpError::protocol("skill id cannot be empty"));
    }
    if id.starts_with('.') {
        return Err(AcpError::protocol("skill id cannot start with a dot (.)"));
    }
    if id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(AcpError::protocol(
            "skill id cannot contain path separators",
        ));
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(AcpError::protocol(
            "skill id can only include letters, numbers, '-', '_' and '.'",
        ));
    }
    Ok(id.to_string())
}

pub(crate) fn scoped_skill_dirs(
    agent_type: AgentType,
    scope: AgentSkillScope,
    workspace_path: Option<&str>,
) -> Result<Vec<PathBuf>, AcpError> {
    let spec = skill_storage_spec(agent_type).ok_or_else(|| {
        AcpError::protocol(format!(
            "{agent_type} skills are not supported in Settings yet"
        ))
    })?;

    match scope {
        AgentSkillScope::Global => Ok(spec.global_dirs),
        AgentSkillScope::Project => {
            let workspace = workspace_path
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .ok_or_else(|| {
                    AcpError::protocol("workspace_path is required for project scoped skills")
                })?;
            Ok(spec
                .project_rel_dirs
                .iter()
                .map(|relative| PathBuf::from(workspace).join(relative))
                .collect())
        }
    }
}

pub(crate) fn preferred_scope_skill_dir(
    agent_type: AgentType,
    scope: AgentSkillScope,
    workspace_path: Option<&str>,
) -> Result<PathBuf, AcpError> {
    let dirs = scoped_skill_dirs(agent_type, scope, workspace_path)?;
    dirs.into_iter()
        .next()
        .ok_or_else(|| AcpError::protocol("no skill directory resolved for this agent"))
}

fn is_markdown_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("md"))
        .unwrap_or(false)
}

fn skill_name_from_id(id: &str) -> String {
    id.to_string()
}

/// Best-effort extraction of a one-line skill description from a markdown
/// file's YAML frontmatter. Prefers `short-description` (commonly nested under
/// a `metadata:` block) and falls back to a top-level `description`. Only the
/// first 4 KiB is read; frontmatter always fits, and skill bodies can be large.
fn read_skill_description(content_path: &Path) -> Option<String> {
    use std::io::Read;
    let mut file = fs::File::open(content_path).ok()?;
    let mut buf = [0u8; 4096];
    let n = file.read(&mut buf).ok()?;
    let head = std::str::from_utf8(&buf[..n]).ok()?;

    let mut lines = head.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }

    let mut short: Option<String> = None;
    let mut long: Option<String> = None;
    for line in lines {
        let trimmed_end = line.trim_end();
        if trimmed_end == "---" || trimmed_end == "..." {
            break;
        }
        let is_top_level = !line.starts_with(|c: char| c.is_whitespace());
        let stripped = line.trim();

        // `short-description` is allowed at any indent so it resolves when
        // nested under `metadata:` (Codex's `.system` skills follow this).
        if short.is_none() {
            if let Some(rest) = stripped.strip_prefix("short-description:") {
                if let Some(val) = parse_frontmatter_scalar(rest) {
                    short = Some(val);
                    break;
                }
            }
        }
        // `description` is only honored at the top level to avoid colliding
        // with unrelated nested `description:` keys.
        if is_top_level && long.is_none() {
            if let Some(rest) = line.strip_prefix("description:") {
                if let Some(val) = parse_frontmatter_scalar(rest) {
                    long = Some(val);
                }
            }
        }
    }
    short.or(long)
}

/// Read a single-line YAML scalar (with optional matching quotes). Returns
/// `None` for empty values or block-scalar markers (`|` / `>`) we can't span.
fn parse_frontmatter_scalar(rest: &str) -> Option<String> {
    let val = rest.trim();
    if val.starts_with('|') || val.starts_with('>') {
        return None;
    }
    let unquoted = val
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| val.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        .unwrap_or(val)
        .trim();
    if unquoted.is_empty() {
        None
    } else {
        Some(unquoted.to_string())
    }
}

fn build_skill_item(
    id: String,
    scope: AgentSkillScope,
    layout: AgentSkillLayout,
    path: PathBuf,
    enabled: bool,
) -> AgentSkillItem {
    let description = read_skill_description(&skill_content_path(layout, &path));
    let market = read_market_skill_marker(&path);
    let market_managed = is_market_skill_source(&path);
    let official = is_official_skill_source(&path);
    AgentSkillItem {
        name: skill_name_from_id(&id),
        id,
        scope,
        layout,
        path: path.to_string_lossy().to_string(),
        description,
        enabled,
        copy_mode: false,
        read_only: false,
        official,
        market_managed,
        market_skill_id: market.as_ref().map(|value| value.skill_id.to_string()),
        installed_version: market.as_ref().map(|value| value.installed_version.clone()),
        market_visibility: market.as_ref().map(|value| value.visibility.clone()),
        publisher_type: market.as_ref().map(|value| value.publisher_type.clone()),
        market_content_sha256: market.map(|value| value.content_sha256),
    }
}

const DISABLED_SKILLS_DIR: &str = ".iyw-claw-disabled";
const CONFLICTED_SKILLS_DIR: &str = ".iyw-claw-conflicts";
const SHARED_SKILL_COPY_MARKER: &str = ".iyw-claw-managed-copy.json";
const SHARED_SKILL_PUBLISH_STATE_MARKER: &str = ".iyw-claw-publish-state.json";
/// Written into a shared skill's source directory when the skill is installed
/// from the official market. Marks the content as market-managed so later
/// saves through the generic skill API are refused, while enable/disable and
/// uninstall keep working. Deleting the skill removes the whole source
/// directory, so the marker needs no separate cleanup.
const OFFICIAL_SKILL_MARKER: &str = ".iyw-claw-official-skill.json";
const MARKET_SKILL_MARKER: &str = ".iyw-claw-market-skill.json";

fn disabled_skills_dir(dir: &Path) -> PathBuf {
    dir.join(DISABLED_SKILLS_DIR)
}

fn shared_skills_dir() -> PathBuf {
    central_experts_dir()
}

fn shared_skill_path(skill_id: &str) -> PathBuf {
    shared_skills_dir().join(skill_id)
}

fn is_reserved_shared_skill_id(skill_id: &str) -> bool {
    is_bundled_expert_id(skill_id)
        || crate::commands::office_tools::is_officecli_skill_id(skill_id)
        || crate::commands::internet_tools::is_internet_tool_skill_id(skill_id)
}

fn ensure_shared_skill_writable(skill_id: &str) -> Result<(), AcpError> {
    if is_reserved_shared_skill_id(skill_id) {
        return Err(AcpError::protocol(format!(
            "skill '{skill_id}' is managed by iyw-claw and cannot be modified here"
        )));
    }
    Ok(())
}

fn is_official_skill_source(source: &Path) -> bool {
    source.join(OFFICIAL_SKILL_MARKER).is_file()
        || read_market_skill_marker(source)
            .is_some_and(|marker| marker.publisher_type == "official")
}

fn is_market_skill_source(source: &Path) -> bool {
    source.join(MARKET_SKILL_MARKER).is_file()
}

fn is_official_shared_skill(skill_id: &str) -> bool {
    is_official_skill_source(&shared_skill_path(skill_id))
}

/// Refuse content writes to market-managed skills. The official channel
/// itself passes `official: true` so it can (re)install over its own marker.
/// Enable/disable and delete deliberately bypass this guard.
fn ensure_shared_skill_not_official(skill_id: &str) -> Result<(), AcpError> {
    let source = shared_skill_path(skill_id);
    if is_official_shared_skill(skill_id) || is_market_skill_source(&source) {
        return Err(AcpError::protocol(format!(
            "skill '{skill_id}' is managed by the Skill market and cannot be modified"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OfficialSkillMarker {
    skill_id: String,
    source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MarketSkillMarker {
    pub schema_version: u8,
    pub source: String,
    pub skill_id: i64,
    pub slug: String,
    pub installed_version: String,
    pub content_sha256: String,
    pub object_sha256: String,
    pub visibility: String,
    pub publisher_type: String,
    #[serde(default = "default_market_package_type")]
    pub package_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_types: Option<Vec<AgentType>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub target_references: BTreeMap<i64, Vec<AgentType>>,
    #[serde(default)]
    pub dependencies: Vec<MarketSkillDependencyMarker>,
    pub installed_at: String,
}

fn default_market_package_type() -> String {
    "skill".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MarketSkillDependencyMarker {
    pub skill_id: i64,
    pub slug: String,
    pub version: String,
}

pub(crate) struct MarketSkillInstall {
    pub marker: MarketSkillMarker,
    pub package: crate::acp::skill_package::ValidatedSkillPackage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SharedSkillPublishStateMarker {
    skill_id: String,
    enabled: bool,
}

fn write_official_skill_marker(source: &Path, skill_id: &str) -> Result<(), AcpError> {
    let marker = OfficialSkillMarker {
        skill_id: skill_id.to_string(),
        source: "official_market".to_string(),
    };
    let serialized = serde_json::to_string_pretty(&marker)
        .map_err(|e| AcpError::protocol(format!("failed to serialize official marker: {e}")))?;
    fs::write(source.join(OFFICIAL_SKILL_MARKER), serialized)
        .map_err(|e| AcpError::protocol(format!("failed to write official marker: {e}")))
}

fn read_market_skill_marker(source: &Path) -> Option<MarketSkillMarker> {
    let value = fs::read_to_string(source.join(MARKET_SKILL_MARKER)).ok()?;
    serde_json::from_str(&value).ok()
}

pub(crate) fn installed_market_skill_versions() -> HashMap<i64, String> {
    let Ok(entries) = fs::read_dir(shared_skills_dir()) else {
        return HashMap::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| read_market_skill_marker(&entry.path()))
        .map(|marker| (marker.skill_id, marker.installed_version))
        .collect()
}

pub(crate) fn installed_market_skill_targets(skill_id: i64) -> Vec<AgentType> {
    let Ok(entries) = fs::read_dir(shared_skills_dir()) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| read_market_skill_marker(&entry.path()))
        .find(|marker| marker.skill_id == skill_id)
        .map(|marker| market_reference_targets(&market_target_references(&marker)))
        .unwrap_or_default()
}

fn ensure_market_dependency_not_in_use(skill_id: &str) -> Result<(), AcpError> {
    let root = shared_skills_dir();
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(AcpError::protocol(format!(
                "failed to inspect market Skill dependencies: {error}"
            )))
        }
    };
    for entry in entries {
        let entry = entry.map_err(|error| {
            AcpError::protocol(format!(
                "failed to inspect market Skill dependencies: {error}"
            ))
        })?;
        let source = entry.path();
        let Some(marker) = read_market_skill_marker(&source) else {
            continue;
        };
        if marker.slug == skill_id
            || !marker
                .dependencies
                .iter()
                .any(|dependency| dependency.slug == skill_id)
        {
            continue;
        }
        if shared_skill_publish_enabled(&source, &marker.slug)? {
            return Err(AcpError::protocol(format!(
                "skill '{skill_id}' is required by enabled expert package '{}'",
                marker.slug
            )));
        }
    }
    Ok(())
}

fn read_official_skill_marker(source: &Path) -> Option<OfficialSkillMarker> {
    let value = fs::read_to_string(source.join(OFFICIAL_SKILL_MARKER)).ok()?;
    serde_json::from_str(&value).ok()
}

struct PreparedMarketSkillInstall {
    skill_id: String,
    source: PathBuf,
    files: Vec<(PathBuf, Vec<u8>)>,
    targets: Vec<AgentType>,
    previous: PreviousMarketSkillState,
}

#[derive(Clone)]
struct PreviousMarketSkillState {
    skill_id: String,
    existed: bool,
    enabled: bool,
    publications: Vec<(AgentType, AgentSkillSyncMode)>,
}

struct PendingMarketSkillInstall {
    prepared: PreparedMarketSkillInstall,
    swap: PendingSkillDirectorySwap,
}

pub(crate) fn install_market_skills(
    agent_types: &[AgentType],
    installs: Vec<MarketSkillInstall>,
) -> Result<Vec<AgentSkillItem>, AcpError> {
    let _paths = require_private_agent_storage_for_write()?;
    if agent_types.is_empty() {
        return Err(AcpError::protocol("Skill publication targets are empty"));
    }
    if installs.is_empty() {
        return Err(AcpError::protocol("market Skill install plan is empty"));
    }
    let root_skill_id = market_install_root_id(&installs)?;
    let retained_skill_ids = installs
        .iter()
        .map(|install| install.marker.skill_id)
        .collect::<BTreeSet<_>>();
    let _guard = shared_skill_mutation_guard();
    let prepared = prepare_market_skill_installs(installs)?;
    let pending = begin_market_skill_swaps(prepared)?;
    let installed = match publish_market_skill_installs(&pending) {
        Ok(installed) => installed,
        Err(error) => {
            rollback_market_skill_installs(pending, true);
            return Err(error);
        }
    };
    for value in pending {
        value.swap.commit();
    }
    if let Err(error) = release_market_root_references(root_skill_id, &retained_skill_ids) {
        tracing::warn!(
            root_skill_id,
            error = %error,
            "[skill-market] install committed but stale dependency cleanup failed"
        );
    }
    Ok(installed)
}

/// 卸载指定 market skill（按 `skill_id` 匹配本地 market marker）。
/// 若该 skill 仍被其他已启用的 expert 包依赖则拒绝卸载；未找到任何
/// 安装记录时视为已卸载（幂等）。返回移除的目录数。
pub(crate) fn uninstall_market_skill_by_id(skill_id: i64) -> Result<usize, AcpError> {
    let _guard = shared_skill_mutation_guard();
    uninstall_market_skill_by_id_locked(skill_id)
}

fn uninstall_market_skill_by_id_locked(skill_id: i64) -> Result<usize, AcpError> {
    let root = shared_skills_dir();
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => {
            return Err(AcpError::protocol(format!(
                "failed to inspect market Skill installs: {error}"
            )))
        }
    };
    for entry in entries {
        let entry = entry.map_err(|error| {
            AcpError::protocol(format!("failed to inspect market Skill installs: {error}"))
        })?;
        let source = entry.path();
        let Some(marker) = read_market_skill_marker(&source) else {
            continue;
        };
        if marker.skill_id != skill_id {
            continue;
        }
        if marker.target_references.is_empty() {
            ensure_market_dependency_not_in_use(&marker.slug)?;
        } else if !marker.target_references.contains_key(&skill_id) {
            return Err(AcpError::protocol(format!(
                "skill '{}' is installed as a dependency and cannot be uninstalled directly",
                marker.slug
            )));
        }
    }
    release_market_root_references(skill_id, &BTreeSet::new())
}

fn market_install_root_id(installs: &[MarketSkillInstall]) -> Result<i64, AcpError> {
    let root_ids = installs
        .iter()
        .flat_map(|install| install.marker.target_references.keys().copied())
        .collect::<BTreeSet<_>>();
    if root_ids.len() != 1 {
        return Err(AcpError::protocol(
            "market Skill install plan must reference exactly one root Skill",
        ));
    }
    Ok(*root_ids.iter().next().expect("validated root Skill ID"))
}

fn release_market_root_references(
    root_skill_id: i64,
    retained_skill_ids: &BTreeSet<i64>,
) -> Result<usize, AcpError> {
    let root = shared_skills_dir();
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => {
            return Err(AcpError::protocol(format!(
                "failed to inspect market Skill references: {error}"
            )))
        }
    };
    let mut stale_references = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            AcpError::protocol(format!(
                "failed to inspect market Skill references: {error}"
            ))
        })?;
        let Some(marker) = read_market_skill_marker(&entry.path()) else {
            continue;
        };
        let legacy_root = marker.target_references.is_empty() && marker.skill_id == root_skill_id;
        if !retained_skill_ids.contains(&marker.skill_id)
            && (legacy_root || marker.target_references.contains_key(&root_skill_id))
        {
            stale_references.push(marker.slug);
        }
    }
    let mut removed = 0usize;
    for skill_id in stale_references {
        if release_market_dependency_reference(&skill_id, root_skill_id)? {
            removed += 1;
        }
    }
    Ok(removed)
}

fn release_market_dependency_reference(
    skill_id: &str,
    root_skill_id: i64,
) -> Result<bool, AcpError> {
    let source = shared_skill_path(skill_id);
    let Some(mut marker) = read_market_skill_marker(&source) else {
        return Ok(false);
    };
    let legacy_reference = marker.target_references.is_empty();
    let mut references = market_target_references(&marker);
    references.remove(&root_skill_id);
    if references.is_empty() {
        if legacy_reference {
            ensure_market_dependency_not_in_use(skill_id)?;
        }
        remove_shared_skill_publications_locked(skill_id)?;
        remove_skill_entry(&source).map_err(|error| {
            AcpError::protocol(format!(
                "failed to remove unused market dependency '{skill_id}': {error}"
            ))
        })?;
        return Ok(true);
    }
    marker.target_references = references;
    marker.agent_types = Some(market_reference_targets(&marker.target_references));
    write_market_skill_marker(&source, &marker)?;
    if shared_skill_publish_enabled(&source, skill_id)? {
        publish_shared_skill_to_targets_locked(
            marker.agent_types.as_deref().unwrap_or_default(),
            skill_id,
            AgentSkillSyncMode::default(),
        )?;
    }
    Ok(false)
}

fn repair_market_target_references_locked() -> Result<(), AcpError> {
    let root = shared_skills_dir();
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(AcpError::protocol(format!(
                "failed to inspect market Skill references: {error}"
            )))
        }
    };
    let markers = entries
        .filter_map(Result::ok)
        .filter_map(|entry| read_market_skill_marker(&entry.path()))
        .map(|marker| (marker.skill_id, marker))
        .collect::<BTreeMap<_, _>>();
    let root_ids = markers
        .values()
        .flat_map(|marker| marker.target_references.keys().copied())
        .collect::<BTreeSet<_>>();
    let mut stale = Vec::new();
    for root_skill_id in root_ids {
        let closure = market_root_dependency_closure(root_skill_id, &markers);
        stale.extend(markers.values().filter_map(|marker| {
            (marker.target_references.contains_key(&root_skill_id)
                && !closure.contains(&marker.skill_id))
            .then(|| (marker.slug.clone(), root_skill_id))
        }));
    }
    for (skill_id, root_skill_id) in stale {
        release_market_dependency_reference(&skill_id, root_skill_id)?;
    }
    Ok(())
}

fn market_root_dependency_closure(
    root_skill_id: i64,
    markers: &BTreeMap<i64, MarketSkillMarker>,
) -> BTreeSet<i64> {
    let Some(root) = markers
        .get(&root_skill_id)
        .filter(|marker| marker.target_references.contains_key(&root_skill_id))
    else {
        return BTreeSet::new();
    };
    let mut closure = BTreeSet::new();
    let mut pending = vec![root.skill_id];
    while let Some(skill_id) = pending.pop() {
        if !closure.insert(skill_id) {
            continue;
        }
        if let Some(marker) = markers.get(&skill_id) {
            pending.extend(
                marker
                    .dependencies
                    .iter()
                    .map(|dependency| dependency.skill_id),
            );
        }
    }
    closure
}

fn prepare_market_skill_installs(
    installs: Vec<MarketSkillInstall>,
) -> Result<Vec<PreparedMarketSkillInstall>, AcpError> {
    let mut slugs = BTreeSet::new();
    let mut skill_ids = BTreeSet::new();
    let mut prepared = Vec::with_capacity(installs.len());
    for mut install in installs {
        let skill_id = validate_skill_id(&install.marker.slug)?;
        ensure_shared_skill_writable(&skill_id)?;
        if !slugs.insert(skill_id.clone()) || !skill_ids.insert(install.marker.skill_id) {
            return Err(AcpError::protocol(
                "market Skill install plan contains duplicate packages",
            ));
        }
        let source = shared_skill_path(&skill_id);
        ensure_market_install_target(&source, &install.marker)?;
        let existed = path_entry_exists(&source);
        if let Some(existing) = existed.then(|| read_market_skill_marker(&source)).flatten() {
            let mut references = market_target_references(&existing);
            references.extend(install.marker.target_references.clone());
            install.marker.target_references = references;
            install.marker.agent_types =
                Some(market_reference_targets(&install.marker.target_references));
        }
        let (enabled, publications) = if existed {
            (
                shared_skill_publish_enabled(&source, &skill_id)?,
                existing_shared_skill_publications(&source, &skill_id)?,
            )
        } else {
            (true, Vec::new())
        };
        let files = market_install_files(&skill_id, &install.marker, install.package, enabled)?;
        let targets = install.marker.agent_types.clone().unwrap_or_default();
        if targets.is_empty() {
            return Err(AcpError::protocol("Skill publication targets are empty"));
        }
        prepared.push(PreparedMarketSkillInstall {
            skill_id: skill_id.clone(),
            source,
            files,
            targets,
            previous: PreviousMarketSkillState {
                skill_id,
                existed,
                enabled,
                publications,
            },
        });
    }
    Ok(prepared)
}

fn begin_market_skill_swaps(
    prepared: Vec<PreparedMarketSkillInstall>,
) -> Result<Vec<PendingMarketSkillInstall>, AcpError> {
    let mut pending = Vec::with_capacity(prepared.len());
    for prepared in prepared {
        match begin_skill_directory_swap(&prepared.source, &prepared.files) {
            Ok(swap) => pending.push(PendingMarketSkillInstall { prepared, swap }),
            Err(error) => {
                rollback_market_skill_installs(pending, false);
                return Err(error);
            }
        }
    }
    Ok(pending)
}

fn publish_market_skill_installs(
    pending: &[PendingMarketSkillInstall],
) -> Result<Vec<AgentSkillItem>, AcpError> {
    let mut installed = Vec::with_capacity(pending.len());
    for value in pending {
        if value.prepared.previous.enabled {
            installed.push(publish_shared_skill_to_targets_locked(
                &value.prepared.targets,
                &value.prepared.skill_id,
                AgentSkillSyncMode::default(),
            )?);
        } else {
            remove_shared_skill_publications_locked(&value.prepared.skill_id)?;
            installed.push(build_shared_skill_item_for_agent(
                value.prepared.targets[0],
                value.prepared.skill_id.clone(),
            )?);
        }
    }
    Ok(installed)
}

fn rollback_market_skill_installs(
    pending: Vec<PendingMarketSkillInstall>,
    restore_publications: bool,
) {
    let previous = pending
        .iter()
        .map(|value| value.prepared.previous.clone())
        .collect::<Vec<_>>();
    for value in pending.into_iter().rev() {
        if let Err(error) = value.swap.rollback() {
            tracing::error!(
                skill_id = %value.prepared.skill_id,
                error = %error,
                "[skill-market] failed to roll back Skill directory"
            );
        }
    }
    if !restore_publications {
        return;
    }
    for state in previous {
        let result = if state.existed && state.enabled {
            restore_shared_skill_publications(&state.skill_id, &state.publications)
        } else {
            remove_shared_skill_publications_locked(&state.skill_id)
        };
        if let Err(error) = result {
            tracing::error!(
                skill_id = %state.skill_id,
                error = %error,
                "[skill-market] failed to restore Skill publication during rollback"
            );
        }
    }
}

fn existing_shared_skill_publications(
    source: &Path,
    skill_id: &str,
) -> Result<Vec<(AgentType, AgentSkillSyncMode)>, AcpError> {
    let mut publications = Vec::new();
    for agent_type in skill_capable_agent_types() {
        let (published, copy_mode) = shared_skill_publish_status(agent_type, source, skill_id)?;
        if published {
            publications.push((
                agent_type,
                if copy_mode {
                    AgentSkillSyncMode::Copy
                } else {
                    AgentSkillSyncMode::Symlink
                },
            ));
        }
    }
    Ok(publications)
}

fn restore_shared_skill_publications(
    skill_id: &str,
    publications: &[(AgentType, AgentSkillSyncMode)],
) -> Result<(), AcpError> {
    remove_shared_skill_publications_locked(skill_id)?;
    for (agent_type, mode) in publications {
        publish_shared_skill_to_agent(*agent_type, skill_id, *mode)?;
    }
    Ok(())
}

fn ensure_market_install_target(source: &Path, marker: &MarketSkillMarker) -> Result<(), AcpError> {
    if !path_entry_exists(source) {
        return Ok(());
    }
    if !is_real_skill_directory(source) {
        return Err(AcpError::protocol(
            "An existing non-directory entry uses this Skill slug",
        ));
    }
    if let Some(existing) = read_market_skill_marker(source) {
        if existing.skill_id == marker.skill_id {
            return Ok(());
        }
        return Err(AcpError::protocol(
            "A different market Skill uses this slug",
        ));
    }
    if marker.publisher_type == "official" {
        if let Some(existing) = read_official_skill_marker(source) {
            if existing.skill_id == marker.slug && existing.source == "official_market" {
                return Ok(());
            }
        }
    }
    Err(AcpError::protocol(
        "A local Skill with this slug already exists and will not be overwritten",
    ))
}

fn is_real_skill_directory(path: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return false;
        }
    }
    true
}

fn market_install_files(
    skill_id: &str,
    marker: &MarketSkillMarker,
    package: crate::acp::skill_package::ValidatedSkillPackage,
    enabled: bool,
) -> Result<Vec<(PathBuf, Vec<u8>)>, AcpError> {
    let mut files: Vec<(PathBuf, Vec<u8>)> = package
        .files
        .into_iter()
        .map(|file| (file.path, file.bytes))
        .collect();
    let market = serde_json::to_vec_pretty(marker).map_err(|error| {
        AcpError::protocol(format!("failed to serialize market marker: {error}"))
    })?;
    let publish = serde_json::to_vec_pretty(&SharedSkillPublishStateMarker {
        skill_id: skill_id.to_string(),
        enabled,
    })
    .map_err(|error| AcpError::protocol(format!("failed to serialize publish state: {error}")))?;
    files.push((PathBuf::from(MARKET_SKILL_MARKER), market));
    files.push((PathBuf::from(SHARED_SKILL_PUBLISH_STATE_MARKER), publish));
    Ok(files)
}

fn market_target_references(marker: &MarketSkillMarker) -> BTreeMap<i64, Vec<AgentType>> {
    if !marker.target_references.is_empty() {
        return marker.target_references.clone();
    }
    marker
        .agent_types
        .clone()
        .filter(|targets| !targets.is_empty())
        .map(|targets| BTreeMap::from([(marker.skill_id, targets)]))
        .unwrap_or_default()
}

fn market_reference_targets(references: &BTreeMap<i64, Vec<AgentType>>) -> Vec<AgentType> {
    references
        .values()
        .flatten()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn write_market_skill_marker(source: &Path, marker: &MarketSkillMarker) -> Result<(), AcpError> {
    let bytes = serde_json::to_vec_pretty(marker).map_err(|error| {
        AcpError::protocol(format!("failed to serialize market marker: {error}"))
    })?;
    fs::write(source.join(MARKET_SKILL_MARKER), bytes)
        .map_err(|error| AcpError::protocol(format!("failed to write market marker: {error}")))
}

fn shared_skill_publish_state_path(source: &Path) -> PathBuf {
    source.join(SHARED_SKILL_PUBLISH_STATE_MARKER)
}

fn shared_skill_mutation_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn shared_skill_publish_enabled(source: &Path, skill_id: &str) -> Result<bool, AcpError> {
    // Missing state means enabled so manually added and pre-v1 central skills
    // are repaired automatically. Disable/delete paths persist `false` first.
    let path = shared_skill_publish_state_path(source);
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(true),
        Err(error) => {
            return Err(AcpError::protocol(format!(
                "failed to read publish state for skill '{skill_id}': {error}"
            )))
        }
    };
    let marker: SharedSkillPublishStateMarker = serde_json::from_str(&raw).map_err(|error| {
        AcpError::protocol(format!(
            "invalid publish state for skill '{skill_id}': {error}"
        ))
    })?;
    if marker.skill_id != skill_id {
        return Err(AcpError::protocol(format!(
            "publish state belongs to '{}' instead of '{skill_id}'",
            marker.skill_id
        )));
    }
    Ok(marker.enabled)
}

fn write_shared_skill_publish_state(
    source: &Path,
    skill_id: &str,
    enabled: bool,
) -> Result<(), AcpError> {
    let marker = SharedSkillPublishStateMarker {
        skill_id: skill_id.to_string(),
        enabled,
    };
    let raw = serde_json::to_string_pretty(&marker).map_err(|error| {
        AcpError::protocol(format!("failed to serialize publish state: {error}"))
    })?;
    fs::write(shared_skill_publish_state_path(source), raw).map_err(|error| {
        AcpError::protocol(format!(
            "failed to persist publish state for skill '{skill_id}': {error}"
        ))
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SharedSkillCopyMarker {
    skill_id: String,
    source_path: String,
}

fn shared_copy_marker_path(path: &Path) -> PathBuf {
    path.join(SHARED_SKILL_COPY_MARKER)
}

fn shared_copy_marker_matches(path: &Path, source: &Path, skill_id: &str) -> bool {
    let Ok(content) = fs::read_to_string(shared_copy_marker_path(path)) else {
        return false;
    };
    let Ok(marker) = serde_json::from_str::<SharedSkillCopyMarker>(&content) else {
        return false;
    };
    marker.skill_id == skill_id && marker.source_path == source.to_string_lossy().as_ref()
}

fn write_shared_copy_marker(path: &Path, source: &Path, skill_id: &str) -> Result<(), AcpError> {
    let marker = SharedSkillCopyMarker {
        skill_id: skill_id.to_string(),
        source_path: source.to_string_lossy().to_string(),
    };
    let serialized = serde_json::to_string_pretty(&marker)
        .map_err(|e| AcpError::protocol(format!("failed to serialize copy marker: {e}")))?;
    fs::write(shared_copy_marker_path(path), serialized)
        .map_err(|e| AcpError::protocol(format!("failed to write copy marker: {e}")))
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ft.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if ft.is_file() {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn shared_skill_publish_dirs(agent_type: AgentType) -> Result<Vec<PathBuf>, AcpError> {
    scoped_skill_dirs(agent_type, AgentSkillScope::Global, None)
}

fn preferred_shared_skill_publish_path(
    agent_type: AgentType,
    skill_id: &str,
) -> Result<PathBuf, AcpError> {
    preferred_scope_skill_dir(agent_type, AgentSkillScope::Global, None)
        .map(|dir| dir.join(skill_id))
}

fn shared_skill_publish_status(
    agent_type: AgentType,
    source: &Path,
    skill_id: &str,
) -> Result<(bool, bool), AcpError> {
    for dir in shared_skill_publish_dirs(agent_type)? {
        let candidate = dir.join(skill_id);
        if !path_entry_exists(&candidate) {
            continue;
        }
        if classify_link(&candidate, source) == ExpertLinkState::LinkedToIywClaw {
            return Ok((true, false));
        }
        if shared_copy_marker_matches(&candidate, source, skill_id) {
            return Ok((true, true));
        }
    }
    Ok((false, false))
}

fn build_shared_skill_item_for_agent(
    agent_type: AgentType,
    skill_id: String,
) -> Result<AgentSkillItem, AcpError> {
    let source = shared_skill_path(&skill_id);
    let mut skill = locate_existing_skill(
        &shared_skills_dir(),
        SkillStorageKind::SkillDirectoryOnly,
        &skill_id,
        AgentSkillScope::Global,
        false,
    )
    .ok_or_else(|| AcpError::protocol(format!("skill not found: {skill_id}")))?;
    let (enabled, copy_mode) = shared_skill_publish_status(agent_type, &source, &skill_id)?;
    skill.enabled = enabled;
    skill.copy_mode = copy_mode;
    skill.read_only = is_reserved_shared_skill_id(&skill_id);
    skill.official = is_official_shared_skill(&skill_id);
    Ok(skill)
}

fn list_shared_skills_for_agent(
    agent_type: AgentType,
    include_unpublished: bool,
) -> Result<Vec<AgentSkillItem>, AcpError> {
    list_market_skills_from_dir(agent_type, &shared_skills_dir(), include_unpublished)
}

fn list_market_skills_from_dir(
    agent_type: AgentType,
    dir: &Path,
    include_unpublished: bool,
) -> Result<Vec<AgentSkillItem>, AcpError> {
    let mut skills = list_skills_from_dir(
        AgentSkillScope::Global,
        dir,
        SkillStorageKind::SkillDirectoryOnly,
        false,
    )?;
    skills.retain(|skill| !is_reserved_shared_skill_id(&skill.id));
    for skill in &mut skills {
        let source = PathBuf::from(&skill.path);
        let (enabled, copy_mode) = shared_skill_publish_status(agent_type, &source, &skill.id)?;
        skill.enabled = enabled;
        skill.copy_mode = copy_mode;
        skill.official = is_official_skill_source(&source);
    }
    if !include_unpublished {
        skills.retain(|skill| skill.enabled);
    }
    Ok(skills)
}

fn locate_read_only_global_native_skill(
    agent_type: AgentType,
    spec: &SkillStorageSpec,
    skill_id: &str,
) -> Option<AgentSkillItem> {
    for dir in &spec.global_dirs {
        let Some(mut skill) =
            locate_existing_skill(dir, spec.kind, skill_id, AgentSkillScope::Global, false)
        else {
            continue;
        };
        set_skill_read_only(agent_type, &mut skill);
        if skill.read_only {
            return Some(skill);
        }
    }
    None
}

fn import_native_skill_to_shared_source(
    skill: &AgentSkillItem,
    skill_id: &str,
) -> Result<(), AcpError> {
    let target = shared_skill_path(skill_id);
    if target.join("SKILL.md").is_file() {
        return Ok(());
    }
    if path_entry_exists(&target) {
        return Err(AcpError::protocol(format!(
            "shared skill target already exists and is not a valid skill: {}",
            target.to_string_lossy()
        )));
    }

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            AcpError::protocol(format!("failed to create shared skills directory: {e}"))
        })?;
    }

    let source_path = PathBuf::from(&skill.path);
    match skill.layout {
        AgentSkillLayout::SkillDirectory => {
            copy_dir_recursive(&source_path, &target)
                .map_err(|e| AcpError::protocol(format!("failed to import skill: {e}")))?;
        }
        AgentSkillLayout::MarkdownFile => {
            fs::create_dir_all(&target)
                .map_err(|e| AcpError::protocol(format!("failed to create skill: {e}")))?;
            let content_path = skill_content_path(skill.layout, &source_path);
            fs::copy(&content_path, target.join("SKILL.md"))
                .map_err(|e| AcpError::protocol(format!("failed to import skill: {e}")))?;
        }
    }

    if !target.join("SKILL.md").is_file() {
        return Err(AcpError::protocol(format!(
            "imported skill does not contain SKILL.md: {skill_id}"
        )));
    }
    Ok(())
}

fn skill_capable_agent_types() -> Vec<AgentType> {
    crate::commands::managed_skills::supported_skill_agent_types()
}

async fn installed_enabled_skill_agent_types(
    conn: &sea_orm::DatabaseConnection,
    primary_agent_type: AgentType,
) -> Result<Vec<AgentType>, AcpError> {
    let settings = agent_setting_service::list_map_by_agent_type(conn)
        .await
        .map_err(|error| AcpError::protocol(error.to_string()))?;
    let mut targets = skill_capable_agent_types()
        .into_iter()
        .filter(|agent_type| {
            settings.get(agent_type).is_some_and(|setting| {
                setting.enabled
                    && setting
                        .installed_version
                        .as_deref()
                        .is_some_and(|version| !version.is_empty())
            })
        })
        .collect::<Vec<_>>();
    let Some(primary_index) = targets
        .iter()
        .position(|agent_type| *agent_type == primary_agent_type)
    else {
        return Err(AcpError::protocol(format!(
            "{primary_agent_type} is not installed and enabled"
        )));
    };
    targets.swap(0, primary_index);
    for agent_type in &targets {
        crate::commands::agent_storage::ensure_active_agent_profile_layout(conn, *agent_type)
            .await
            .map_err(|error| AcpError::protocol(error.to_string()))?;
    }
    Ok(targets)
}

fn take_over_read_only_global_native_skill(
    agent_type: AgentType,
    spec: &SkillStorageSpec,
    skill_id: &str,
    sync_mode: AgentSkillSyncMode,
    targets: &[AgentType],
) -> Result<AgentSkillItem, AcpError> {
    let _paths = require_private_agent_storage_for_write()?;
    ensure_shared_skill_writable(skill_id)?;

    let source = shared_skill_path(skill_id);
    if !source.join("SKILL.md").is_file() {
        let native = locate_read_only_global_native_skill(agent_type, spec, skill_id)
            .ok_or_else(|| AcpError::protocol(format!("built-in skill not found: {skill_id}")))?;
        import_native_skill_to_shared_source(&native, skill_id)?;
    }

    write_shared_skill_publish_state(&source, skill_id, true)?;
    let _guard = shared_skill_mutation_guard();
    publish_shared_skill_to_active_agents_locked(agent_type, targets, skill_id, sync_mode)
}

struct PreparedPublishTarget {
    target: PathBuf,
    backup: PathBuf,
    keep_on_success: bool,
}

impl PreparedPublishTarget {
    fn commit(self) {
        if !self.keep_on_success {
            if let Err(error) = remove_skill_entry(&self.backup) {
                tracing::warn!(path = %self.backup.display(), error = %error, "failed to remove publication backup");
            }
        }
    }

    fn rollback(self) -> Result<(), AcpError> {
        if path_entry_exists(&self.target) {
            remove_skill_entry(&self.target).map_err(|error| {
                AcpError::protocol(format!(
                    "failed to remove partial Skill publication: {error}"
                ))
            })?;
        }
        move_skill_entry(&self.backup, &self.target)
    }
}

fn prepare_shared_publish_target(
    target: &Path,
    source: &Path,
    skill_id: &str,
) -> Result<Option<PreparedPublishTarget>, AcpError> {
    if !path_entry_exists(target) {
        return Ok(None);
    }
    let agent_skills_dir = target
        .parent()
        .ok_or_else(|| AcpError::protocol("skill target has no parent directory"))?;
    let managed = classify_link(target, source) == ExpertLinkState::LinkedToIywClaw
        || shared_copy_marker_matches(target, source, skill_id);
    let backup = if managed {
        agent_skills_dir.join(format!(
            ".iyw-claw-publish-backup-{skill_id}-{}",
            uuid::Uuid::new_v4()
        ))
    } else {
        next_conflict_backup_path(agent_skills_dir, skill_id)
    };
    move_skill_entry(target, &backup).map_err(|error| {
        AcpError::protocol(format!(
            "failed to preserve conflicting skill '{}' at '{}': {error}",
            target.display(),
            backup.display()
        ))
    })?;
    Ok(Some(PreparedPublishTarget {
        target: target.to_path_buf(),
        backup,
        keep_on_success: !managed,
    }))
}

fn next_conflict_backup_path(agent_skills_dir: &Path, skill_id: &str) -> PathBuf {
    let conflicts = agent_skills_dir.join(CONFLICTED_SKILLS_DIR);
    let initial = conflicts.join(skill_id);
    if !path_entry_exists(&initial) {
        return initial;
    }
    for suffix in 1_u64.. {
        let candidate = conflicts.join(format!("{skill_id}-{suffix}"));
        if !path_entry_exists(&candidate) {
            return candidate;
        }
    }
    unreachable!("u64 conflict suffixes exhausted")
}

fn publish_shared_skill_to_agent(
    agent_type: AgentType,
    skill_id: &str,
    sync_mode: AgentSkillSyncMode,
) -> Result<AgentSkillItem, AcpError> {
    let source = shared_skill_path(skill_id);
    if !source.join("SKILL.md").is_file() {
        return Err(AcpError::protocol(format!("skill not found: {skill_id}")));
    }
    let target = preferred_shared_skill_publish_path(agent_type, skill_id)?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            AcpError::protocol(format!("failed to create agent skills directory: {e}"))
        })?;
    }
    let prepared = prepare_shared_publish_target(&target, &source, skill_id)?;
    let result = (|| {
        let copy_mode = create_shared_skill_publication(&source, &target, skill_id, sync_mode)?;
        let mut skill = build_shared_skill_item_for_agent(agent_type, skill_id.to_string())?;
        skill.enabled = true;
        skill.copy_mode = copy_mode;
        Ok(skill)
    })();
    match result {
        Ok(skill) => {
            if let Some(prepared) = prepared {
                prepared.commit();
            }
            Ok(skill)
        }
        Err(error) => {
            let rollback = match prepared {
                Some(prepared) => prepared.rollback(),
                None => remove_partial_publish_target(&target),
            };
            if let Err(rollback_error) = rollback {
                return Err(AcpError::protocol(format!(
                    "{error}; failed to restore previous publication: {rollback_error}"
                )));
            }
            Err(error)
        }
    }
}

fn create_shared_skill_publication(
    source: &Path,
    target: &Path,
    skill_id: &str,
    sync_mode: AgentSkillSyncMode,
) -> Result<bool, AcpError> {
    Ok(match sync_mode {
        AgentSkillSyncMode::Symlink => match create_link_raw(source, target) {
            Ok(is_copy) => {
                if is_copy {
                    write_shared_copy_marker(target, source, skill_id)?;
                }
                is_copy
            }
            Err(err) => {
                return Err(AcpError::protocol(format!(
                    "failed to link skill into agent directory: {err}"
                )));
            }
        },
        AgentSkillSyncMode::Copy => {
            copy_dir_recursive(source, target)
                .map_err(|e| AcpError::protocol(format!("failed to copy skill: {e}")))?;
            write_shared_copy_marker(target, source, skill_id)?;
            true
        }
    })
}

fn remove_partial_publish_target(target: &Path) -> Result<(), AcpError> {
    if !path_entry_exists(target) {
        return Ok(());
    }
    remove_skill_entry(target).map_err(|error| {
        AcpError::protocol(format!(
            "failed to remove partial Skill publication: {error}"
        ))
    })
}

fn publish_shared_skill_to_active_agents_locked(
    primary_agent_type: AgentType,
    agent_types: &[AgentType],
    skill_id: &str,
    sync_mode: AgentSkillSyncMode,
) -> Result<AgentSkillItem, AcpError> {
    let primary = publish_shared_skill_to_agent(primary_agent_type, skill_id, sync_mode)?;
    for agent_type in agent_types
        .iter()
        .copied()
        .filter(|agent_type| *agent_type != primary_agent_type)
    {
        if let Err(error) = publish_shared_skill_to_agent(agent_type, skill_id, sync_mode) {
            tracing::warn!(
                primary_agent_type = %primary_agent_type,
                agent_type = %agent_type,
                skill_id,
                error = %error,
                "[skills] secondary Agent publication failed"
            );
        }
    }
    for agent_type in skill_capable_agent_types()
        .into_iter()
        .filter(|agent_type| !agent_types.contains(agent_type))
    {
        if let Err(error) = remove_shared_skill_publication_from_agent(agent_type, skill_id) {
            tracing::warn!(
                agent_type = %agent_type,
                skill_id,
                error = %error,
                "[skills] stale Agent publication cleanup failed"
            );
        }
    }
    Ok(primary)
}

fn publish_shared_skill_to_targets_locked(
    agent_types: &[AgentType],
    skill_id: &str,
    sync_mode: AgentSkillSyncMode,
) -> Result<AgentSkillItem, AcpError> {
    let primary_agent_type = *agent_types
        .first()
        .ok_or_else(|| AcpError::protocol("Skill publication targets are empty"))?;
    let primary = publish_shared_skill_to_agent(primary_agent_type, skill_id, sync_mode)?;
    for agent_type in agent_types.iter().copied().skip(1) {
        publish_shared_skill_to_agent(agent_type, skill_id, sync_mode)?;
    }
    for agent_type in skill_capable_agent_types()
        .into_iter()
        .filter(|agent_type| !agent_types.contains(agent_type))
    {
        remove_shared_skill_publication_from_agent(agent_type, skill_id)?;
    }
    Ok(primary)
}

pub async fn reconcile_shared_market_skills(
    conn: &sea_orm::DatabaseConnection,
) -> Result<(), AcpError> {
    if AgentStoragePaths::active().is_none() {
        return Ok(());
    }
    require_private_agent_storage_for_write()?;
    let settings = agent_setting_service::list_map_by_agent_type(conn)
        .await
        .map_err(|error| AcpError::protocol(error.to_string()))?;
    let agent_states = skill_capable_agent_types()
        .into_iter()
        .map(|agent_type| {
            let enabled = settings.get(&agent_type).is_some_and(|setting| {
                setting.enabled
                    && setting
                        .installed_version
                        .as_deref()
                        .is_some_and(|value| !value.is_empty())
            });
            (agent_type, enabled)
        })
        .collect::<Vec<_>>();
    for (agent_type, enabled) in &agent_states {
        if *enabled {
            crate::commands::agent_storage::ensure_active_agent_profile_layout(conn, *agent_type)
                .await
                .map_err(|error| AcpError::protocol(error.to_string()))?;
        }
    }
    let _guard = shared_skill_mutation_guard();
    let mut errors = Vec::new();
    if let Err(error) = repair_market_target_references_locked() {
        errors.push(format!("market reference repair: {error}"));
    }
    for (agent_type, enabled) in agent_states {
        if let Err(error) = reconcile_shared_skills_for_agent_state_locked(agent_type, enabled) {
            errors.push(format!("{agent_type}: {error}"));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(AcpError::protocol(format!(
            "shared skill reconcile failed for {} agent(s): {}",
            errors.len(),
            errors.join("; ")
        )))
    }
}

pub(crate) fn reconcile_shared_market_skills_for_agent(
    agent_type: AgentType,
) -> Result<(), AcpError> {
    require_private_agent_storage_for_write()?;
    let _guard = shared_skill_mutation_guard();
    reconcile_shared_market_skills_for_agent_locked(agent_type)
}

pub(crate) fn reconcile_shared_skills_for_agent_state(
    agent_type: AgentType,
    enabled: bool,
) -> Result<(), AcpError> {
    require_private_agent_storage_for_write()?;
    let _guard = shared_skill_mutation_guard();
    reconcile_shared_skills_for_agent_state_locked(agent_type, enabled)
}

fn reconcile_shared_skills_for_agent_state_locked(
    agent_type: AgentType,
    enabled: bool,
) -> Result<(), AcpError> {
    if enabled {
        return reconcile_shared_market_skills_for_agent_locked(agent_type);
    }
    let skills = list_skills_from_dir(
        AgentSkillScope::Global,
        &shared_skills_dir(),
        SkillStorageKind::SkillDirectoryOnly,
        false,
    )?;
    for skill in skills {
        if !is_reserved_shared_skill_id(&skill.id) {
            remove_shared_skill_publication_from_agent(agent_type, &skill.id)?;
        }
    }
    Ok(())
}

fn reconcile_shared_market_skills_for_agent_locked(agent_type: AgentType) -> Result<(), AcpError> {
    if skill_storage_spec(agent_type).is_none() {
        return Ok(());
    }
    let skills = list_skills_from_dir(
        AgentSkillScope::Global,
        &shared_skills_dir(),
        SkillStorageKind::SkillDirectoryOnly,
        false,
    )?;
    let mut errors = Vec::new();
    for skill in skills {
        if is_reserved_shared_skill_id(&skill.id) {
            continue;
        }
        if let Err(error) = reconcile_shared_market_skill_for_agent(agent_type, &skill) {
            tracing::warn!(
                agent_type = %agent_type,
                skill_id = %skill.id,
                error = %error,
                "[skills] failed to reconcile central skill publication"
            );
            errors.push(format!("{}: {error}", skill.id));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(AcpError::protocol(format!(
            "failed to reconcile {} central skill(s) for {agent_type}: {}",
            errors.len(),
            errors.join("; ")
        )))
    }
}

fn reconcile_shared_market_skill_for_agent(
    agent_type: AgentType,
    skill: &AgentSkillItem,
) -> Result<(), AcpError> {
    let source = Path::new(&skill.path);
    if read_market_skill_marker(source).is_some_and(|marker| {
        marker
            .agent_types
            .is_some_and(|targets| !targets.contains(&agent_type))
    }) {
        remove_shared_skill_publication_from_agent(agent_type, &skill.id)?;
        return Ok(());
    }
    if !shared_skill_publish_enabled(source, &skill.id)? {
        if remove_shared_skill_publication_from_agent(agent_type, &skill.id)? {
            tracing::info!(
                agent_type = %agent_type,
                skill_id = %skill.id,
                "[skills] removed explicitly disabled central skill publication"
            );
        }
        return Ok(());
    }
    let (published, copy_mode) = shared_skill_publish_status(agent_type, source, &skill.id)?;
    if published && !copy_mode {
        return Ok(());
    }
    let result =
        publish_shared_skill_to_agent(agent_type, &skill.id, AgentSkillSyncMode::default())?;
    tracing::info!(
        agent_type = %agent_type,
        skill_id = %skill.id,
        copy_mode = result.copy_mode,
        "[skills] published central skill to agent profile"
    );
    Ok(())
}

fn remove_shared_skill_publication_from_agent(
    agent_type: AgentType,
    skill_id: &str,
) -> Result<bool, AcpError> {
    let source = shared_skill_path(skill_id);
    let mut removed = false;
    for dir in shared_skill_publish_dirs(agent_type)? {
        let candidate = dir.join(skill_id);
        if !path_entry_exists(&candidate) {
            continue;
        }
        let linked = classify_link(&candidate, &source) == ExpertLinkState::LinkedToIywClaw;
        let copied = shared_copy_marker_matches(&candidate, &source, skill_id);
        if linked || copied {
            remove_skill_entry(&candidate).map_err(|error| {
                AcpError::protocol(format!("failed to remove published skill: {error}"))
            })?;
            removed = true;
        }
    }
    Ok(removed)
}

fn remove_shared_skill_publications_locked(skill_id: &str) -> Result<(), AcpError> {
    for agent_type in skill_capable_agent_types() {
        remove_shared_skill_publication_from_agent(agent_type, skill_id)?;
    }
    Ok(())
}

/// Codex ships a handful of built-in skills under `~/.codex/skills/.system/`
/// (imagegen, skill-creator, etc.). We scan that directory so users see
/// these in the `$` autocomplete and the Skills settings list — but any
/// write to those files would clobber the CLI's own assets.
fn is_read_only_skill_path(agent_type: AgentType, skill_path: &Path) -> bool {
    if agent_type != AgentType::Codex {
        return false;
    }
    let ro_root = codex_home_dir().join("skills").join(".system");
    skill_path.starts_with(&ro_root)
}

fn skill_content_path(layout: AgentSkillLayout, skill_path: &Path) -> PathBuf {
    match layout {
        AgentSkillLayout::SkillDirectory => skill_path.join("SKILL.md"),
        AgentSkillLayout::MarkdownFile => skill_path.to_path_buf(),
    }
}

const MAX_SKILL_IMPORT_FILES: usize = 512;
const MAX_SKILL_IMPORT_BYTES: usize = 25 * 1024 * 1024;
pub(crate) const MAX_SKILL_IMPORT_REQUEST_BYTES: usize = 36 * 1024 * 1024;

struct ValidatedSkillDirectory {
    files: Vec<(PathBuf, Vec<u8>)>,
    skill_content: String,
}

fn validate_skill_directory(
    files: Option<Vec<AgentSkillFile>>,
) -> Result<Option<ValidatedSkillDirectory>, AcpError> {
    let Some(files) = files else {
        return Ok(None);
    };
    if files.is_empty() || files.len() > MAX_SKILL_IMPORT_FILES {
        return Err(AcpError::protocol(format!(
            "skill folder must contain 1 to {MAX_SKILL_IMPORT_FILES} files"
        )));
    }

    let mut seen = BTreeSet::new();
    let mut total_bytes = 0usize;
    let mut validated = Vec::with_capacity(files.len());
    for file in files {
        let (path, bytes) = decode_skill_file(file)?;
        let key = path.to_string_lossy().replace('\\', "/");
        let key = if cfg!(any(windows, target_os = "macos")) {
            key.to_ascii_lowercase()
        } else {
            key
        };
        if !seen.insert(key) {
            return Err(AcpError::protocol("skill folder contains duplicate paths"));
        }
        total_bytes = total_bytes.saturating_add(bytes.len());
        if total_bytes > MAX_SKILL_IMPORT_BYTES {
            return Err(AcpError::protocol("skill folder exceeds the 25 MB limit"));
        }
        validated.push((path, bytes));
    }

    let skill_content = validated
        .iter()
        .find(|(path, _)| path == Path::new("SKILL.md"))
        .ok_or_else(|| AcpError::protocol("skill folder root must contain SKILL.md"))
        .and_then(|(_, bytes)| {
            String::from_utf8(bytes.clone())
                .map_err(|_| AcpError::protocol("SKILL.md must be UTF-8 text"))
        })?;
    Ok(Some(ValidatedSkillDirectory {
        files: validated,
        skill_content,
    }))
}

fn decode_skill_file(file: AgentSkillFile) -> Result<(PathBuf, Vec<u8>), AcpError> {
    let path = validate_skill_file_path(&file.path)?;
    let max_encoded_size = MAX_SKILL_IMPORT_BYTES.div_ceil(3) * 4 + 4;
    if file.content_base64.len() > max_encoded_size {
        return Err(AcpError::protocol("skill folder exceeds the 25 MB limit"));
    }
    let bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        file.content_base64,
    )
    .map_err(|_| AcpError::protocol("skill folder contains invalid file content"))?;
    Ok((path, bytes))
}

fn validate_skill_file_path(raw: &str) -> Result<PathBuf, AcpError> {
    if raw.is_empty()
        || raw.contains('\0')
        || raw.contains('\\')
        || raw
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(AcpError::protocol(
            "skill file paths must be safe relative paths",
        ));
    }
    let path = PathBuf::from(raw);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(AcpError::protocol(
            "skill file paths must be safe relative paths",
        ));
    }
    Ok(path)
}

fn write_skill_directory(target: &Path, files: &[(PathBuf, Vec<u8>)]) -> Result<(), AcpError> {
    begin_skill_directory_swap(target, files)?.commit();
    Ok(())
}

struct PendingSkillDirectorySwap {
    target: PathBuf,
    backup: Option<PathBuf>,
}

impl PendingSkillDirectorySwap {
    fn commit(self) {
        if let Some(backup) = self.backup {
            if let Err(error) = remove_skill_entry(&backup) {
                tracing::warn!(path = %backup.display(), error = %error, "failed to remove Skill backup");
            }
        }
    }

    fn rollback(self) -> Result<(), AcpError> {
        if path_entry_exists(&self.target) {
            remove_skill_entry(&self.target).map_err(|error| {
                AcpError::protocol(format!("failed to remove replacement Skill: {error}"))
            })?;
        }
        if let Some(backup) = self.backup {
            fs::rename(&backup, &self.target).map_err(|error| {
                AcpError::protocol(format!("failed to restore previous Skill: {error}"))
            })?;
        }
        Ok(())
    }
}

fn begin_skill_directory_swap(
    target: &Path,
    files: &[(PathBuf, Vec<u8>)],
) -> Result<PendingSkillDirectorySwap, AcpError> {
    let parent = target
        .parent()
        .ok_or_else(|| AcpError::protocol("skill directory has no parent"))?;
    fs::create_dir_all(parent)
        .map_err(|e| AcpError::protocol(format!("failed to create skill parent: {e}")))?;
    let suffix = uuid::Uuid::new_v4();
    let staging = parent.join(format!(".iyw-claw-skill-import-{suffix}"));
    let backup = parent.join(format!(".iyw-claw-skill-backup-{suffix}"));

    if let Err(error) = write_skill_directory_files(&staging, files) {
        let _ = remove_skill_entry(&staging);
        return Err(error);
    }
    let backup = if path_entry_exists(target) {
        if let Err(error) = fs::rename(target, &backup) {
            let _ = remove_skill_entry(&staging);
            return Err(AcpError::protocol(format!(
                "failed to replace skill folder: {error}"
            )));
        }
        Some(backup)
    } else {
        None
    };
    if let Err(error) = fs::rename(&staging, target) {
        if let Some(backup) = &backup {
            let _ = fs::rename(backup, target);
        }
        let _ = remove_skill_entry(&staging);
        return Err(AcpError::protocol(format!(
            "failed to install skill folder: {error}"
        )));
    }
    Ok(PendingSkillDirectorySwap {
        target: target.to_path_buf(),
        backup,
    })
}

fn write_skill_directory_files(
    target: &Path,
    files: &[(PathBuf, Vec<u8>)],
) -> Result<(), AcpError> {
    fs::create_dir_all(target)
        .map_err(|e| AcpError::protocol(format!("failed to stage skill folder: {e}")))?;
    for (relative, content) in files {
        let path = target.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                AcpError::protocol(format!("failed to stage skill subdirectory: {e}"))
            })?;
        }
        fs::write(path, content)
            .map_err(|e| AcpError::protocol(format!("failed to stage skill file: {e}")))?;
    }
    Ok(())
}

/// Symlink-safe removal: if `path` is a symlink (to a file or directory),
/// only the link itself is removed. Otherwise directories are removed
/// recursively and files are unlinked. This prevents `remove_dir_all` from
/// accidentally wiping the contents of a symlink target — which is critical
/// for the Experts feature where agent skill dirs may contain symlinks into
/// the central `~/.iyw-claw/skills/` store.
pub(crate) fn remove_skill_entry(path: &Path) -> std::io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    let file_type = meta.file_type();

    #[cfg(windows)]
    let is_reparse_point = {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    };

    if file_type.is_symlink() {
        #[cfg(windows)]
        {
            // Directory symlinks on Windows require remove_dir.
            return match fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
                    fs::remove_dir(path)
                }
                Err(err) => Err(err),
            };
        }

        #[cfg(not(windows))]
        {
            return fs::remove_file(path);
        }
    }

    if file_type.is_dir() {
        #[cfg(windows)]
        {
            // Junctions are directory reparse points; remove only the link.
            if is_reparse_point {
                return fs::remove_dir(path);
            }
        }
        return fs::remove_dir_all(path);
    }

    fs::remove_file(path)
}

fn path_entry_exists(path: &Path) -> bool {
    path.exists() || fs::symlink_metadata(path).is_ok()
}

fn move_skill_entry(source: &Path, target: &Path) -> Result<(), AcpError> {
    if !path_entry_exists(source) {
        return Err(AcpError::protocol("skill entry does not exist"));
    }
    if path_entry_exists(target) {
        return Err(AcpError::protocol(format!(
            "target skill entry already exists: {}",
            target.display()
        )));
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AcpError::protocol(format!("failed to create skill directory: {e}")))?;
    }
    fs::rename(source, target).map_err(|e| {
        AcpError::protocol(format!(
            "failed to move skill entry from '{}' to '{}': {e}",
            source.display(),
            target.display()
        ))
    })
}

fn collect_skills_from_dir(
    scope: AgentSkillScope,
    dir: &Path,
    kind: SkillStorageKind,
    enabled: bool,
    by_id: &mut BTreeMap<String, AgentSkillItem>,
) -> Result<(), AcpError> {
    if !dir.exists() {
        return Ok(());
    }

    let entries = fs::read_dir(dir)
        .map_err(|e| AcpError::protocol(format!("failed to read skills directory: {e}")))?;

    for entry in entries {
        let entry = match entry {
            Ok(value) => value,
            Err(_) => continue,
        };
        let path = entry.path();
        let file_name = entry.file_name();
        let id = file_name.to_string_lossy().to_string();

        if path.is_dir()
            && matches!(
                kind,
                SkillStorageKind::SkillDirectoryOnly
                    | SkillStorageKind::SkillDirectoryOrMarkdownFile
            )
        {
            let skill_doc = path.join("SKILL.md");
            if !skill_doc.is_file() {
                continue;
            }
            if by_id.contains_key(&id) {
                continue;
            }
            by_id.insert(
                id.clone(),
                build_skill_item(id, scope, AgentSkillLayout::SkillDirectory, path, enabled),
            );
            continue;
        }

        if path.is_file()
            && matches!(kind, SkillStorageKind::SkillDirectoryOrMarkdownFile)
            && is_markdown_file(&path)
        {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
                .unwrap_or_else(|| id.clone());
            if by_id.contains_key(&stem) {
                continue;
            }
            by_id.insert(
                stem.clone(),
                build_skill_item(stem, scope, AgentSkillLayout::MarkdownFile, path, enabled),
            );
        }
    }

    Ok(())
}

pub(crate) fn list_skills_from_dir(
    scope: AgentSkillScope,
    dir: &Path,
    kind: SkillStorageKind,
    include_disabled: bool,
) -> Result<Vec<AgentSkillItem>, AcpError> {
    let mut by_id: BTreeMap<String, AgentSkillItem> = BTreeMap::new();
    collect_skills_from_dir(scope, dir, kind, true, &mut by_id)?;
    if include_disabled {
        collect_skills_from_dir(scope, &disabled_skills_dir(dir), kind, false, &mut by_id)?;
    }
    Ok(by_id.into_values().collect())
}

fn locate_existing_skill(
    dir: &Path,
    kind: SkillStorageKind,
    skill_id: &str,
    scope: AgentSkillScope,
    enabled: bool,
) -> Option<AgentSkillItem> {
    if matches!(
        kind,
        SkillStorageKind::SkillDirectoryOnly | SkillStorageKind::SkillDirectoryOrMarkdownFile
    ) {
        let skill_dir = dir.join(skill_id);
        if skill_dir.is_dir() && skill_dir.join("SKILL.md").is_file() {
            return Some(build_skill_item(
                skill_id.to_string(),
                scope,
                AgentSkillLayout::SkillDirectory,
                skill_dir,
                enabled,
            ));
        }
    }

    if matches!(kind, SkillStorageKind::SkillDirectoryOrMarkdownFile) {
        let file_path = dir.join(format!("{skill_id}.md"));
        if file_path.is_file() {
            return Some(build_skill_item(
                skill_id.to_string(),
                scope,
                AgentSkillLayout::MarkdownFile,
                file_path,
                enabled,
            ));
        }
    }

    None
}

fn locate_existing_skill_across_dirs(
    dirs: &[PathBuf],
    kind: SkillStorageKind,
    skill_id: &str,
    scope: AgentSkillScope,
    include_disabled: bool,
) -> Option<AgentSkillItem> {
    for dir in dirs {
        if let Some(found) = locate_existing_skill(dir, kind, skill_id, scope, true) {
            return Some(found);
        }
    }
    if include_disabled {
        for dir in dirs {
            let disabled_dir = disabled_skills_dir(dir);
            if let Some(found) = locate_existing_skill(&disabled_dir, kind, skill_id, scope, false)
            {
                return Some(found);
            }
        }
    }
    None
}

fn locate_disabled_skill_across_dirs(
    dirs: &[PathBuf],
    kind: SkillStorageKind,
    skill_id: &str,
    scope: AgentSkillScope,
) -> Option<AgentSkillItem> {
    for dir in dirs {
        let disabled_dir = disabled_skills_dir(dir);
        if let Some(found) = locate_existing_skill(&disabled_dir, kind, skill_id, scope, false) {
            return Some(found);
        }
    }
    None
}

fn set_skill_read_only(agent_type: AgentType, skill: &mut AgentSkillItem) {
    if is_read_only_skill_path(agent_type, Path::new(&skill.path)) {
        skill.read_only = true;
    }
}

fn disabled_path_for_active_skill(skill: &AgentSkillItem) -> Result<PathBuf, AcpError> {
    let active_path = PathBuf::from(&skill.path);
    let active_dir = active_path
        .parent()
        .ok_or_else(|| AcpError::protocol("skill path has no parent directory"))?;
    let disabled_dir = disabled_skills_dir(active_dir);
    Ok(match skill.layout {
        AgentSkillLayout::SkillDirectory => disabled_dir.join(&skill.id),
        AgentSkillLayout::MarkdownFile => disabled_dir.join(format!("{}.md", skill.id)),
    })
}

fn active_path_for_disabled_skill(skill: &AgentSkillItem) -> Result<PathBuf, AcpError> {
    let disabled_path = PathBuf::from(&skill.path);
    let disabled_dir = disabled_path
        .parent()
        .ok_or_else(|| AcpError::protocol("disabled skill path has no parent directory"))?;
    if disabled_dir.file_name().and_then(|name| name.to_str()) != Some(DISABLED_SKILLS_DIR) {
        return Err(AcpError::protocol("skill is not in the disabled store"));
    }
    let active_dir = disabled_dir
        .parent()
        .ok_or_else(|| AcpError::protocol("disabled store has no parent directory"))?;
    Ok(match skill.layout {
        AgentSkillLayout::SkillDirectory => active_dir.join(&skill.id),
        AgentSkillLayout::MarkdownFile => active_dir.join(format!("{}.md", skill.id)),
    })
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentRuntimeConfig {
    #[serde(default, alias = "api_base_url")]
    api_base_url: Option<String>,
    #[serde(default, alias = "api_key")]
    api_key: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
}

fn trim_non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

/// Primary env var keys for each agent type: (api_base_url, api_key, model).
/// Shared by runtime env resolution, model-provider cascade, and config patching.
fn agent_env_keys(agent_type: AgentType) -> (&'static str, &'static str, &'static str) {
    match agent_type {
        AgentType::ClaudeCode => (
            "ANTHROPIC_BASE_URL",
            "ANTHROPIC_AUTH_TOKEN",
            "ANTHROPIC_MODEL",
        ),
        AgentType::Gemini => ("GOOGLE_GEMINI_BASE_URL", "GEMINI_API_KEY", "GEMINI_MODEL"),
        // Kimi Code does NOT read shell KIMI_API_KEY/OPENAI_API_KEY; the only
        // non-interactive credential path is the `KIMI_MODEL_*` family, which
        // also takes priority over `~/.kimi-code/config.toml`.
        AgentType::KimiCode => (
            "KIMI_MODEL_BASE_URL",
            "KIMI_MODEL_API_KEY",
            "KIMI_MODEL_NAME",
        ),
        AgentType::CodeBuddy => ("CODEBUDDY_BASE_URL", "CODEBUDDY_API_KEY", "CODEBUDDY_MODEL"),
        AgentType::Grok => ("GROK_XAI_API_BASE_URL", "XAI_API_KEY", "GROK_DEFAULT_MODEL"),
        _ => ("OPENAI_BASE_URL", "OPENAI_API_KEY", "OPENAI_MODEL"),
    }
}

/// Serialize a BTreeMap into env_json for database storage.
/// Returns `None` when the map is empty.
fn serialize_env_map(env: &BTreeMap<String, String>) -> Result<Option<String>, AcpError> {
    if env.is_empty() {
        Ok(None)
    } else {
        serde_json::to_string(env)
            .map(Some)
            .map_err(|e| AcpError::protocol(e.to_string()))
    }
}

pub(crate) fn build_runtime_env_from_setting(
    agent_type: AgentType,
    setting: Option<&crate::db::entities::agent_setting::Model>,
    local_config_json: Option<&str>,
) -> BTreeMap<String, String> {
    let mut merged = setting
        .and_then(|model| model.env_json.as_deref())
        .and_then(|raw| serde_json::from_str::<BTreeMap<String, String>>(raw).ok())
        .unwrap_or_default();

    let Some(raw_config_json) = local_config_json else {
        return merged;
    };
    let Ok(config) = serde_json::from_str::<AgentRuntimeConfig>(raw_config_json) else {
        return merged;
    };

    for (key, value) in config.env {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        merged.insert(key, trimmed.to_string());
    }

    let (api_base_url_key, api_key_key, model_key) = agent_env_keys(agent_type);
    if let Some(value) = trim_non_empty(config.api_base_url) {
        merged.insert(api_base_url_key.to_string(), value);
    }
    if let Some(value) = trim_non_empty(config.api_key) {
        merged.insert(api_key_key.to_string(), value);
    }
    if agent_type != AgentType::ClaudeCode {
        if let Some(value) = trim_non_empty(config.model) {
            merged.insert(model_key.to_string(), value);
        }
    }

    merged
}

fn managed_profile_env_keys(agent_type: AgentType) -> &'static [&'static str] {
    match agent_type {
        AgentType::ClaudeCode => &["CLAUDE_CONFIG_DIR"],
        AgentType::Codex => &["CODEX_HOME"],
        AgentType::Gemini => &["GEMINI_CLI_HOME"],
        AgentType::OpenClaw => &["OPENCLAW_HOME", "OPENCLAW_STATE_DIR"],
        AgentType::OpenCode => &["XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME"],
        AgentType::Cline => &["CLINE_DIR"],
        AgentType::Hermes => &["HERMES_HOME"],
        AgentType::CodeBuddy => &["CODEBUDDY_CONFIG_DIR"],
        AgentType::KimiCode => &["KIMI_CODE_HOME"],
        AgentType::Pi => &[
            "PI_ACP_PI_COMMAND",
            "PI_CODING_AGENT_DIR",
            "PI_CODING_AGENT_SESSION_DIR",
        ],
        AgentType::Grok => &["GROK_HOME"],
    }
}

fn remove_managed_profile_env(agent_type: AgentType, runtime_env: &mut BTreeMap<String, String>) {
    let protected = managed_profile_env_keys(agent_type);
    runtime_env.retain(|key, _| {
        !protected
            .iter()
            .any(|candidate| key.eq_ignore_ascii_case(candidate))
    });
}

/// Claude Code provider-model JSON keys → ANTHROPIC_*_MODEL env var names.
const CLAUDE_MODEL_KEY_MAP: &[(&str, &str)] = &[
    ("main", "ANTHROPIC_MODEL"),
    ("reasoning", "ANTHROPIC_REASONING_MODEL"),
    ("haiku", "ANTHROPIC_DEFAULT_HAIKU_MODEL"),
    ("sonnet", "ANTHROPIC_DEFAULT_SONNET_MODEL"),
    ("opus", "ANTHROPIC_DEFAULT_OPUS_MODEL"),
    // The custom model option trio appends one entry to the in-session /model
    // picker (a model the provider's gateway serves). Carried by the provider's
    // model JSON like the five model fields, so binding/cascade pushes it too.
    ("customOption", "ANTHROPIC_CUSTOM_MODEL_OPTION"),
    ("customOptionName", "ANTHROPIC_CUSTOM_MODEL_OPTION_NAME"),
    (
        "customOptionDescription",
        "ANTHROPIC_CUSTOM_MODEL_OPTION_DESCRIPTION",
    ),
];

/// Parse the model field stored on a model_provider into the env-var actions to
/// apply on the dependent agent's `env_json` / local config file.
///
/// The provider's model field is authoritative: every env key relevant to the
/// agent type is returned, with `Some(value)` meaning "set" and `None` meaning
/// "clear". This lets the caller overwrite even when the provider's value is
/// empty.
///
/// - Claude: returns one entry per `CLAUDE_MODEL_KEY_MAP` row — the five
///   ANTHROPIC_*_MODEL fields plus the ANTHROPIC_CUSTOM_MODEL_OPTION trio. Each
///   entry is `None` when the provider's JSON omits that key or has an empty
///   value.
/// - Gemini: returns `GEMINI_MODEL`.
/// - Codex: returns `OPENAI_MODEL` so the provider can override env_json (the
///   root `model` in `config.toml` is handled separately by
///   `provider_codex_model_action`).
/// - CodeBuddy: returns `CODEBUDDY_MODEL`.
/// - Others: returns `OPENAI_MODEL`.
pub(crate) fn parse_provider_model(
    agent_type: AgentType,
    raw: Option<&str>,
) -> BTreeMap<String, Option<String>> {
    let mut out: BTreeMap<String, Option<String>> = BTreeMap::new();
    let trimmed_raw = raw.map(str::trim).filter(|s| !s.is_empty());
    match agent_type {
        AgentType::ClaudeCode => {
            let parsed = trimmed_raw
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
                .and_then(|v| v.as_object().cloned());
            for (key, env_name) in CLAUDE_MODEL_KEY_MAP {
                let value = parsed
                    .as_ref()
                    .and_then(|obj| obj.get(*key))
                    .and_then(|x| x.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                out.insert((*env_name).to_string(), value);
            }
        }
        AgentType::Gemini => {
            out.insert("GEMINI_MODEL".to_string(), trimmed_raw.map(str::to_string));
        }
        // Kimi reads its model name from KIMI_MODEL_NAME (the `KIMI_MODEL_*`
        // family), not OPENAI_MODEL — see `agent_env_keys`.
        AgentType::KimiCode => {
            out.insert(
                "KIMI_MODEL_NAME".to_string(),
                trimmed_raw.map(str::to_string),
            );
        }
        AgentType::CodeBuddy => {
            out.insert(
                "CODEBUDDY_MODEL".to_string(),
                trimmed_raw.map(str::to_string),
            );
        }
        _ => {
            out.insert("OPENAI_MODEL".to_string(), trimmed_raw.map(str::to_string));
        }
    }
    out
}

/// Action to apply to the Codex `config.toml` root `model` key.
pub(crate) enum CodexModelAction {
    /// Not a Codex agent — leave the toml untouched.
    NoOp,
    /// Set the `model` key to this value.
    Set(String),
    /// Remove the `model` key.
    Clear,
}

pub(crate) fn provider_codex_model_action(
    agent_type: AgentType,
    raw: Option<&str>,
) -> CodexModelAction {
    if agent_type != AgentType::Codex {
        return CodexModelAction::NoOp;
    }
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        Some(v) => CodexModelAction::Set(v.to_string()),
        None => CodexModelAction::Clear,
    }
}

/// Update on-disk config files for a single agent when model provider credentials change.
/// Uses `agent_env_keys` to determine the correct env var names per agent type.
///
/// For `model_env`: entries with `Some(value)` are written; entries with `None`
/// are explicitly cleared (overwritten with empty string in the env-patch, so
/// `persist_agent_local_config_json` removes them).
fn cascade_update_agent_config(
    agent_type: AgentType,
    api_url: &str,
    api_key: &str,
    model_env: &BTreeMap<String, Option<String>>,
    codex_model: &CodexModelAction,
) -> Result<(), AcpError> {
    let _paths = require_private_agent_storage_for_write()?;
    let (url_key, key_key, _) = agent_env_keys(agent_type);
    match agent_type {
        AgentType::ClaudeCode | AgentType::Gemini => {
            // Write into config.env (not root-level). For model entries, use
            // JSON-null for "clear" — `merge_json_values` interprets null as
            // "remove this key".
            let mut env = serde_json::Map::new();
            env.insert(
                url_key.to_string(),
                serde_json::Value::String(api_url.to_string()),
            );
            env.insert(
                key_key.to_string(),
                serde_json::Value::String(api_key.to_string()),
            );
            for (k, v) in model_env {
                let value = match v {
                    Some(s) => serde_json::Value::String(s.clone()),
                    None => serde_json::Value::Null,
                };
                env.insert(k.clone(), value);
            }
            let patch = serde_json::json!({ "env": env });
            let patch_str =
                serde_json::to_string(&patch).map_err(|e| AcpError::protocol(e.to_string()))?;
            persist_agent_local_config_json(agent_type, Some(&patch_str))?;
        }
        AgentType::OpenClaw => {
            // agent_local_config_path returns None for OpenClaw — no-op
        }
        AgentType::Hermes => {
            // Hermes self-manages credentials in ~/.hermes/.env via
            // `hermes model` / `hermes setup`; iyw-claw writes no provider creds.
        }
        AgentType::Codex => {
            let auth_path = codex_auth_json_path();
            let mut auth_obj = if auth_path.exists() {
                fs::read_to_string(&auth_path)
                    .ok()
                    .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
                    .filter(|v| v.is_object())
                    .unwrap_or_else(|| serde_json::json!({}))
            } else {
                serde_json::json!({})
            };
            if !api_key.trim().is_empty() {
                auth_obj[key_key] = serde_json::Value::String(api_key.to_string());
            }
            let auth_str = serde_json::to_string_pretty(&auth_obj)
                .map_err(|e| AcpError::protocol(e.to_string()))?;

            let config_path = codex_config_toml_path();
            let mut toml_value = if config_path.exists() {
                fs::read_to_string(&config_path)
                    .ok()
                    .and_then(|raw| raw.parse::<toml::Value>().ok())
                    .filter(|v| v.is_table())
                    .unwrap_or_else(|| toml::Value::Table(toml::map::Map::new()))
            } else {
                toml::Value::Table(toml::map::Map::new())
            };
            let table = toml_value
                .as_table_mut()
                .ok_or_else(|| AcpError::protocol("codex config root must be a TOML table"))?;
            table.remove("api_base_url");

            let provider_name = table
                .get("model_provider")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| "iyw-claw".to_string());
            table.insert(
                "model_provider".to_string(),
                toml::Value::String(provider_name.clone()),
            );

            let providers_item = table
                .entry("model_providers".to_string())
                .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
            if !providers_item.is_table() {
                *providers_item = toml::Value::Table(toml::map::Map::new());
            }
            let providers = providers_item
                .as_table_mut()
                .ok_or_else(|| AcpError::protocol("invalid model_providers table"))?;
            let provider_item = providers
                .entry(provider_name.clone())
                .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
            if !provider_item.is_table() {
                *provider_item = toml::Value::Table(toml::map::Map::new());
            }
            let provider_table = provider_item
                .as_table_mut()
                .ok_or_else(|| AcpError::protocol("invalid model provider table"))?;
            if api_url.trim().is_empty() {
                provider_table.remove("base_url");
            } else {
                provider_table.insert(
                    "base_url".to_string(),
                    toml::Value::String(api_url.to_string()),
                );
            }
            if provider_name == "iyw-claw" {
                provider_table.insert(
                    "name".to_string(),
                    toml::Value::String("iyw-claw".to_string()),
                );
                provider_table.insert(
                    "wire_api".to_string(),
                    toml::Value::String("responses".to_string()),
                );
                provider_table.insert(
                    "requires_openai_auth".to_string(),
                    toml::Value::Boolean(true),
                );
            }
            match codex_model {
                CodexModelAction::Set(model) => {
                    table.insert("model".to_string(), toml::Value::String(model.to_string()));
                }
                CodexModelAction::Clear => {
                    table.remove("model");
                }
                CodexModelAction::NoOp => {}
            }
            let toml_str = toml::to_string_pretty(&toml_value)
                .map_err(|e| AcpError::protocol(e.to_string()))?;

            persist_codex_native_config_files(Some(&auth_str), Some(&toml_str), None)?;
        }
        AgentType::OpenCode => {
            let auth_path = opencode_auth_json_path();
            let mut auth_obj = if auth_path.exists() {
                fs::read_to_string(&auth_path)
                    .ok()
                    .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
                    .filter(|v| v.is_object())
                    .unwrap_or_else(|| serde_json::json!({}))
            } else {
                serde_json::json!({})
            };
            if !api_key.trim().is_empty() {
                auth_obj["api_key"] = serde_json::Value::String(api_key.to_string());
            }
            let auth_str = serde_json::to_string_pretty(&auth_obj)
                .map_err(|e| AcpError::protocol(e.to_string()))?;
            persist_opencode_auth_json(&auth_str)?;

            let patch = serde_json::json!({ "apiBaseUrl": api_url });
            let patch_str =
                serde_json::to_string(&patch).map_err(|e| AcpError::protocol(e.to_string()))?;
            persist_agent_local_config_json(agent_type, Some(&patch_str))?;
        }
        AgentType::Cline => {}
        AgentType::CodeBuddy => {
            // CodeBuddy authenticates via env vars (CODEBUDDY_API_KEY /
            // CODEBUDDY_INTERNET_ENVIRONMENT) managed by its dedicated settings
            // panel through `acpUpdateAgentEnv`; it does not participate in the
            // model-provider credential cascade.
        }
        AgentType::KimiCode => {
            // Kimi Code authenticates via the `KIMI_MODEL_*` env family
            // (KIMI_MODEL_API_KEY / KIMI_MODEL_BASE_URL / KIMI_MODEL_NAME)
            // managed by its dedicated settings panel through `acpUpdateAgentEnv`;
            // it does not participate in the model-provider credential cascade.
        }
        AgentType::Pi => {
            // Pi authenticates via its own `~/.pi/agent/auth.json` + model
            // selection in `~/.pi/agent/settings.json`, managed by the dedicated
            // Pi settings panel (`acp_update_pi_config`); it does not participate
            // in the model-provider credential cascade.
        }
        AgentType::Grok => {
            // Grok receives gateway credentials through its launch environment;
            // never overwrite its native config.toml or cached login state.
        }
    }
    Ok(())
}

/// Cascade model provider changes (credentials + model) to all dependent agent settings
/// and config files.
pub(crate) async fn cascade_update_model_provider(
    db: &AppDatabase,
    provider_id: i32,
    new_api_url: &str,
    new_api_key: &str,
    new_model: Option<&str>,
    emitter: &EventEmitter,
) -> Result<(), AcpError> {
    let dependents = agent_setting_service::find_by_model_provider_id(&db.conn, provider_id)
        .await
        .map_err(|e| AcpError::protocol(e.to_string()))?;

    for setting in &dependents {
        let agent_type: AgentType = match serde_json::from_str(&setting.agent_type) {
            Ok(at) => at,
            Err(_) => continue,
        };

        // 1. Update env_json in database (uses agent_env_keys for consistent key names)
        let (url_key, key_key, _) = agent_env_keys(agent_type);
        let mut env_map: BTreeMap<String, String> = setting
            .env_json
            .as_deref()
            .and_then(|raw| serde_json::from_str(raw).ok())
            .unwrap_or_default();

        if !new_api_url.trim().is_empty() {
            env_map.insert(url_key.to_string(), new_api_url.to_string());
        }
        if !new_api_key.trim().is_empty() {
            env_map.insert(key_key.to_string(), new_api_key.to_string());
        }

        let model_env = parse_provider_model(agent_type, new_model);
        for (k, v) in &model_env {
            match v {
                Some(value) => {
                    env_map.insert(k.clone(), value.clone());
                }
                None => {
                    env_map.remove(k);
                }
            }
        }

        let patch = agent_setting_service::AgentSettingsUpdate {
            enabled: setting.enabled,
            env_json: serialize_env_map(&env_map)?,
            model_provider_id: setting.model_provider_id,
        };
        agent_setting_service::update(&db.conn, agent_type, patch)
            .await
            .map_err(|e| AcpError::protocol(e.to_string()))?;

        // 2. Update on-disk config files
        let codex_action = provider_codex_model_action(agent_type, new_model);
        if let Err(e) = cascade_update_agent_config(
            agent_type,
            new_api_url,
            new_api_key,
            &model_env,
            &codex_action,
        ) {
            tracing::warn!(
                "[ModelProvider] cascade_update_agent_config({agent_type}) failed: {e}, skipping config update"
            );
        }

        emit_acp_agents_updated(emitter, "env_updated", Some(agent_type));
    }
    Ok(())
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_preflight(
    agent_type: AgentType,
    force_refresh: Option<bool>,
) -> Result<PreflightResult, AcpError> {
    if force_refresh.unwrap_or(false) {
        preflight::clear_npm_env_cache();
    }
    Ok(preflight::run_preflight(agent_type).await)
}

async fn reconcile_agent_skills_before_launch(db: &AppDatabase, agent_type: AgentType) {
    if let Err(error) =
        crate::commands::agent_storage::ensure_active_agent_profile_layout(&db.conn, agent_type)
            .await
    {
        tracing::warn!(
            agent_type = %agent_type,
            error = %error,
            "[skills] failed to initialize Agent profile before launch"
        );
    }
    let install_report = crate::commands::experts::ensure_central_experts_installed().await;
    if !install_report.errors.is_empty() {
        tracing::warn!(
            agent_type = %agent_type,
            errors = ?install_report.errors,
            "[skills] central skill installation before Agent launch was incomplete"
        );
    }
    if let Err(error) =
        crate::commands::managed_skills::reconcile_agent_core(&db.conn, agent_type, true).await
    {
        tracing::warn!(
            agent_type = %agent_type,
            error = %error,
            "[skills] managed skill reconcile before Agent launch failed"
        );
    }
    if let Err(error) = reconcile_shared_market_skills_for_agent(agent_type) {
        tracing::warn!(
            agent_type = %agent_type,
            error = %error,
            "[skills] central skill reconcile before Agent launch failed"
        );
    }
}

/// Resolve the full runtime env every ACP spawn should receive — settings
/// override, model provider credentials, git credential helper, OpenClaw
/// reset flag. Returns `AcpError::protocol("...disabled in settings")` when
/// the user has disabled the agent.
///
/// This is the **single source of truth** for "what env does an agent
/// process see". Three call sites depend on it:
///
///   1. `acp_connect` — the user-initiated session entry point.
///   2. `ConnectionManagerSpawner::spawn` — used by the delegation broker
///      to spawn subagents. Before this helper existed, delegation passed
///      `BTreeMap::new()`, silently bypassing settings/credentials and
///      letting disabled agents still be invoked through delegation.
///   3. `probe_agent_options` — the live settings-page probe. Must match
///      delegation's env exactly so what the user sees in the panel is
///      what `delegate_to_agent` will actually receive.
///
/// Diverging any of these from the others reintroduces the
/// "[UI shows options] != [delegation gets options]" inconsistency that
/// the multi-agent settings panel was designed to prevent.
pub(crate) async fn build_session_runtime_env(
    db: &AppDatabase,
    agent_type: AgentType,
    session_id: Option<&str>,
    data_dir: &Path,
) -> Result<BTreeMap<String, String>, AcpError> {
    let paths = active_agent_storage_paths()?;
    if !crate::acp::agent_storage::startup_profile_env_is_complete(&paths, |key| {
        std::env::var_os(key)
    }) {
        return Err(AcpError::SdkNotInstalled(
            "Agent profile environment is not active. Restart iyw-claw before launching Agents."
                .to_string(),
        ));
    }
    if let Some(config) = crate::acp::agent_storage::load_config(&db.conn)
        .await
        .map_err(|error| AcpError::protocol(error.to_string()))?
    {
        if !crate::acp::agent_storage::startup_profile_env_matches(&paths, &config, |key| {
            std::env::var_os(key)
        }) {
            return Err(AcpError::protocol(
                "Agent storage settings changed. Restart iyw-claw before launching Agents."
                    .to_string(),
            ));
        }
    }
    let setting = agent_setting_service::get_by_agent_type(&db.conn, agent_type)
        .await
        .map_err(|e| AcpError::protocol(e.to_string()))?;
    let installed = setting
        .as_ref()
        .and_then(|model| model.installed_version.as_deref())
        .is_some_and(|version| !version.is_empty());
    if !installed {
        return Err(AcpError::SdkNotInstalled(format!(
            "{agent_type} is not installed"
        )));
    }
    let disabled = setting
        .as_ref()
        .map(|model| !model.enabled)
        .unwrap_or(false);
    if disabled {
        return Err(AcpError::protocol(format!(
            "{agent_type} is disabled in settings"
        )));
    }

    reconcile_agent_skills_before_launch(db, agent_type).await;

    crate::acp::provider_overlay::enforce_active_provider_overlay(agent_type)
        .map_err(AcpError::protocol)?;
    if agent_type == AgentType::Codex {
        ensure_codex_model_catalog()?;
    }

    let local_config_json = load_agent_local_config_json(agent_type);
    let mut runtime_env =
        build_runtime_env_from_setting(agent_type, setting.as_ref(), local_config_json.as_deref());
    remove_managed_profile_env(agent_type, &mut runtime_env);
    crate::acp::provider_overlay::apply_provider_runtime_env(agent_type, &mut runtime_env);
    crate::acp::account_credentials::inject_runtime_credential_for_acp(
        &db.conn,
        agent_type,
        &mut runtime_env,
    )
    .await?;
    crate::acp::runtime_context::prepend_tool_dirs(Some(&paths), &mut runtime_env);
    crate::acp::automation_tools::inject_scheduled_task_env(agent_type, &mut runtime_env);
    runtime_env.remove(MANAGED_AGENT_VERSION_ENV);
    if let Some(version) = setting
        .as_ref()
        .and_then(|model| model.installed_version.as_deref())
        .map(str::trim)
        .filter(|version| !version.is_empty())
    {
        runtime_env.insert(MANAGED_AGENT_VERSION_ENV.to_string(), version.to_string());
        if agent_type == AgentType::Pi {
            let paths = active_agent_storage_paths()?;
            let pi_command = if let Some(command) =
                npm_runtime::resolve_private_npm_command(&paths, agent_type, version, "pi")
            {
                command
            } else {
                npm_runtime::preferred_private_npm_command_path(&paths, agent_type, version, "pi")?
            };
            runtime_env.insert(
                "PI_ACP_PI_COMMAND".to_string(),
                pi_command.to_string_lossy().into_owned(),
            );
        }
    }

    // codex resume no longer needs a `MODEL_PROVIDER` pin: codex-acp 1.0.1
    // (#224) resolves the resumed provider from `~/.codex/config.toml` via
    // `config/read`, matching new sessions (which pass `null` so codex reads the
    // config's own `model_provider`). The 1.0.0 workaround that injected
    // `MODEL_PROVIDER` to stop resumed sessions falling back to "openai" is now
    // redundant and was removed.

    if let Some(cred_env) = crate::commands::terminal::prepare_credential_env(data_dir) {
        for (key, value) in cred_env {
            runtime_env.insert(key, value);
        }
    }

    if agent_type == AgentType::OpenClaw && session_id.is_none() {
        runtime_env.insert("OPENCLAW_RESET_SESSION".into(), "1".into());
    }

    Ok(runtime_env)
}

/// Per-launch env keys that vary by session/run but don't represent user
/// config, so they're excluded from the config fingerprint. Without this, a
/// session-id-derived value would flip the fingerprint the moment a real
/// session id is assigned and make every session look "stale". Currently only
/// OpenClaw's reset flag (set iff `session_id` is None at spawn).
fn is_volatile_fingerprint_key(key: &str) -> bool {
    key == "OPENCLAW_RESET_SESSION"
}

/// Fingerprint the effective config a spawned agent process is locked to: the
/// resolved `runtime_env` (minus per-launch volatile keys) plus the raw content
/// of the agent's native config file(s). Both surfaces only take effect at
/// process start, so a change to either is exactly what "this running session is
/// stale" means. The digest is process-local — never persisted, never sent on
/// the wire (only the resulting `stale` bool reaches the frontend) — so a
/// non-cryptographic hash would do; SHA-256 keeps it deterministic and matches
/// the rest of the codebase.
pub(crate) fn fingerprint_config(
    agent_type: AgentType,
    runtime_env: &BTreeMap<String, String>,
) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    // BTreeMap iterates in sorted key order → deterministic across calls.
    for (k, v) in runtime_env {
        if is_volatile_fingerprint_key(k) {
            continue;
        }
        hasher.update(k.as_bytes());
        hasher.update([0u8]);
        hasher.update(v.as_bytes());
        hasher.update([0u8]);
    }
    hasher.update(b"\x01native\x01");
    if let Some(native) = load_agent_local_config_json(agent_type) {
        hasher.update(native.as_bytes());
    }
    // Grok's native TOML is edited through its settings surface and is not
    // represented by the generic JSON projection above. Include it explicitly
    // so model, MCP, and permission changes mark running sessions stale.
    if agent_type == AgentType::Grok {
        hasher.update(b"\x01grok_toml\x01");
        if let Some(toml) = load_grok_config_toml_raw() {
            hasher.update(toml.as_bytes());
        }
    }
    format!("{:x}", hasher.finalize())
}

/// Recompute the canonical config fingerprint for `agent_type` from current
/// settings (DB + native config files), independent of any running session.
/// Passes `session_id = None` so the result is session-independent (the only
/// session-derived key is excluded anyway), making it directly comparable to
/// the fingerprint `fingerprint_config` produced at spawn time. Propagates the
/// agent's "disabled in settings" error verbatim.
pub(crate) async fn compute_session_config_fingerprint(
    db: &AppDatabase,
    agent_type: AgentType,
    data_dir: &Path,
) -> Result<String, AcpError> {
    let runtime_env = build_session_runtime_env(db, agent_type, None, data_dir).await?;
    Ok(fingerprint_config(agent_type, &runtime_env))
}

/// After a settings save, recompute the effective config fingerprint for each of
/// `agent_types` and tell every running connection of those agents whether it
/// has drifted onto stale (launch-time) config. Best-effort: an agent whose
/// fingerprint can't be recomputed (e.g. it was just disabled) is skipped, not
/// fatal. Returns the number of running connections currently on stale config
/// across the affected agents — for the settings-side "N sessions need restart"
/// toast.
pub(crate) async fn refresh_config_staleness(
    manager: &ConnectionManager,
    db: &AppDatabase,
    data_dir: &Path,
    agent_types: &[AgentType],
    kind: ConfigStaleKind,
) -> usize {
    let mut fresh: HashMap<AgentType, String> = HashMap::new();
    for &agent_type in agent_types {
        if fresh.contains_key(&agent_type) {
            continue;
        }
        if let Ok(fp) = compute_session_config_fingerprint(db, agent_type, data_dir).await {
            fresh.insert(agent_type, fp);
        }
    }
    if fresh.is_empty() {
        return 0;
    }
    manager.refresh_connection_staleness(&fresh, kind).await
}

/// `acp_update_agent_env_core` followed by a staleness refresh. Shared by the
/// Tauri command and the web handler so both report how many running sessions
/// the save left on stale config. Returns that count.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn acp_update_agent_env_and_refresh(
    agent_type: AgentType,
    enabled: bool,
    env: BTreeMap<String, String>,
    model_provider_id: Option<i32>,
    db: &AppDatabase,
    manager: &ConnectionManager,
    data_dir: &Path,
    emitter: &EventEmitter,
) -> Result<usize, AcpError> {
    acp_update_agent_env_core(agent_type, enabled, env, model_provider_id, db, emitter).await?;
    Ok(refresh_config_staleness(
        manager,
        db,
        data_dir,
        &[agent_type],
        ConfigStaleKind::AgentConfig,
    )
    .await)
}

/// `acp_update_agent_preferences_core` followed by a staleness refresh. Shared
/// by the Tauri command and the web handler; returns the count of running
/// sessions left on stale config.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn acp_update_agent_preferences_and_refresh(
    agent_type: AgentType,
    enabled: bool,
    env: BTreeMap<String, String>,
    config_json: Option<String>,
    opencode_auth_json: Option<String>,
    codex_auth_json: Option<String>,
    codex_config_toml: Option<String>,
    db: &AppDatabase,
    manager: &ConnectionManager,
    data_dir: &Path,
    emitter: &EventEmitter,
) -> Result<usize, AcpError> {
    acp_update_agent_preferences_core(
        agent_type,
        enabled,
        env,
        config_json,
        opencode_auth_json,
        codex_auth_json,
        codex_config_toml,
        db,
        emitter,
    )
    .await?;
    Ok(refresh_config_staleness(
        manager,
        db,
        data_dir,
        &[agent_type],
        ConfigStaleKind::AgentConfig,
    )
    .await)
}

pub(crate) async fn ensure_acp_working_dir(
    data_dir: &Path,
    working_dir: Option<&str>,
) -> Result<(), AcpError> {
    let Some(raw) = working_dir.filter(|value| !value.trim().is_empty()) else {
        return Ok(());
    };
    let path = PathBuf::from(raw);
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    if path.is_dir() {
        return Ok(());
    }
    if path.exists() {
        return Err(AcpError::SpawnFailed(format!(
            "working directory is not a directory: {}",
            path.display()
        )));
    }
    if crate::commands::chat_attachments::is_managed_chat_dir(data_dir, &path) {
        crate::commands::chat_attachments::ensure_managed_chat_dir(data_dir, &path)
            .await
            .map_err(|error| {
                AcpError::SpawnFailed(format!(
                    "failed to restore managed Chat working directory {}: {error}",
                    path.display()
                ))
            })?;
        return Ok(());
    }
    Err(AcpError::SpawnFailed(format!(
        "working directory does not exist: {}",
        path.display()
    )))
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
#[allow(clippy::too_many_arguments)]
pub async fn acp_connect(
    agent_type: AgentType,
    working_dir: Option<String>,
    session_id: Option<String>,
    preferred_mode_id: Option<String>,
    preferred_config_values: Option<BTreeMap<String, String>>,
    manager: State<'_, ConnectionManager>,
    db: State<'_, AppDatabase>,
    app_handle: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<String, AcpError> {
    // Resolve through the effective data dir so a custom `IYW_CLAW_DATA_DIR`
    // reaches the credential helper script the agent's git subprocess
    // will execute. `acp_connect` may be called before the app data dir
    // exists on disk (first launch); fall back to a sentinel that the
    // credential helper treats as "no credentials configured".
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map(|p| crate::paths::resolve_effective_data_dir(&p))
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    ensure_acp_working_dir(&app_data_dir, working_dir.as_deref()).await?;
    let runtime_env =
        build_session_runtime_env(&db, agent_type, session_id.as_deref(), &app_data_dir).await?;

    // Guard: the session page must never trigger a download or install.
    // If the agent isn't ready, return SdkNotInstalled here so the frontend
    // can prompt the user to install it from Agent Settings.
    verify_agent_installed(agent_type, &runtime_env)?;
    crate::acp::account_credentials::sync_agent_credentials_for_acp(&db.conn, agent_type).await?;

    let emitter = EventEmitter::Tauri(app_handle);
    manager
        .spawn_agent(
            agent_type,
            working_dir,
            session_id,
            runtime_env,
            window.label().to_string(),
            emitter,
            preferred_mode_id,
            preferred_config_values.unwrap_or_default(),
        )
        .await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_prompt(
    connection_id: String,
    blocks: Vec<PromptInputBlock>,
    folder_id: Option<i32>,
    conversation_id: Option<i32>,
    client_message_id: Option<String>,
    db: State<'_, crate::db::AppDatabase>,
    manager: State<'_, ConnectionManager>,
) -> Result<(), AcpError> {
    manager
        .send_prompt_linked_with_message_id(
            &db,
            &connection_id,
            blocks,
            folder_id,
            conversation_id,
            None,
            client_message_id,
        )
        .await
        .map(|_| ())
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_set_mode(
    connection_id: String,
    mode_id: String,
    manager: State<'_, ConnectionManager>,
) -> Result<(), AcpError> {
    manager.set_mode(&connection_id, mode_id).await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_set_config_option(
    connection_id: String,
    config_id: String,
    value_id: String,
    manager: State<'_, ConnectionManager>,
) -> Result<(), AcpError> {
    manager
        .set_config_option(&connection_id, config_id, value_id)
        .await
}

/// Spawn a transient ACP connection for `agent_type` with a silent emitter,
/// read whatever `SessionConfigOptions` / `SessionModes` the agent advertises,
/// and tear it down. The returned snapshot drives the delegation-settings UI
/// so the user picks from the exact option set the agent will accept when
/// iyw-claw-mcp later spawns a subagent.
///
/// Does NOT touch the chat-side `selectorsCache`, `localStorage` preferences,
/// or any active connection state — see `ConnectionManager::probe_agent_options`
/// for the isolation guarantees.
pub async fn acp_describe_agent_options_core(
    manager: &ConnectionManager,
    db: &AppDatabase,
    data_dir: &Path,
    agent_type: AgentType,
    working_dir: Option<String>,
) -> Result<crate::acp::types::AgentOptionsSnapshot, AcpError> {
    // Build the same runtime env delegation/acp_connect would build so
    // probe sees exactly what `delegate_to_agent` will see at runtime.
    // Without this, the settings UI could show options that the agent
    // never advertises in production (settings override an API URL,
    // model_provider injects a different model list, etc.).
    let runtime_env = build_session_runtime_env(db, agent_type, None, data_dir).await?;
    verify_agent_installed(agent_type, &runtime_env)?;
    crate::acp::account_credentials::sync_agent_credentials_for_acp(&db.conn, agent_type).await?;
    manager
        .probe_agent_options(agent_type, working_dir, runtime_env)
        .await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_describe_agent_options(
    agent_type: AgentType,
    working_dir: Option<String>,
    manager: State<'_, ConnectionManager>,
    db: State<'_, AppDatabase>,
    app_handle: tauri::AppHandle,
) -> Result<crate::acp::types::AgentOptionsSnapshot, AcpError> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map(|p| crate::paths::resolve_effective_data_dir(&p))
        .unwrap_or_else(|_| PathBuf::from("."));
    acp_describe_agent_options_core(&manager, &db, &app_data_dir, agent_type, working_dir).await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_cancel(
    connection_id: String,
    db: State<'_, AppDatabase>,
    manager: State<'_, ConnectionManager>,
) -> Result<(), AcpError> {
    manager.cancel(&db.conn, &connection_id).await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_fork(
    connection_id: String,
    conversation_id: Option<i32>,
    folder_id: Option<i32>,
    db: State<'_, AppDatabase>,
    manager: State<'_, ConnectionManager>,
) -> Result<ForkResultInfo, AcpError> {
    manager
        .fork_session(&db, &connection_id, conversation_id, folder_id)
        .await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_respond_permission(
    connection_id: String,
    request_id: String,
    option_id: String,
    manager: State<'_, ConnectionManager>,
) -> Result<(), AcpError> {
    manager
        .respond_permission(&connection_id, &request_id, &option_id)
        .await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_answer_question(
    connection_id: String,
    question_id: String,
    answer: crate::acp::question::QuestionAnswer,
    manager: State<'_, ConnectionManager>,
) -> Result<(), AcpError> {
    manager
        .answer_question(&connection_id, &question_id, answer)
        .await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_disconnect(
    connection_id: String,
    manager: State<'_, ConnectionManager>,
) -> Result<(), AcpError> {
    manager.disconnect(&connection_id).await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_touch_connection(
    connection_id: String,
    manager: State<'_, ConnectionManager>,
) -> Result<bool, AcpError> {
    Ok(manager.touch(&connection_id).await)
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_list_connections(
    manager: State<'_, ConnectionManager>,
) -> Result<Vec<ConnectionInfo>, AcpError> {
    Ok(manager.list_connections().await)
}

pub(crate) async fn acp_get_session_snapshot_core(
    manager: &ConnectionManager,
    connection_id: &str,
) -> Result<Option<crate::acp::LiveSessionSnapshot>, AcpError> {
    let Some(state) = manager.get_state(connection_id).await else {
        return Ok(None);
    };
    let snap = state.read().await.to_snapshot();
    Ok(Some(snap))
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_get_session_snapshot(
    connection_id: String,
    manager: State<'_, ConnectionManager>,
) -> Result<Option<crate::acp::LiveSessionSnapshot>, AcpError> {
    acp_get_session_snapshot_core(&manager, &connection_id).await
}

pub(crate) async fn acp_get_session_snapshot_by_conversation_core(
    manager: &ConnectionManager,
    conversation_id: i32,
) -> Result<Option<crate::acp::LiveSessionSnapshot>, AcpError> {
    let Some(conn_id) = manager
        .find_connection_by_conversation_id(conversation_id)
        .await
    else {
        return Ok(None);
    };
    acp_get_session_snapshot_core(manager, &conn_id).await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_get_session_snapshot_by_conversation(
    conversation_id: i32,
    manager: State<'_, ConnectionManager>,
) -> Result<Option<crate::acp::LiveSessionSnapshot>, AcpError> {
    acp_get_session_snapshot_by_conversation_core(&manager, conversation_id).await
}

/// Discover the live connection (if any) another client is currently running
/// for this conversation, returning its id plus the current `event_seq`
/// (informational). The frontend calls this when opening a conversation: if
/// `Some`, it attaches to that connection as a viewer (cross-client live
/// streaming) instead of spawning a fresh agent; if `None`, no client is live
/// and it spawns/owns one.
///
/// Matches by `conversation_id` first, then falls back to `session_id`
/// (`external_id`). The fallback is load-bearing: a connection binds its
/// `conversation_id` only on the first prompt, so a historical conversation
/// opened by a second client BEFORE any prompt is sent would miss the
/// by-conversation lookup — and then `acp_connect` would reuse the live owner's
/// connection by `external_id` and the frontend would mis-tag it as a locally
/// owned connection, tearing it down (killing the real owner's agent) on tab
/// close. Discovering it here lets the second client attach as a viewer.
pub(crate) async fn acp_find_connection_for_conversation_core(
    manager: &ConnectionManager,
    conversation_id: i32,
    session_id: Option<&str>,
    agent_type: AgentType,
) -> Result<Option<crate::acp::ConversationConnectionInfo>, AcpError> {
    let connection_id = match manager
        .find_connection_by_conversation_id(conversation_id)
        .await
    {
        Some(id) => id,
        // The `session_id` (external_id) fallback is matched WITH `agent_type`:
        // `external_id` is unique only per agent, so matching it alone could
        // attach a viewer to a different agent's connection sharing a session id.
        None => match session_id {
            Some(sid) if !sid.is_empty() => {
                match manager
                    .find_connection_by_external_id(sid, agent_type)
                    .await
                {
                    Some(id) => id,
                    None => return Ok(None),
                }
            }
            _ => return Ok(None),
        },
    };
    // The connection may be GC'd between the lookup and the state read; treat a
    // missing state as "no live connection" rather than erroring.
    let Some(state) = manager.get_state(&connection_id).await else {
        return Ok(None);
    };
    let s = state.read().await;
    // Discovery means "a LIVE connection a viewer can attach to". Teardown
    // writes a terminal status onto the state BEFORE the cleanup hook removes
    // the map entry (see `acp/connection.rs`), and `find_connection_by_
    // conversation_id` only matches `conversation_id` — so without this guard
    // discovery can briefly hand back a connection that is going away, and the
    // viewer would attach to a dead stream. Treat terminal statuses as "no live
    // connection" (matching `find_connection_for_reuse`'s contract) so the
    // client reads the persisted detail instead.
    if matches!(
        s.status,
        ConnectionStatus::Disconnected | ConnectionStatus::Error
    ) {
        return Ok(None);
    }
    Ok(Some(crate::acp::ConversationConnectionInfo {
        connection_id,
        event_seq: s.event_seq,
    }))
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_find_connection_for_conversation(
    conversation_id: i32,
    session_id: Option<String>,
    agent_type: AgentType,
    manager: State<'_, ConnectionManager>,
) -> Result<Option<crate::acp::ConversationConnectionInfo>, AcpError> {
    acp_find_connection_for_conversation_core(
        &manager,
        conversation_id,
        session_id.as_deref(),
        agent_type,
    )
    .await
}

pub(crate) async fn acp_get_agent_status_core(
    agent_type: AgentType,
    db: &AppDatabase,
) -> Result<crate::acp::types::AcpAgentStatus, AcpError> {
    let storage = AgentStoragePaths::active();
    let platform = registry::current_platform();
    let meta = registry::get_agent_meta(agent_type);
    let setting = agent_setting_service::get_by_agent_type(&db.conn, agent_type)
        .await
        .map_err(|e| AcpError::protocol(e.to_string()))?;

    let (available, installed_version) = match &meta.distribution {
        registry::AgentDistribution::Npx { cmd, .. } => {
            let detected = setting.as_ref().and_then(|model| {
                let version = model.installed_version.as_deref()?;
                let paths = storage.as_ref()?;
                is_cmd_available(paths, agent_type, version, cmd).then_some(())?;
                Some(version.to_string())
            });
            (true, detected)
        }
        registry::AgentDistribution::Binary { platforms, cmd, .. } => {
            let detected = storage.as_ref().and_then(|paths| {
                binary_cache::detect_installed_version(paths, agent_type, cmd)
                    .ok()
                    .flatten()
            });
            (platforms.iter().any(|p| p.platform == platform), detected)
        }
        registry::AgentDistribution::Uvx { system_cmd, .. } => (
            uvx_agent_launchable(*system_cmd),
            storage
                .as_ref()
                .and_then(|paths| binary_cache::uvx_prepared_version(paths, agent_type)),
        ),
    };

    Ok(crate::acp::types::AcpAgentStatus {
        agent_type,
        available,
        enabled: setting.map(|m| m.enabled).unwrap_or(true),
        installed_version,
    })
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_get_agent_status(
    agent_type: AgentType,
    db: tauri::State<'_, AppDatabase>,
) -> Result<crate::acp::types::AcpAgentStatus, AcpError> {
    acp_get_agent_status_core(agent_type, &db).await
}

pub(crate) async fn acp_list_agents_core(db: &AppDatabase) -> Result<Vec<AcpAgentInfo>, AcpError> {
    let platform = registry::current_platform();
    let agent_types = registry::all_acp_agents();

    let defaults = agent_types
        .iter()
        .enumerate()
        .map(
            |(idx, agent_type)| agent_setting_service::AgentDefaultInput {
                agent_type: *agent_type,
                registry_id: registry::registry_id_for(*agent_type).to_string(),
                default_sort_order: idx as i32,
            },
        )
        .collect::<Vec<_>>();

    agent_setting_service::ensure_defaults(&db.conn, &defaults)
        .await
        .map_err(|e| AcpError::protocol(e.to_string()))?;
    let legacy_backend_order = [
        AgentType::ClaudeCode,
        AgentType::Codex,
        AgentType::Gemini,
        AgentType::OpenClaw,
        AgentType::OpenCode,
        AgentType::Cline,
        AgentType::Hermes,
        AgentType::CodeBuddy,
        AgentType::KimiCode,
        AgentType::Pi,
    ];
    let legacy_frontend_order = [
        AgentType::Codex,
        AgentType::ClaudeCode,
        AgentType::OpenCode,
        AgentType::Gemini,
        AgentType::OpenClaw,
        AgentType::Cline,
        AgentType::Hermes,
        AgentType::CodeBuddy,
        AgentType::KimiCode,
        AgentType::Pi,
    ];
    for legacy_order in [&legacy_backend_order[..], &legacy_frontend_order[..]] {
        let migrated = agent_setting_service::reorder_if_current_order_matches(
            &db.conn,
            legacy_order,
            &agent_types,
        )
        .await
        .map_err(|e| AcpError::protocol(e.to_string()))?;
        if migrated {
            agent_setting_service::reset_enabled_to_defaults(&db.conn, &agent_types)
                .await
                .map_err(|e| AcpError::protocol(e.to_string()))?;
            break;
        }
    }
    let settings_map = agent_setting_service::list_map_by_agent_type(&db.conn)
        .await
        .map_err(|e| AcpError::protocol(e.to_string()))?;
    let storage = AgentStoragePaths::active();

    let mut agents = Vec::new();
    for (idx, agent_type) in agent_types.into_iter().enumerate() {
        let setting = settings_map.get(&agent_type);
        let meta = registry::get_agent_meta(agent_type);
        let (available, dist_type, local_installed_version) = match &meta.distribution {
            registry::AgentDistribution::Npx { cmd, .. } => {
                let cached = setting.and_then(|model| {
                    let version = model.installed_version.as_deref()?;
                    let paths = storage.as_ref()?;
                    is_cmd_available(paths, agent_type, version, cmd).then_some(())?;
                    Some(version.to_string())
                });
                (true, "npx", cached)
            }
            registry::AgentDistribution::Binary { platforms, cmd, .. } => {
                let detected = storage.as_ref().and_then(|paths| {
                    binary_cache::detect_installed_version(paths, agent_type, cmd)
                        .ok()
                        .flatten()
                });
                (
                    platforms.iter().any(|p| p.platform == platform),
                    "binary",
                    detected,
                )
            }
            registry::AgentDistribution::Uvx { system_cmd, .. } => (
                uvx_agent_launchable(*system_cmd),
                "uvx",
                storage
                    .as_ref()
                    .and_then(|paths| binary_cache::uvx_prepared_version(paths, agent_type)),
            ),
        };

        let mut env = setting
            .and_then(|m| m.env_json.as_deref())
            .and_then(|s| serde_json::from_str::<BTreeMap<String, String>>(s).ok())
            .unwrap_or_default();
        let local_config_json = load_agent_local_config_json(agent_type);
        if let Some(raw_local_config) = local_config_json.as_deref() {
            if let Ok(local_cfg) = serde_json::from_str::<AgentRuntimeConfig>(raw_local_config) {
                for (key, value) in local_cfg.env {
                    let trimmed = value.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    env.insert(key, trimmed.to_string());
                }
                let (api_base_url_key, api_key_key, model_key) = agent_env_keys(agent_type);
                if let Some(value) = trim_non_empty(local_cfg.api_base_url) {
                    env.insert(api_base_url_key.to_string(), value);
                }
                if let Some(value) = trim_non_empty(local_cfg.api_key) {
                    env.insert(api_key_key.to_string(), value);
                }
                if agent_type != AgentType::ClaudeCode {
                    if let Some(value) = trim_non_empty(local_cfg.model) {
                        env.insert(model_key.to_string(), value);
                    }
                }
            }
        }
        let sort_order = setting.map(|m| m.sort_order).unwrap_or(idx as i32);
        // Persist detected version to DB for binary agents (npx written during install/upgrade)
        if dist_type == "binary" {
            let _ = agent_setting_service::set_installed_version(
                &db.conn,
                agent_type,
                local_installed_version.clone(),
            )
            .await;
        }
        if local_installed_version.is_some() {
            if let Err(error) = crate::commands::agent_storage::ensure_active_agent_profile_layout(
                &db.conn, agent_type,
            )
            .await
            {
                tracing::warn!(
                    agent_type = %agent_type,
                    error = %error,
                    "[agent-storage] failed to initialize installed Agent profile"
                );
            }
        }
        let codex_auth_json = if agent_type == AgentType::Codex {
            load_codex_auth_json_raw()
        } else {
            None
        };
        let opencode_auth_json = if agent_type == AgentType::OpenCode {
            load_opencode_auth_json_raw()
        } else {
            None
        };
        let codex_config_toml = if agent_type == AgentType::Codex {
            load_codex_config_toml_raw()
        } else {
            None
        };
        let cline_secrets_json = if agent_type == AgentType::Cline {
            load_cline_secrets_json_raw()
        } else {
            None
        };
        // Hermes is self-managed: project its own ~/.hermes/.env + config.yaml
        // into config_json (read-only) and attach the raw config.yaml for the
        // advanced editor. The env-merge block above is skipped because
        // `load_agent_local_config_json` returns None for Hermes (no iyw-claw
        // local config path), so no Hermes credential leaks into process env.
        let (config_json, hermes_config_yaml) = if agent_type == AgentType::Hermes {
            (
                load_hermes_local_config_json(),
                fs::read_to_string(hermes_config_yaml_path()).ok(),
            )
        } else {
            (local_config_json, None)
        };

        agents.push(AcpAgentInfo {
            agent_type,
            registry_id: registry::registry_id_for(agent_type).to_string(),
            registry_version: meta.registry_version().map(ToString::to_string),
            name: meta.name.to_string(),
            description: meta.description.to_string(),
            available,
            distribution_type: dist_type.to_string(),
            enabled: setting
                .map(|m| m.enabled)
                .unwrap_or_else(|| agent_setting_service::default_enabled(agent_type)),
            sort_order,
            installed_version: local_installed_version,
            env,
            config_json,
            config_file_path: agent_local_config_path(agent_type)
                .map(|path| path.display().to_string()),
            opencode_auth_json,
            codex_auth_json,
            codex_config_toml,
            cline_secrets_json,
            hermes_config_yaml,
            model_provider_id: setting.and_then(|m| m.model_provider_id),
        });
    }

    agents.sort_by(|a, b| {
        a.sort_order
            .cmp(&b.sort_order)
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(agents)
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_list_agents(
    db: tauri::State<'_, AppDatabase>,
) -> Result<Vec<AcpAgentInfo>, AcpError> {
    acp_list_agents_core(&db).await
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_clear_binary_cache(agent_type: AgentType) -> Result<(), AcpError> {
    let paths = active_agent_storage_paths()?;
    let meta = registry::get_agent_meta(agent_type);
    if matches!(
        meta.distribution,
        registry::AgentDistribution::Binary { .. }
    ) {
        binary_cache::clear_agent_cache(&paths, agent_type)?;
    }
    Ok(())
}

fn enabled_state_changed(previous: bool, current: bool) -> bool {
    previous != current
}

async fn reconcile_agent_enablement_best_effort(
    db: &AppDatabase,
    agent_type: AgentType,
    enabled: bool,
) {
    if let Err(error) =
        crate::commands::managed_skills::reconcile_agent_core(&db.conn, agent_type, enabled).await
    {
        tracing::warn!("[ACP] managed skills reconcile failed for {agent_type}: {error}");
    }
    if let Err(error) = reconcile_shared_skills_for_agent_state(agent_type, enabled) {
        tracing::warn!("[ACP] shared skills reconcile failed for {agent_type}: {error}");
    }
    if let Err(error) =
        crate::commands::mcp_sync::reconcile_managed_mcp_for_agent(&db.conn, agent_type, enabled)
            .await
    {
        tracing::warn!("[ACP] managed MCP reconcile failed for {agent_type}: {error}");
    }
}

async fn reconcile_installed_agent_best_effort(db: &AppDatabase, agent_type: AgentType) {
    let enabled = match agent_setting_service::get_by_agent_type(&db.conn, agent_type).await {
        Ok(Some(setting)) => setting.enabled,
        Ok(None) => agent_setting_service::default_enabled(agent_type),
        Err(error) => {
            tracing::warn!(
                agent_type = %agent_type,
                error = %error,
                "[ACP] failed to read installed Agent enablement"
            );
            return;
        }
    };
    reconcile_agent_enablement_best_effort(db, agent_type, enabled).await;
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn acp_update_agent_preferences_core(
    agent_type: AgentType,
    enabled: bool,
    env: BTreeMap<String, String>,
    config_json: Option<String>,
    opencode_auth_json: Option<String>,
    codex_auth_json: Option<String>,
    codex_config_toml: Option<String>,
    db: &AppDatabase,
    emitter: &EventEmitter,
) -> Result<(), AcpError> {
    let _paths = require_private_agent_storage_for_write()?;
    let default = agent_setting_service::AgentDefaultInput {
        agent_type,
        registry_id: registry::registry_id_for(agent_type).to_string(),
        default_sort_order: i32::MAX / 2,
    };

    agent_setting_service::ensure_defaults(&db.conn, &[default])
        .await
        .map_err(|e| AcpError::protocol(e.to_string()))?;
    let previous_enabled = agent_setting_service::get_by_agent_type(&db.conn, agent_type)
        .await
        .map_err(|e| AcpError::protocol(e.to_string()))?
        .ok_or_else(|| AcpError::protocol(format!("agent setting not found: {agent_type}")))?
        .enabled;

    let env_json = serialize_env_map(&env)?;
    let config_json = config_json.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });
    if let Some(raw) = config_json.as_deref() {
        let parsed = serde_json::from_str::<serde_json::Value>(raw)
            .map_err(|e| AcpError::protocol(format!("invalid config_json: {e}")))?;
        if !parsed.is_object() {
            return Err(AcpError::protocol(
                "invalid config_json: root must be a JSON object",
            ));
        }
    }

    let patch = agent_setting_service::AgentSettingsUpdate {
        enabled,
        env_json,
        model_provider_id: None,
    };
    agent_setting_service::update(&db.conn, agent_type, patch)
        .await
        .map_err(|e| AcpError::protocol(e.to_string()))?;
    if enabled_state_changed(previous_enabled, enabled) {
        reconcile_agent_enablement_best_effort(db, agent_type, enabled).await;
    }

    if agent_type == AgentType::Codex {
        if codex_auth_json.is_some() || codex_config_toml.is_some() {
            persist_codex_native_config_files(
                codex_auth_json.as_deref(),
                codex_config_toml.as_deref(),
                None,
            )?;
        }
        emit_acp_agents_updated(emitter, "preferences_updated", Some(agent_type));
        return Ok(());
    }

    if agent_type == AgentType::OpenCode {
        persist_opencode_native_config(opencode_auth_json.as_deref(), config_json.as_deref())?;
        emit_acp_agents_updated(emitter, "preferences_updated", Some(agent_type));
        return Ok(());
    }

    if agent_type == AgentType::Cline {
        if let Some(raw) = config_json.as_deref() {
            persist_cline_local_config(Some(raw))?;
        }
        emit_acp_agents_updated(emitter, "preferences_updated", Some(agent_type));
        return Ok(());
    }

    let mut local_patch_value = config_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
        .filter(|value| value.is_object())
        .unwrap_or_else(|| serde_json::json!({}));
    if !env.is_empty() {
        let env_json_value =
            serde_json::to_value(&env).map_err(|e| AcpError::protocol(e.to_string()))?;
        if let Some(obj) = local_patch_value.as_object_mut() {
            obj.insert("env".to_string(), env_json_value);
        }
    }
    let local_patch_json = serde_json::to_string(&local_patch_value)
        .map_err(|e| AcpError::protocol(format!("serialize local patch failed: {e}")))?;
    persist_agent_local_config_json(agent_type, Some(local_patch_json.as_str()))?;
    emit_acp_agents_updated(emitter, "preferences_updated", Some(agent_type));
    Ok(())
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
#[allow(clippy::too_many_arguments)]
pub async fn acp_update_agent_preferences(
    agent_type: AgentType,
    enabled: bool,
    env: BTreeMap<String, String>,
    config_json: Option<String>,
    opencode_auth_json: Option<String>,
    codex_auth_json: Option<String>,
    codex_config_toml: Option<String>,
    manager: State<'_, ConnectionManager>,
    db: State<'_, AppDatabase>,
    app: tauri::AppHandle,
) -> Result<usize, AcpError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map(|p| crate::paths::resolve_effective_data_dir(&p))
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    let emitter = EventEmitter::Tauri(app);
    acp_update_agent_preferences_and_refresh(
        agent_type,
        enabled,
        env,
        config_json,
        opencode_auth_json,
        codex_auth_json,
        codex_config_toml,
        &db,
        &manager,
        &app_data_dir,
        &emitter,
    )
    .await
}

pub(crate) async fn acp_update_agent_env_core(
    agent_type: AgentType,
    enabled: bool,
    env: BTreeMap<String, String>,
    model_provider_id: Option<i32>,
    db: &AppDatabase,
    emitter: &EventEmitter,
) -> Result<(), AcpError> {
    let _paths = require_private_agent_storage_for_write()?;
    let default = agent_setting_service::AgentDefaultInput {
        agent_type,
        registry_id: registry::registry_id_for(agent_type).to_string(),
        default_sort_order: i32::MAX / 2,
    };

    agent_setting_service::ensure_defaults(&db.conn, &[default])
        .await
        .map_err(|e| AcpError::protocol(e.to_string()))?;
    let previous_enabled = agent_setting_service::get_by_agent_type(&db.conn, agent_type)
        .await
        .map_err(|e| AcpError::protocol(e.to_string()))?
        .ok_or_else(|| AcpError::protocol(format!("agent setting not found: {agent_type}")))?
        .enabled;

    // If a provider is selected, the provider's model field is authoritative:
    // each relevant env key is set when the provider has a value and cleared
    // (removed) when empty. Codex's root `model` in config.toml is handled the
    // same way.
    let mut merged_env = env;
    let mut codex_action = CodexModelAction::NoOp;
    // When a Claude provider is bound, capture the inputs to also rewrite the
    // on-disk config.env below. Claude's model fields live in config.env, which
    // the runtime overlays OVER db env_json (see `build_runtime_env_from_setting`),
    // so clearing a key from db env alone is not enough — a stale value left in
    // `~/.claude/settings.json` (e.g. ANTHROPIC_CUSTOM_MODEL_OPTION) would win at
    // launch. Binding must therefore be authoritative on disk too, matching the
    // provider-edit cascade.
    let mut claude_local_cascade: Option<(String, String, BTreeMap<String, Option<String>>)> = None;
    if let Some(pid) = model_provider_id {
        let provider = crate::db::service::model_provider_service::get_by_id(&db.conn, pid)
            .await
            .map_err(|e| AcpError::protocol(e.to_string()))?
            .ok_or_else(|| AcpError::protocol(format!("model provider not found: {pid}")))?;

        // Reject cross-type binding: provider.model is formatted for its declared
        // agent_type (Claude = JSON, Codex/Gemini/others = plain string). Binding
        // a mismatched provider would parse the model under the wrong format and
        // silently write invalid env / config.toml entries.
        let provider_agent_type: AgentType =
            serde_json::from_value(serde_json::Value::String(provider.agent_type.clone()))
                .map_err(|_| {
                    AcpError::protocol(format!(
                        "model provider {pid} has invalid agent_type: {}",
                        provider.agent_type
                    ))
                })?;
        if provider_agent_type != agent_type {
            return Err(AcpError::protocol(format!(
                "model provider {pid} is for {provider_agent_type}, cannot be bound to {agent_type}"
            )));
        }

        let model_env = parse_provider_model(agent_type, provider.model.as_deref());
        for (k, v) in &model_env {
            match v {
                Some(value) => {
                    merged_env.insert(k.clone(), value.clone());
                }
                None => {
                    merged_env.remove(k);
                }
            }
        }
        codex_action = provider_codex_model_action(agent_type, provider.model.as_deref());
        // Codex's on-disk config is handled by `apply_codex_root_model_action`
        // below; Gemini's analogous config.env gap is pre-existing and out of
        // scope here. Only Claude needs the local-config cascade on bind.
        if agent_type == AgentType::ClaudeCode {
            claude_local_cascade = Some((
                provider.api_url.clone(),
                provider.api_key.clone(),
                model_env,
            ));
        }
    }

    let patch = agent_setting_service::AgentSettingsUpdate {
        enabled,
        env_json: serialize_env_map(&merged_env)?,
        model_provider_id,
    };
    agent_setting_service::update(&db.conn, agent_type, patch)
        .await
        .map_err(|e| AcpError::protocol(e.to_string()))?;
    if enabled_state_changed(previous_enabled, enabled) {
        reconcile_agent_enablement_best_effort(db, agent_type, enabled).await;
    }

    // Authoritatively rewrite the local config.env so a stale model key (e.g. the
    // custom model option) cannot survive a bind/rebind via any save path. `None`
    // entries become JSON-null and are removed by `merge_json_values`.
    if let Some((api_url, api_key, model_env)) = claude_local_cascade {
        if let Err(e) = cascade_update_agent_config(
            agent_type,
            &api_url,
            &api_key,
            &model_env,
            &CodexModelAction::NoOp,
        ) {
            eprintln!(
                "[acp_update_agent_env] cascade_update_agent_config({agent_type}) failed: {e}"
            );
        }
    }

    if let Err(e) = apply_codex_root_model_action(&codex_action) {
        tracing::error!("[acp_update_agent_env] apply_codex_root_model_action failed: {e}");
    }

    emit_acp_agents_updated(emitter, "env_updated", Some(agent_type));
    Ok(())
}

/// Apply a `CodexModelAction` to the `model` field at the root of
/// `~/.codex/config.toml`, preserving everything else.
fn apply_codex_root_model_action(action: &CodexModelAction) -> Result<(), AcpError> {
    if matches!(action, CodexModelAction::NoOp) {
        return Ok(());
    }
    let config_path = codex_config_toml_path();
    let mut toml_value = if config_path.exists() {
        fs::read_to_string(&config_path)
            .ok()
            .and_then(|raw| raw.parse::<toml::Value>().ok())
            .filter(|v| v.is_table())
            .unwrap_or_else(|| toml::Value::Table(toml::map::Map::new()))
    } else {
        toml::Value::Table(toml::map::Map::new())
    };
    let table = toml_value
        .as_table_mut()
        .ok_or_else(|| AcpError::protocol("codex config root must be a TOML table"))?;
    match action {
        CodexModelAction::Set(model) => {
            table.insert("model".to_string(), toml::Value::String(model.clone()));
        }
        CodexModelAction::Clear => {
            table.remove("model");
        }
        CodexModelAction::NoOp => unreachable!(),
    }
    let toml_str =
        toml::to_string_pretty(&toml_value).map_err(|e| AcpError::protocol(e.to_string()))?;
    persist_codex_native_config_files(None, Some(&toml_str), None)?;
    Ok(())
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_update_agent_env(
    agent_type: AgentType,
    enabled: bool,
    env: BTreeMap<String, String>,
    model_provider_id: Option<i32>,
    manager: State<'_, ConnectionManager>,
    db: State<'_, AppDatabase>,
    app: tauri::AppHandle,
) -> Result<usize, AcpError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map(|p| crate::paths::resolve_effective_data_dir(&p))
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    let emitter = EventEmitter::Tauri(app);
    acp_update_agent_env_and_refresh(
        agent_type,
        enabled,
        env,
        model_provider_id,
        &db,
        &manager,
        &app_data_dir,
        &emitter,
    )
    .await
}

/// Decide what to write to OpenCode's `auth.json`. `None` (caller passed no
/// auth payload) leaves the file untouched. An explicitly empty payload becomes
/// `{}` so clearing the last credential truncates the file instead of being
/// skipped — otherwise a stale key would survive on disk and the disconnected
/// provider would reappear after reload.
fn opencode_auth_payload_to_write(raw: Option<&str>) -> Option<String> {
    let trimmed = raw?.trim();
    Some(if trimmed.is_empty() {
        "{}".to_string()
    } else {
        trimmed.to_string()
    })
}

/// Persist OpenCode's native files (`auth.json` + `opencode.json`) for a
/// config/preferences save. Shared by both the config and preferences commands
/// so the empty-auth handling can't drift between the two exposed paths. An
/// explicitly empty auth payload truncates `auth.json` to `{}`; `None` leaves
/// each file untouched.
fn persist_opencode_native_config(
    opencode_auth_json: Option<&str>,
    config_json: Option<&str>,
) -> Result<(), AcpError> {
    if let Some(auth) = opencode_auth_payload_to_write(opencode_auth_json) {
        persist_opencode_auth_json(&auth)?;
    }
    if let Some(raw) = config_json {
        persist_agent_local_config_json(AgentType::OpenCode, Some(raw))?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn acp_update_agent_config_core(
    agent_type: AgentType,
    config_json: Option<String>,
    opencode_auth_json: Option<String>,
    codex_auth_json: Option<String>,
    codex_config_toml: Option<String>,
    emitter: &EventEmitter,
) -> Result<(), AcpError> {
    let _paths = require_private_agent_storage_for_write()?;
    let config_json = config_json.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });
    if let Some(raw) = config_json.as_deref() {
        let parsed = serde_json::from_str::<serde_json::Value>(raw)
            .map_err(|e| AcpError::protocol(format!("invalid config_json: {e}")))?;
        if !parsed.is_object() {
            return Err(AcpError::protocol(
                "invalid config_json: root must be a JSON object",
            ));
        }
    }

    if agent_type == AgentType::Codex {
        let codex_model_ids = codex_model_ids_from_projection(config_json.as_deref())?;
        if codex_auth_json.is_some() || codex_config_toml.is_some() || codex_model_ids.is_some() {
            persist_codex_native_config_files(
                codex_auth_json.as_deref(),
                codex_config_toml.as_deref(),
                codex_model_ids.as_deref(),
            )?;
        }
        emit_acp_agents_updated(emitter, "config_updated", Some(agent_type));
        return Ok(());
    }

    if agent_type == AgentType::OpenCode {
        persist_opencode_native_config(opencode_auth_json.as_deref(), config_json.as_deref())?;
        emit_acp_agents_updated(emitter, "config_updated", Some(agent_type));
        return Ok(());
    }

    if agent_type == AgentType::Cline {
        if let Some(raw) = config_json.as_deref() {
            persist_cline_local_config(Some(raw))?;
        }
        emit_acp_agents_updated(emitter, "config_updated", Some(agent_type));
        return Ok(());
    }

    // Claude Code, Gemini, OpenClaw — write config JSON to local file without merging env
    let local_patch_value = config_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
        .filter(|value| value.is_object())
        .unwrap_or_else(|| serde_json::json!({}));
    let local_patch_json = serde_json::to_string(&local_patch_value)
        .map_err(|e| AcpError::protocol(format!("serialize local patch failed: {e}")))?;
    persist_agent_local_config_json(agent_type, Some(local_patch_json.as_str()))?;
    emit_acp_agents_updated(emitter, "config_updated", Some(agent_type));
    Ok(())
}

/// `acp_update_agent_config_core` (native config file write) followed by a
/// staleness refresh. Shared by the Tauri command and the web handler; returns
/// the count of running sessions left on stale config.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn acp_update_agent_config_and_refresh(
    agent_type: AgentType,
    config_json: Option<String>,
    opencode_auth_json: Option<String>,
    codex_auth_json: Option<String>,
    codex_config_toml: Option<String>,
    db: &AppDatabase,
    manager: &ConnectionManager,
    data_dir: &Path,
    emitter: &EventEmitter,
) -> Result<usize, AcpError> {
    acp_update_agent_config_core(
        agent_type,
        config_json,
        opencode_auth_json,
        codex_auth_json,
        codex_config_toml,
        emitter,
    )
    .await?;
    Ok(refresh_config_staleness(
        manager,
        db,
        data_dir,
        &[agent_type],
        ConfigStaleKind::AgentConfig,
    )
    .await)
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
#[allow(clippy::too_many_arguments)]
pub async fn acp_update_agent_config(
    agent_type: AgentType,
    config_json: Option<String>,
    opencode_auth_json: Option<String>,
    codex_auth_json: Option<String>,
    codex_config_toml: Option<String>,
    manager: State<'_, ConnectionManager>,
    db: State<'_, AppDatabase>,
    app: tauri::AppHandle,
) -> Result<usize, AcpError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map(|p| crate::paths::resolve_effective_data_dir(&p))
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    let emitter = EventEmitter::Tauri(app);
    acp_update_agent_config_and_refresh(
        agent_type,
        config_json,
        opencode_auth_json,
        codex_auth_json,
        codex_config_toml,
        &db,
        &manager,
        &app_data_dir,
        &emitter,
    )
    .await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_update_hermes_config(
    provider: String,
    api_key: Option<String>,
    model: Option<String>,
    base_url: Option<String>,
    raw_config_yaml: Option<String>,
    app: tauri::AppHandle,
) -> Result<(), AcpError> {
    let emitter = EventEmitter::Tauri(app);
    acp_update_hermes_config_core(
        HermesConfigUpdate {
            provider,
            api_key,
            model,
            base_url,
            raw_config_yaml,
        },
        &emitter,
    )
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
#[allow(clippy::too_many_arguments)]
pub async fn acp_update_kimi_code_config(
    mode: String,
    interface_type: Option<String>,
    auth_type: Option<String>,
    base_url: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
    max_context_size: Option<i64>,
    vertex_project: Option<String>,
    vertex_location: Option<String>,
    raw_config_toml: Option<String>,
    manager: State<'_, ConnectionManager>,
    db: State<'_, AppDatabase>,
    app: tauri::AppHandle,
) -> Result<usize, AcpError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map(|p| crate::paths::resolve_effective_data_dir(&p))
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    let emitter = EventEmitter::Tauri(app);
    acp_update_kimi_code_config_and_refresh(
        KimiCodeConfigUpdate {
            mode,
            interface_type,
            auth_type,
            base_url,
            api_key,
            model,
            max_context_size,
            vertex_project,
            vertex_location,
            raw_config_toml,
        },
        &db,
        &manager,
        &app_data_dir,
        &emitter,
    )
    .await
}

/// List the models an API key + endpoint can access (validates the key and
/// populates the Kimi settings model picker). Desktop command; the web handler
/// calls `acp_fetch_kimi_models_core` directly.
#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_fetch_kimi_models(
    base_url: String,
    api_key: String,
) -> Result<Vec<String>, AcpError> {
    acp_fetch_kimi_models_core(&base_url, &api_key).await
}

/// Apply a structured Pi config update, writing pi's native `settings.json`
/// (provider/model/thinking level) and `auth.json` (when an API key is given).
/// Desktop command; the web handler calls `acp_update_pi_config_core` directly.
#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
#[allow(clippy::too_many_arguments)]
pub async fn acp_update_pi_config(
    provider: String,
    model: String,
    thinking_level: Option<String>,
    api_key: Option<String>,
    custom_base_url: Option<String>,
    custom_api: Option<String>,
    db: State<'_, AppDatabase>,
    app: tauri::AppHandle,
) -> Result<(), AcpError> {
    let emitter = EventEmitter::Tauri(app);
    acp_update_pi_config_core(
        PiConfigUpdate {
            provider,
            model,
            thinking_level,
            api_key,
            custom_base_url,
            custom_api,
        },
        &db,
        &emitter,
    )
    .await
}

/// Read pi's current native config (model selection + configured auth providers)
/// for the settings panel. Desktop command; the web handler calls
/// `load_pi_config_core` directly. Reads the filesystem only — no DB/state needed.
#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_load_pi_config() -> Result<PiConfigProjection, AcpError> {
    Ok(load_pi_config_core())
}

/// Validate a user-supplied custom pi binary (BYO-pi): resolve it (path or
/// `PATH`) and best-effort read its `--version`. A not-found binary returns
/// `found=false` (not an error). Desktop command; the web handler calls
/// `acp_validate_pi_command_core` directly.
#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_validate_pi_command(command: String) -> Result<PiCommandValidation, AcpError> {
    Ok(acp_validate_pi_command_core(command))
}

/// Launch Hermes's interactive setup in the OS terminal. `kind` selects the
/// flow (`"setup"` → `hermes-acp --setup`, `"model"` → `hermes model`); the
/// exact command is constructed by the backend from the registry recipe (the
/// renderer cannot supply arbitrary shell text). Ensures `~/.hermes` exists so
/// the `cd` into it can't fail on a fresh install. Desktop-only: these flows
/// need a real interactive TTY and a browser for OAuth.
#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_open_hermes_setup_terminal(kind: String) -> Result<(), AcpError> {
    let paths = active_agent_storage_paths()?;
    if binary_cache::find_cached_uv_tool(&paths, "uvx").is_none() {
        return Err(AcpError::SdkNotInstalled(
            "uv is not installed; install the uv runtime first".to_string(),
        ));
    }
    let (setup, model) = hermes_setup_commands();
    let base_command = match kind.as_str() {
        "setup" => setup,
        "model" => model,
        other => {
            return Err(AcpError::protocol(format!(
                "unknown hermes setup kind: {other}"
            )));
        }
    };
    let command = with_private_uv_shell_env(&base_command, &paths, cfg!(windows));
    let home = hermes_home_dir();
    ensure_hermes_home_secure(&home)?;
    let home_str = home.to_string_lossy();
    open_external_terminal_impl(&command, Some(home_str.as_ref()))
}

#[cfg(feature = "tauri-runtime")]
fn open_external_terminal_impl(command: &str, cwd: Option<&str>) -> Result<(), AcpError> {
    use std::process::Command;
    // Reject control characters: a newline breaks out of the macOS AppleScript
    // string literal (and would corrupt the cmd/shell line elsewhere), turning a
    // single command into multiple statements.
    if command.contains(['\n', '\r']) || cwd.is_some_and(|c| c.contains(['\n', '\r'])) {
        return Err(AcpError::protocol(
            "terminal command and cwd must not contain newlines",
        ));
    }
    let dir = cwd
        .map(|c| c.to_string())
        .unwrap_or_else(|| home_dir_or_default().display().to_string());

    #[cfg(target_os = "macos")]
    {
        // Hand `cd <dir> && <command>` to Terminal.app via AppleScript. Quote the
        // dir for the shell, then escape the whole string for the AppleScript
        // literal (backslashes first, then double-quotes).
        let shell_cmd = format!("cd {} && {}", shell_single_quote(&dir), command);
        let escaped = shell_cmd.replace('\\', "\\\\").replace('"', "\\\"");
        let osa =
            format!("tell application \"Terminal\"\nactivate\ndo script \"{escaped}\"\nend tell");
        Command::new("osascript")
            .arg("-e")
            .arg(osa)
            .spawn()
            .map_err(|e| AcpError::protocol(format!("open Terminal failed: {e}")))?;
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        // `start "" cmd /K <command>` opens a new console that stays open. The
        // empty "" is the window title `start` would otherwise eat.
        Command::new("cmd")
            .args(["/C", "start", "", "cmd", "/K", command])
            .current_dir(&dir)
            .spawn()
            .map_err(|e| AcpError::protocol(format!("open terminal failed: {e}")))?;
        return Ok(());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // Probe common Linux terminal emulators in order; keep the window open
        // after the command by re-exec'ing the user's shell.
        let keep_open = format!("{command}; exec \"${{SHELL:-bash}}\"");
        let candidates: [(&str, [&str; 3]); 4] = [
            ("x-terminal-emulator", ["-e", "sh", "-c"]),
            ("gnome-terminal", ["--", "sh", "-c"]),
            ("konsole", ["-e", "sh", "-c"]),
            ("xterm", ["-e", "sh", "-c"]),
        ];
        for (term, args) in candidates {
            if resolve_command_on_path(term).is_some() {
                return Command::new(term)
                    .args(args)
                    .arg(&keep_open)
                    .current_dir(&dir)
                    .spawn()
                    .map(|_| ())
                    .map_err(|e| AcpError::protocol(format!("open {term} failed: {e}")));
            }
        }
        return Err(AcpError::protocol(
            "no supported terminal emulator found (tried x-terminal-emulator, gnome-terminal, konsole, xterm)",
        ));
    }

    #[allow(unreachable_code)]
    Err(AcpError::protocol(
        "unsupported platform for terminal launch",
    ))
}

/// Quote a string for a single-quoted POSIX shell argument.
#[cfg(all(feature = "tauri-runtime", target_os = "macos"))]
fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Ensure `~/.hermes` exists and reveal it in the system file manager.
#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_reveal_hermes_home(app: tauri::AppHandle) -> Result<(), AcpError> {
    use tauri_plugin_opener::OpenerExt;
    let home = hermes_home_dir();
    ensure_hermes_home_secure(&home)?;
    app.opener()
        .open_path(home.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| AcpError::protocol(format!("open hermes folder failed: {e}")))?;
    Ok(())
}

pub(crate) async fn acp_download_agent_binary_core(
    agent_type: AgentType,
    version_override: Option<String>,
    task_id: String,
    db: &AppDatabase,
    emitter: &EventEmitter,
) -> Result<(), AcpError> {
    let _storage_work_guard = crate::acp::agent_storage_work::begin_agent_storage_work().await;
    emit_agent_install_event(emitter, &task_id, AgentInstallEventKind::Started, "");
    let paths = active_agent_storage_paths()?;

    let meta = registry::get_agent_meta(agent_type);
    agent_setting_service::ensure_defaults(
        &db.conn,
        &[agent_setting_service::AgentDefaultInput {
            agent_type,
            registry_id: registry::registry_id_for(agent_type).to_string(),
            default_sort_order: i32::MAX / 2,
        }],
    )
    .await
    .map_err(|error| AcpError::protocol(error.to_string()))?;
    let result = match meta.distribution {
        registry::AgentDistribution::Binary {
            version,
            cmd,
            platforms,
            ..
        } => {
            // A custom version substitutes into the pinned download URL and the
            // cache key; `None`/empty keeps the registry-pinned version.
            let custom = match version_override.as_deref() {
                Some(raw) if !raw.trim().is_empty() => {
                    Some(sanitize_custom_version(raw).ok_or_else(|| {
                        AcpError::protocol(format!("invalid custom version: {}", raw.trim()))
                    })?)
                }
                _ => None,
            };

            let platform = registry::current_platform();
            let fallback = platforms
                .iter()
                .find(|p| p.platform == platform)
                .ok_or_else(|| {
                    AcpError::PlatformNotSupported(format!(
                        "{} is not available on {platform}",
                        meta.name
                    ))
                })?;

            let effective_version = custom.as_deref().unwrap_or(version);
            let archive_url = match &custom {
                Some(c) => apply_custom_version_to_url(fallback.url, version, c),
                None => fallback.url.to_string(),
            };

            emit_agent_install_event(
                emitter,
                &task_id,
                AgentInstallEventKind::Log,
                format!(
                    "Downloading {} v{effective_version} for {platform}",
                    meta.name
                ),
            );

            let emitter_clone = emitter.clone();
            let task_id_clone = task_id.clone();
            let _ = binary_cache::ensure_binary_for_agent_with_progress(
                &paths,
                agent_type,
                effective_version,
                &archive_url,
                cmd,
                move |msg| {
                    emit_agent_install_event(
                        &emitter_clone,
                        &task_id_clone,
                        AgentInstallEventKind::Log,
                        msg,
                    );
                },
            )
            .await?;
            crate::commands::agent_storage::ensure_active_agent_profile_layout(
                &db.conn, agent_type,
            )
            .await
            .map_err(|error| AcpError::protocol(error.to_string()))?;
            agent_setting_service::set_installed_version(
                &db.conn,
                agent_type,
                Some(effective_version.to_string()),
            )
            .await
            .map_err(|error| AcpError::protocol(error.to_string()))?;
            emit_acp_agents_updated(emitter, "binary_downloaded", Some(agent_type));
            Ok(())
        }
        registry::AgentDistribution::Npx { .. } => Err(AcpError::protocol(
            "download is only supported for binary agents",
        )),
        registry::AgentDistribution::Uvx { .. } => Err(AcpError::protocol(
            "download is only supported for binary agents",
        )),
    };

    let result: Result<(), AcpError> = async {
        result?;
        crate::acp::provider_overlay::enforce_active_provider_overlay(agent_type)
            .map_err(AcpError::protocol)?;
        crate::acp::account_credentials::sync_agent_credentials_for_acp(&db.conn, agent_type)
            .await?;
        reconcile_installed_agent_best_effort(db, agent_type).await;
        Ok(())
    }
    .await;

    match &result {
        Ok(()) => {
            emit_agent_install_event(
                emitter,
                &task_id,
                AgentInstallEventKind::Completed,
                format!("{} installed successfully", meta.name),
            );
        }
        Err(e) => {
            emit_agent_install_event(
                emitter,
                &task_id,
                AgentInstallEventKind::Failed,
                e.to_string(),
            );
        }
    }
    result
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_download_agent_binary(
    agent_type: AgentType,
    version: Option<String>,
    task_id: String,
    db: State<'_, AppDatabase>,
    app: tauri::AppHandle,
) -> Result<(), AcpError> {
    let emitter = EventEmitter::Tauri(app);
    acp_download_agent_binary_core(agent_type, version, task_id, &db, &emitter).await
}

/// Provision ONLY the uv toolchain (uvx) into iyw-claw's cache — independent of
/// installing any `Uvx` agent's package. Streams progress over the shared
/// agent-install event stream so the Settings page shows a live log. Backs the
/// uv preflight check's "Install uv" fix. After this succeeds,
/// `resolve_uvx_command()` resolves the cached uvx, so a subsequent preflight /
/// agent-status reports uv as available.
pub(crate) async fn acp_install_uv_tool_core(
    task_id: String,
    emitter: &EventEmitter,
) -> Result<(), AcpError> {
    let _storage_work_guard = crate::acp::agent_storage_work::begin_agent_storage_work().await;
    emit_agent_install_event(emitter, &task_id, AgentInstallEventKind::Started, "");
    let paths = active_agent_storage_paths()?;

    let emitter_clone = emitter.clone();
    let task_id_clone = task_id.clone();
    let result = crate::acp::binary_cache::ensure_uv_tool(&paths, move |msg| {
        emit_agent_install_event(
            &emitter_clone,
            &task_id_clone,
            AgentInstallEventKind::Log,
            msg.to_string(),
        );
    })
    .await
    .map(|_| ());

    match &result {
        Ok(()) => {
            emit_agent_install_event(
                emitter,
                &task_id,
                AgentInstallEventKind::Completed,
                "uv runtime installed successfully".to_string(),
            );
            // uv is shared across all uvx agents, so its arrival flips their
            // availability — notify every client to refetch the agent list.
            emit_acp_agents_updated(emitter, "uv_installed", None);
        }
        Err(e) => {
            emit_agent_install_event(
                emitter,
                &task_id,
                AgentInstallEventKind::Failed,
                e.to_string(),
            );
        }
    }
    result
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_install_uv_tool(task_id: String, app: tauri::AppHandle) -> Result<(), AcpError> {
    let emitter = EventEmitter::Tauri(app);
    acp_install_uv_tool_core(task_id, &emitter).await
}

pub(crate) async fn acp_detect_agent_local_version_core(
    agent_type: AgentType,
    conn: &sea_orm::DatabaseConnection,
) -> Result<Option<String>, AcpError> {
    let recorded = agent_setting_service::get_by_agent_type(conn, agent_type)
        .await
        .map_err(|e| AcpError::protocol(e.to_string()))?
        .and_then(|model| model.installed_version);
    let detected = detect_local_version(agent_type, recorded.as_deref());
    if let Some(version) = detected.clone() {
        let _ =
            agent_setting_service::set_installed_version(conn, agent_type, Some(version.clone()))
                .await;
        return Ok(Some(version));
    }

    let _ = agent_setting_service::set_installed_version(conn, agent_type, None).await;
    Ok(None)
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_detect_agent_local_version(
    agent_type: AgentType,
    db: State<'_, AppDatabase>,
) -> Result<Option<String>, AcpError> {
    acp_detect_agent_local_version_core(agent_type, &db.conn).await
}

pub(crate) async fn acp_prepare_npx_agent_core(
    agent_type: AgentType,
    registry_version: Option<String>,
    version_override: Option<String>,
    clean_first: bool,
    task_id: String,
    db: &AppDatabase,
    emitter: &EventEmitter,
) -> Result<String, AcpError> {
    let _storage_work_guard = crate::acp::agent_storage_work::begin_agent_storage_work().await;
    emit_agent_install_event(emitter, &task_id, AgentInstallEventKind::Started, "");
    let paths = active_agent_storage_paths()?;

    let meta = registry::get_agent_meta(agent_type);
    let result = match meta.distribution {
        registry::AgentDistribution::Npx {
            package,
            cmd,
            version,
            ..
        } => {
            // `version_override` of None/empty keeps the registry-pinned spec;
            // a custom version installs `<name>@<version>` instead.
            let legacy_install_spec = build_npm_install_spec(package, version_override.as_deref())?;
            let legacy_resolved = version_from_package_spec(&legacy_install_spec)
                .or_else(|| {
                    registry_version
                        .as_deref()
                        .and_then(normalize_version_candidate)
                })
                .or_else(|| normalize_version_candidate(version))
                .ok_or_else(|| {
                    AcpError::protocol("failed to determine private npm runtime version")
                })?;
            let mut install_spec = legacy_install_spec.clone();
            let mut resolved = legacy_resolved.clone();
            let mut managed_install = None;

            let default = agent_setting_service::AgentDefaultInput {
                agent_type,
                registry_id: registry::registry_id_for(agent_type).to_string(),
                default_sort_order: i32::MAX / 2,
            };
            agent_setting_service::ensure_defaults(&db.conn, &[default])
                .await
                .map_err(|e| AcpError::protocol(e.to_string()))?;

            if clean_first {
                emit_agent_install_event(
                    emitter,
                    &task_id,
                    AgentInstallEventKind::Log,
                    "Clean reinstall requested; preparing a fresh private staging prefix",
                );
            }

            if agent_type == AgentType::Codex {
                match resolve_npm_agent_install(&db.conn, agent_type, &resolved).await {
                    Ok(managed) => {
                        emit_agent_install_event(
                            emitter,
                            &task_id,
                            AgentInstallEventKind::Log,
                            format!(
                                "Codex source resolved by version center ({})",
                                managed.registry
                            ),
                        );
                        install_spec = managed.install_spec.clone();
                        resolved = managed.version.clone();
                        managed_install = Some(managed);
                    }
                    Err(error) => {
                        let unavailable = error.is_unavailable();
                        let error = error.into_error();
                        if !unavailable {
                            return Err(error);
                        }
                        tracing::warn!(
                            agent_type = ?agent_type,
                            requested_version = %resolved,
                            error = %error,
                            "[agent-version-center] Codex npm resolve failed; using legacy source"
                        );
                        emit_agent_install_event(
                            emitter,
                            &task_id,
                            AgentInstallEventKind::Log,
                            format!(
                                "Version center unavailable ({error}); using the legacy npm source"
                            ),
                        );
                    }
                }
            }
            emit_agent_install_event(
                emitter,
                &task_id,
                AgentInstallEventKind::Log,
                format!("Installing {} ({install_spec})", meta.name),
            );
            let mut packages = vec![install_spec.as_str()];
            let mut required_commands = vec![cmd];
            if agent_type == AgentType::Pi {
                packages.push(PI_CODING_AGENT_PACKAGE);
                required_commands.push("pi");
            }
            if let Some(managed) = managed_install.as_ref() {
                let install_result = install_managed_npm_package(
                    &paths,
                    agent_type,
                    &managed.package_name,
                    &managed.version,
                    &managed.registry,
                    &managed.integrity,
                    &required_commands,
                    &task_id,
                    emitter,
                )
                .await;
                match install_result {
                    Ok(_) => {}
                    Err(ManagedNpmSourceError::Rejected(error)) => return Err(error),
                    Err(ManagedNpmSourceError::Unavailable(error)) => {
                        tracing::warn!(
                            agent_type = ?agent_type,
                            managed_error = %error,
                            "[agent-version-center] managed npm source unavailable; using legacy source"
                        );
                        emit_agent_install_event(
                            emitter,
                            &task_id,
                            AgentInstallEventKind::Log,
                            format!(
                                "Managed npm source unavailable ({error}); using the legacy npm source"
                            ),
                        );
                        install_private_npm_package(
                            &paths,
                            agent_type,
                            &legacy_resolved,
                            &[legacy_install_spec.as_str()],
                            None,
                            None,
                            &[cmd],
                            &task_id,
                            emitter,
                        )
                        .await?;
                        resolved = legacy_resolved;
                    }
                }
            } else {
                install_private_npm_package(
                    &paths,
                    agent_type,
                    &resolved,
                    &packages,
                    None,
                    None,
                    &required_commands,
                    &task_id,
                    emitter,
                )
                .await?;
            }

            crate::commands::agent_storage::ensure_active_agent_profile_layout(
                &db.conn, agent_type,
            )
            .await
            .map_err(|error| AcpError::protocol(error.to_string()))?;
            agent_setting_service::set_installed_version(
                &db.conn,
                agent_type,
                Some(resolved.clone()),
            )
            .await
            .map_err(|e| AcpError::protocol(e.to_string()))?;
            emit_acp_agents_updated(emitter, "npx_prepared", Some(agent_type));
            Ok(resolved)
        }
        registry::AgentDistribution::Binary { .. } => Err(AcpError::protocol(
            "prepare is only supported for npx agents",
        )),
        registry::AgentDistribution::Uvx {
            package,
            cmd,
            version,
            python,
            ..
        } => {
            let default = agent_setting_service::AgentDefaultInput {
                agent_type,
                registry_id: registry::registry_id_for(agent_type).to_string(),
                default_sort_order: i32::MAX / 2,
            };
            agent_setting_service::ensure_defaults(&db.conn, &[default])
                .await
                .map_err(|e| AcpError::protocol(e.to_string()))?;

            // Pre-fetch the pinned package into uvx's cache so the first
            // connect doesn't pay the download cost. The version is pinned in
            // the package spec, so `version_override` does not apply here.
            prewarm_uvx_agent(meta.name, package, cmd, python, &task_id, emitter).await?;

            let resolved = version.to_string();
            binary_cache::mark_uvx_agent_prepared(&paths, agent_type, &resolved)?;
            crate::commands::agent_storage::ensure_active_agent_profile_layout(
                &db.conn, agent_type,
            )
            .await
            .map_err(|error| AcpError::protocol(error.to_string()))?;
            agent_setting_service::set_installed_version(
                &db.conn,
                agent_type,
                Some(resolved.clone()),
            )
            .await
            .map_err(|e| AcpError::protocol(e.to_string()))?;
            emit_acp_agents_updated(emitter, "uvx_prepared", Some(agent_type));
            Ok(resolved)
        }
    };

    let result: Result<String, AcpError> = async {
        let version = result?;
        crate::acp::provider_overlay::enforce_active_provider_overlay(agent_type)
            .map_err(AcpError::protocol)?;
        crate::acp::account_credentials::sync_agent_credentials_for_acp(&db.conn, agent_type)
            .await?;
        reconcile_installed_agent_best_effort(db, agent_type).await;
        Ok(version)
    }
    .await;

    match &result {
        Ok(version) => {
            emit_agent_install_event(
                emitter,
                &task_id,
                AgentInstallEventKind::Completed,
                format!("{} v{version} installed successfully", meta.name),
            );
        }
        Err(e) => {
            emit_agent_install_event(
                emitter,
                &task_id,
                AgentInstallEventKind::Failed,
                e.to_string(),
            );
        }
    }
    result
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_prepare_npx_agent(
    agent_type: AgentType,
    registry_version: Option<String>,
    version: Option<String>,
    clean_first: Option<bool>,
    task_id: String,
    db: State<'_, AppDatabase>,
    app: tauri::AppHandle,
) -> Result<String, AcpError> {
    let emitter = EventEmitter::Tauri(app);
    acp_prepare_npx_agent_core(
        agent_type,
        registry_version,
        version,
        clean_first.unwrap_or(false),
        task_id,
        &db,
        &emitter,
    )
    .await
}

pub(crate) async fn acp_uninstall_agent_core(
    agent_type: AgentType,
    task_id: String,
    db: &AppDatabase,
    emitter: &EventEmitter,
) -> Result<(), AcpError> {
    let _storage_work_guard = crate::acp::agent_storage_work::begin_agent_storage_work().await;
    emit_agent_install_event(emitter, &task_id, AgentInstallEventKind::Started, "");
    let paths = active_agent_storage_paths()?;

    let meta = registry::get_agent_meta(agent_type);
    emit_agent_install_event(
        emitter,
        &task_id,
        AgentInstallEventKind::Log,
        format!("Uninstalling {}...", meta.name),
    );

    let result: Result<(), AcpError> = async {
        match meta.distribution {
            registry::AgentDistribution::Binary { .. } => {
                binary_cache::clear_agent_cache(&paths, agent_type)?;
            }
            registry::AgentDistribution::Npx { .. } => {
                npm_runtime::uninstall_private_npm_runtime(&paths, agent_type)?;
            }
            registry::AgentDistribution::Uvx { .. } => {
                binary_cache::clear_uvx_agent_prepared(&paths, agent_type)?;
            }
        }

        agent_setting_service::set_installed_version(&db.conn, agent_type, None)
            .await
            .map_err(|e| AcpError::protocol(e.to_string()))?;
        reconcile_agent_enablement_best_effort(db, agent_type, false).await;
        emit_acp_agents_updated(emitter, "agent_uninstalled", Some(agent_type));
        Ok(())
    }
    .await;

    match &result {
        Ok(()) => {
            emit_agent_install_event(
                emitter,
                &task_id,
                AgentInstallEventKind::Completed,
                format!("{} uninstalled successfully", meta.name),
            );
        }
        Err(e) => {
            emit_agent_install_event(
                emitter,
                &task_id,
                AgentInstallEventKind::Failed,
                e.to_string(),
            );
        }
    }
    result
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_uninstall_agent(
    agent_type: AgentType,
    task_id: String,
    db: State<'_, AppDatabase>,
    app: tauri::AppHandle,
) -> Result<(), AcpError> {
    let emitter = EventEmitter::Tauri(app);
    acp_uninstall_agent_core(agent_type, task_id, &db, &emitter).await
}

/// The npm package that ships the `pi` binary pi-acp spawns as `pi --mode rpc`.
/// It is installed beside the pinned pi-acp adapter in the same private prefix.
const PI_CODING_AGENT_PACKAGE: &str = "@earendil-works/pi-coding-agent";

/// Install the Pi adapter and child command together in one private runtime.
pub(crate) async fn acp_install_pi_binary_core(
    task_id: String,
    db: &AppDatabase,
    emitter: &EventEmitter,
) -> Result<(), AcpError> {
    let _storage_work_guard = crate::acp::agent_storage_work::begin_agent_storage_work().await;
    emit_agent_install_event(emitter, &task_id, AgentInstallEventKind::Started, "");
    let paths = active_agent_storage_paths()?;
    let meta = registry::get_agent_meta(AgentType::Pi);
    agent_setting_service::ensure_defaults(
        &db.conn,
        &[agent_setting_service::AgentDefaultInput {
            agent_type: AgentType::Pi,
            registry_id: registry::registry_id_for(AgentType::Pi).to_string(),
            default_sort_order: i32::MAX / 2,
        }],
    )
    .await
    .map_err(|e| AcpError::protocol(e.to_string()))?;
    let result: Result<(), AcpError> = if let registry::AgentDistribution::Npx {
        package,
        cmd,
        version,
        ..
    } = meta.distribution
    {
        async {
            install_private_npm_package(
                &paths,
                AgentType::Pi,
                version,
                &[package, PI_CODING_AGENT_PACKAGE],
                None,
                None,
                &[cmd, "pi"],
                &task_id,
                emitter,
            )
            .await?;
            crate::commands::agent_storage::ensure_active_agent_profile_layout(
                &db.conn,
                AgentType::Pi,
            )
            .await
            .map_err(|error| AcpError::protocol(error.to_string()))?;
            agent_setting_service::set_installed_version(
                &db.conn,
                AgentType::Pi,
                Some(version.to_string()),
            )
            .await
            .map_err(|e| AcpError::protocol(e.to_string()))?;
            crate::acp::provider_overlay::enforce_active_provider_overlay(AgentType::Pi)
                .map_err(AcpError::protocol)?;
            crate::acp::account_credentials::sync_agent_credentials_for_acp(
                &db.conn,
                AgentType::Pi,
            )
            .await?;
            reconcile_installed_agent_best_effort(db, AgentType::Pi).await;
            Ok(())
        }
        .await
    } else {
        Err(AcpError::protocol("Pi is not an npm Agent"))
    };

    match &result {
        Ok(()) => emit_agent_install_event(
            emitter,
            &task_id,
            AgentInstallEventKind::Completed,
            "pi installed successfully",
        ),
        Err(e) => emit_agent_install_event(
            emitter,
            &task_id,
            AgentInstallEventKind::Failed,
            e.to_string(),
        ),
    }
    result
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_install_pi_binary(
    task_id: String,
    db: State<'_, AppDatabase>,
    app: tauri::AppHandle,
) -> Result<(), AcpError> {
    let emitter = EventEmitter::Tauri(app);
    acp_install_pi_binary_core(task_id, &db, &emitter).await
}

/// Uninstall the coupled private Pi adapter and child runtime.
pub(crate) async fn acp_uninstall_pi_binary_core(
    task_id: String,
    db: &AppDatabase,
    emitter: &EventEmitter,
) -> Result<(), AcpError> {
    let _storage_work_guard = crate::acp::agent_storage_work::begin_agent_storage_work().await;
    emit_agent_install_event(emitter, &task_id, AgentInstallEventKind::Started, "");
    let paths = active_agent_storage_paths()?;
    emit_agent_install_event(
        emitter,
        &task_id,
        AgentInstallEventKind::Log,
        "Removing private Pi runtime",
    );

    let result: Result<(), AcpError> = async {
        npm_runtime::uninstall_private_npm_runtime(&paths, AgentType::Pi)?;
        agent_setting_service::set_installed_version(&db.conn, AgentType::Pi, None)
            .await
            .map_err(|e| AcpError::protocol(e.to_string()))?;
        reconcile_agent_enablement_best_effort(db, AgentType::Pi, false).await;
        Ok(())
    }
    .await;

    match &result {
        Ok(()) => emit_agent_install_event(
            emitter,
            &task_id,
            AgentInstallEventKind::Completed,
            "pi uninstalled successfully",
        ),
        Err(e) => emit_agent_install_event(
            emitter,
            &task_id,
            AgentInstallEventKind::Failed,
            e.to_string(),
        ),
    }
    result
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_uninstall_pi_binary(
    task_id: String,
    db: State<'_, AppDatabase>,
    app: tauri::AppHandle,
) -> Result<(), AcpError> {
    let emitter = EventEmitter::Tauri(app);
    acp_uninstall_pi_binary_core(task_id, &db, &emitter).await
}

pub(crate) async fn acp_reorder_agents_core(
    agent_types: &[AgentType],
    db: &AppDatabase,
    emitter: &EventEmitter,
) -> Result<(), AcpError> {
    if agent_types.is_empty() {
        return Ok(());
    }
    agent_setting_service::reorder(&db.conn, agent_types)
        .await
        .map_err(|e| {
            let message = e.to_string();
            if message.contains("database or disk is full") || message.contains("(code: 13)") {
                AcpError::protocol("无法保存排序：数据库可写空间不足。请释放磁盘空间后重试。")
            } else {
                AcpError::protocol(message)
            }
        })?;
    emit_acp_agents_updated(emitter, "agent_reordered", None);
    Ok(())
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_reorder_agents(
    agent_types: Vec<AgentType>,
    db: State<'_, AppDatabase>,
    app: tauri::AppHandle,
) -> Result<(), AcpError> {
    let emitter = EventEmitter::Tauri(app);
    acp_reorder_agents_core(&agent_types, &db, &emitter).await
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_list_agent_skills(
    agent_type: AgentType,
    workspace_path: Option<String>,
    include_disabled: Option<bool>,
) -> Result<AgentSkillsListResult, AcpError> {
    let Some(spec) = skill_storage_spec(agent_type) else {
        return Ok(AgentSkillsListResult {
            supported: false,
            message: Some(format!("{agent_type} 暂不支持在设置页管理 Skills")),
            locations: Vec::new(),
            skills: Vec::new(),
        });
    };

    let include_disabled = include_disabled.unwrap_or(false);
    let mut locations = Vec::new();
    let mut skills_by_key: BTreeMap<String, AgentSkillItem> = BTreeMap::new();

    let shared_dir = shared_skills_dir();
    locations.push(AgentSkillLocation {
        scope: AgentSkillScope::Global,
        path: shared_dir.to_string_lossy().to_string(),
        exists: shared_dir.exists(),
    });
    for mut skill in list_shared_skills_for_agent(agent_type, include_disabled)? {
        let key = format!("global:{}", skill.id);
        set_skill_read_only(agent_type, &mut skill);
        skills_by_key.entry(key).or_insert(skill);
    }

    if let Some(workspace) = workspace_path.as_deref().map(str::trim) {
        if !workspace.is_empty() {
            for relative in &spec.project_rel_dirs {
                let project_dir = PathBuf::from(workspace).join(relative);
                locations.push(AgentSkillLocation {
                    scope: AgentSkillScope::Project,
                    path: project_dir.to_string_lossy().to_string(),
                    exists: project_dir.exists(),
                });
                let listed = list_skills_from_dir(
                    AgentSkillScope::Project,
                    &project_dir,
                    spec.kind,
                    include_disabled,
                )?;
                for skill in listed {
                    let key = format!("project:{}", skill.id);
                    skills_by_key.entry(key).or_insert(skill);
                }
            }
        }
    }

    let mut skills = skills_by_key.into_values().collect::<Vec<_>>();
    for skill in &mut skills {
        set_skill_read_only(agent_type, skill);
    }
    skills.sort_by(|a, b| {
        scope_rank(a.scope)
            .cmp(&scope_rank(b.scope))
            .then_with(|| a.name.cmp(&b.name))
    });

    Ok(AgentSkillsListResult {
        supported: true,
        message: None,
        locations,
        skills,
    })
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_read_agent_skill(
    agent_type: AgentType,
    scope: AgentSkillScope,
    skill_id: String,
    workspace_path: Option<String>,
) -> Result<AgentSkillContent, AcpError> {
    let Some(spec) = skill_storage_spec(agent_type) else {
        return Err(AcpError::protocol(format!(
            "{agent_type} skills are not supported in Settings yet"
        )));
    };
    let id = validate_skill_id(&skill_id)?;

    if scope == AgentSkillScope::Global {
        if let Ok(skill) = build_shared_skill_item_for_agent(agent_type, id.clone()) {
            let content_path = skill_content_path(skill.layout, Path::new(&skill.path));
            let content = fs::read_to_string(&content_path)
                .map_err(|e| AcpError::protocol(format!("failed to read skill content: {e}")))?;
            return Ok(AgentSkillContent { skill, content });
        }
    }

    let dirs = scoped_skill_dirs(agent_type, scope, workspace_path.as_deref())?;

    let mut skill = locate_existing_skill_across_dirs(&dirs, spec.kind, &id, scope, true)
        .ok_or_else(|| AcpError::protocol(format!("skill not found: {id}")))?;
    set_skill_read_only(agent_type, &mut skill);
    let content_path = skill_content_path(skill.layout, Path::new(&skill.path));
    let content = fs::read_to_string(&content_path)
        .map_err(|e| AcpError::protocol(format!("failed to read skill content: {e}")))?;
    Ok(AgentSkillContent { skill, content })
}

pub async fn acp_take_over_agent_skill_core(
    conn: &sea_orm::DatabaseConnection,
    agent_type: AgentType,
    skill_id: String,
    sync_mode: Option<AgentSkillSyncMode>,
) -> Result<AgentSkillItem, AcpError> {
    let _paths = require_private_agent_storage_for_write()?;
    let Some(spec) = skill_storage_spec(agent_type) else {
        return Err(AcpError::protocol(format!(
            "{agent_type} skills are not supported in Settings yet"
        )));
    };
    let id = validate_skill_id(&skill_id)?;
    let targets = installed_enabled_skill_agent_types(conn, agent_type).await?;
    take_over_read_only_global_native_skill(
        agent_type,
        &spec,
        &id,
        sync_mode.unwrap_or_default(),
        &targets,
    )
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn acp_take_over_agent_skill(
    agent_type: AgentType,
    skill_id: String,
    sync_mode: Option<AgentSkillSyncMode>,
    db: State<'_, AppDatabase>,
) -> Result<AgentSkillItem, AcpError> {
    acp_take_over_agent_skill_core(&db.conn, agent_type, skill_id, sync_mode).await
}

#[allow(clippy::too_many_arguments)]
/// `official: Some(true)` marks the save as an official-market (re)install:
/// the shared source is stamped with the official marker, and the save may
/// overwrite an already-marked skill. Any other save to a marked skill is
/// refused so market-managed content stays immutable for users.
pub async fn acp_save_agent_skill_core(
    conn: &sea_orm::DatabaseConnection,
    agent_type: AgentType,
    scope: AgentSkillScope,
    skill_id: String,
    content: String,
    files: Option<Vec<AgentSkillFile>>,
    workspace_path: Option<String>,
    layout: Option<AgentSkillLayout>,
    sync_mode: Option<AgentSkillSyncMode>,
    official: Option<bool>,
) -> Result<AgentSkillItem, AcpError> {
    let Some(spec) = skill_storage_spec(agent_type) else {
        return Err(AcpError::protocol(format!(
            "{agent_type} skills are not supported in Settings yet"
        )));
    };
    let id = validate_skill_id(&skill_id)?;
    let imported_directory = validate_skill_directory(files)?;
    if imported_directory.is_some() && layout == Some(AgentSkillLayout::MarkdownFile) {
        return Err(AcpError::protocol(
            "skill folder imports require the skill_directory layout",
        ));
    }
    let content = imported_directory
        .as_ref()
        .map(|directory| directory.skill_content.clone())
        .unwrap_or(content);

    if scope == AgentSkillScope::Global {
        let targets = installed_enabled_skill_agent_types(conn, agent_type).await?;
        let _paths = require_private_agent_storage_for_write()?;
        ensure_shared_skill_writable(&id)?;
        let _guard = shared_skill_mutation_guard();
        let official_install = official.unwrap_or(false);
        if !official_install {
            ensure_shared_skill_not_official(&id)?;
        }
        let source = shared_skill_path(&id);
        let existed = source.join("SKILL.md").is_file();
        let was_copy_mode = if existed {
            shared_skill_publish_status(agent_type, &source, &id)?.1
        } else {
            false
        };

        if let Some(directory) = imported_directory.as_ref() {
            write_skill_directory(&source, &directory.files)?;
        } else {
            fs::create_dir_all(&source)
                .map_err(|e| AcpError::protocol(format!("failed to create shared skill: {e}")))?;
            let content_path = source.join("SKILL.md");
            fs::write(&content_path, &content)
                .map_err(|e| AcpError::protocol(format!("failed to write skill content: {e}")))?;
        }
        if official_install {
            write_official_skill_marker(&source, &id)?;
        }
        write_shared_skill_publish_state(&source, &id, true)?;

        let mode = sync_mode.unwrap_or(if was_copy_mode {
            AgentSkillSyncMode::Copy
        } else {
            AgentSkillSyncMode::Symlink
        });
        return publish_shared_skill_to_active_agents_locked(agent_type, &targets, &id, mode);
    }

    let dirs = scoped_skill_dirs(agent_type, scope, workspace_path.as_deref())?;
    let preferred_dir = preferred_scope_skill_dir(agent_type, scope, workspace_path.as_deref())?;

    fs::create_dir_all(&preferred_dir)
        .map_err(|e| AcpError::protocol(format!("failed to create skills directory: {e}")))?;

    let existing = locate_existing_skill_across_dirs(&dirs, spec.kind, &id, scope, true);
    if let Some(ref item) = existing {
        if is_read_only_skill_path(agent_type, Path::new(&item.path)) {
            return Err(AcpError::protocol(format!(
                "skill '{id}' is a built-in system skill and cannot be modified"
            )));
        }
    }
    let existing_path = existing.as_ref().map(|item| PathBuf::from(&item.path));
    let mut skill = if imported_directory.is_some() {
        build_skill_item(
            id.clone(),
            scope,
            AgentSkillLayout::SkillDirectory,
            preferred_dir.join(&id),
            true,
        )
    } else if let Some(item) = existing {
        item
    } else {
        let new_layout = match spec.kind {
            SkillStorageKind::SkillDirectoryOnly => AgentSkillLayout::SkillDirectory,
            SkillStorageKind::SkillDirectoryOrMarkdownFile => {
                layout.unwrap_or(AgentSkillLayout::MarkdownFile)
            }
        };
        let skill_path = match new_layout {
            AgentSkillLayout::SkillDirectory => preferred_dir.join(&id),
            AgentSkillLayout::MarkdownFile => preferred_dir.join(format!("{id}.md")),
        };
        build_skill_item(id.clone(), scope, new_layout, skill_path, true)
    };

    let skill_path = PathBuf::from(&skill.path);
    let content_path = skill_content_path(skill.layout, &skill_path);

    if let Some(directory) = imported_directory.as_ref() {
        write_skill_directory(&skill_path, &directory.files)?;
        if let Some(old_path) = existing_path.filter(|path| path != &skill_path) {
            remove_skill_entry(&old_path).map_err(|e| {
                AcpError::protocol(format!("failed to remove previous skill entry: {e}"))
            })?;
        }
        skill.description = read_skill_description(&content_path);
        return Ok(skill);
    }

    if skill.layout == AgentSkillLayout::SkillDirectory {
        fs::create_dir_all(&skill_path).map_err(|e| {
            AcpError::protocol(format!(
                "failed to create skill directory '{}': {e}",
                skill.path
            ))
        })?;
    } else if let Some(parent) = content_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            AcpError::protocol(format!("failed to create skill parent directory: {e}"))
        })?;
    }

    fs::write(&content_path, content)
        .map_err(|e| AcpError::protocol(format!("failed to write skill content: {e}")))?;

    skill.description = read_skill_description(&content_path);

    Ok(skill)
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn acp_save_agent_skill(
    agent_type: AgentType,
    scope: AgentSkillScope,
    skill_id: String,
    content: String,
    files: Option<Vec<AgentSkillFile>>,
    workspace_path: Option<String>,
    layout: Option<AgentSkillLayout>,
    sync_mode: Option<AgentSkillSyncMode>,
    official: Option<bool>,
    db: State<'_, AppDatabase>,
) -> Result<AgentSkillItem, AcpError> {
    acp_save_agent_skill_core(
        &db.conn,
        agent_type,
        scope,
        skill_id,
        content,
        files,
        workspace_path,
        layout,
        sync_mode,
        official,
    )
    .await
}

pub async fn acp_set_agent_skill_enabled_core(
    conn: &sea_orm::DatabaseConnection,
    agent_type: AgentType,
    scope: AgentSkillScope,
    skill_id: String,
    workspace_path: Option<String>,
    enabled: bool,
    sync_mode: Option<AgentSkillSyncMode>,
) -> Result<AgentSkillItem, AcpError> {
    let Some(spec) = skill_storage_spec(agent_type) else {
        return Err(AcpError::protocol(format!(
            "{agent_type} skills are not supported in Settings yet"
        )));
    };
    let id = validate_skill_id(&skill_id)?;

    if scope == AgentSkillScope::Global {
        let targets = if enabled {
            Some(installed_enabled_skill_agent_types(conn, agent_type).await?)
        } else {
            None
        };
        let _paths = require_private_agent_storage_for_write()?;
        ensure_shared_skill_writable(&id)?;
        let _guard = shared_skill_mutation_guard();
        let source = shared_skill_path(&id);
        if enabled {
            write_shared_skill_publish_state(&source, &id, true)?;
            return publish_shared_skill_to_active_agents_locked(
                agent_type,
                targets.as_deref().unwrap_or_default(),
                &id,
                sync_mode.unwrap_or_default(),
            );
        }
        ensure_market_dependency_not_in_use(&id)?;
        write_shared_skill_publish_state(&source, &id, false)?;
        remove_shared_skill_publications_locked(&id)?;
        return build_shared_skill_item_for_agent(agent_type, id);
    }

    let dirs = scoped_skill_dirs(agent_type, scope, workspace_path.as_deref())?;
    let active = locate_existing_skill_across_dirs(&dirs, spec.kind, &id, scope, false);

    if enabled {
        if let Some(mut skill) = active {
            set_skill_read_only(agent_type, &mut skill);
            return Ok(skill);
        }
        let disabled_skill = locate_disabled_skill_across_dirs(&dirs, spec.kind, &id, scope)
            .ok_or_else(|| AcpError::protocol(format!("skill not found: {id}")))?;
        let target = active_path_for_disabled_skill(&disabled_skill)?;
        move_skill_entry(Path::new(&disabled_skill.path), &target)?;
        let mut skill = build_skill_item(id, scope, disabled_skill.layout, target, true);
        set_skill_read_only(agent_type, &mut skill);
        return Ok(skill);
    }

    if active.is_none() {
        let mut skill = locate_disabled_skill_across_dirs(&dirs, spec.kind, &id, scope)
            .ok_or_else(|| AcpError::protocol(format!("skill not found: {id}")))?;
        set_skill_read_only(agent_type, &mut skill);
        return Ok(skill);
    }

    let mut skill = active.expect("active checked above");
    set_skill_read_only(agent_type, &mut skill);
    if skill.read_only {
        return Err(AcpError::protocol(format!(
            "skill '{id}' is a built-in system skill and cannot be disabled"
        )));
    }
    let target = disabled_path_for_active_skill(&skill)?;
    move_skill_entry(Path::new(&skill.path), &target)?;
    Ok(build_skill_item(id, scope, skill.layout, target, false))
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn acp_set_agent_skill_enabled(
    agent_type: AgentType,
    scope: AgentSkillScope,
    skill_id: String,
    workspace_path: Option<String>,
    enabled: bool,
    sync_mode: Option<AgentSkillSyncMode>,
    db: State<'_, AppDatabase>,
) -> Result<AgentSkillItem, AcpError> {
    acp_set_agent_skill_enabled_core(
        &db.conn,
        agent_type,
        scope,
        skill_id,
        workspace_path,
        enabled,
        sync_mode,
    )
    .await
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn acp_delete_agent_skill(
    agent_type: AgentType,
    scope: AgentSkillScope,
    skill_id: String,
    workspace_path: Option<String>,
) -> Result<(), AcpError> {
    let Some(spec) = skill_storage_spec(agent_type) else {
        return Err(AcpError::protocol(format!(
            "{agent_type} skills are not supported in Settings yet"
        )));
    };
    let id = validate_skill_id(&skill_id)?;

    if scope == AgentSkillScope::Global {
        let _paths = require_private_agent_storage_for_write()?;
        ensure_shared_skill_writable(&id)?;
        let _guard = shared_skill_mutation_guard();
        let skill_path = shared_skill_path(&id);
        if !skill_path.join("SKILL.md").is_file() {
            return Err(AcpError::protocol(format!("skill not found: {id}")));
        }
        ensure_market_dependency_not_in_use(&id)?;
        write_shared_skill_publish_state(&skill_path, &id, false)?;
        remove_shared_skill_publications_locked(&id)?;
        remove_skill_entry(&skill_path)
            .map_err(|e| AcpError::protocol(format!("failed to delete skill entry: {e}")))?;
        return Ok(());
    }

    let dirs = scoped_skill_dirs(agent_type, scope, workspace_path.as_deref())?;

    let skill = locate_existing_skill_across_dirs(&dirs, spec.kind, &id, scope, true)
        .ok_or_else(|| AcpError::protocol(format!("skill not found: {id}")))?;
    if is_read_only_skill_path(agent_type, Path::new(&skill.path)) {
        return Err(AcpError::protocol(format!(
            "skill '{id}' is a built-in system skill and cannot be deleted"
        )));
    }
    let skill_path = PathBuf::from(&skill.path);
    remove_skill_entry(&skill_path)
        .map_err(|e| AcpError::protocol(format!("failed to delete skill entry: {e}")))?;
    Ok(())
}

pub(crate) async fn opencode_list_plugins_core() -> Result<PluginCheckSummary, AcpError> {
    opencode_plugins::check_opencode_plugins(None).map_err(AcpError::Protocol)
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn opencode_list_plugins() -> Result<PluginCheckSummary, AcpError> {
    opencode_list_plugins_core().await
}

pub(crate) async fn opencode_provider_catalog_core(
    data_dir: &Path,
    force_refresh: bool,
) -> Vec<crate::acp::opencode_catalog::CatalogProvider> {
    crate::acp::opencode_catalog::provider_catalog(data_dir, force_refresh).await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn opencode_provider_catalog(
    force_refresh: Option<bool>,
    app_handle: tauri::AppHandle,
) -> Result<Vec<crate::acp::opencode_catalog::CatalogProvider>, AcpError> {
    let data_dir = app_handle
        .path()
        .app_data_dir()
        .map(|p| crate::paths::resolve_effective_data_dir(&p))
        .unwrap_or_else(|_| PathBuf::from("."));
    Ok(opencode_provider_catalog_core(&data_dir, force_refresh.unwrap_or(false)).await)
}

pub(crate) async fn opencode_install_plugins_core(
    names: Option<Vec<String>>,
    task_id: String,
    emitter: &EventEmitter,
) -> Result<(), AcpError> {
    let _storage_work_guard = crate::acp::agent_storage_work::begin_agent_storage_work().await;
    opencode_plugins::install_missing_plugins(names, task_id, emitter)
        .await
        .map_err(AcpError::Protocol)
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn opencode_install_plugins(
    names: Option<Vec<String>>,
    task_id: String,
    app: tauri::AppHandle,
) -> Result<(), AcpError> {
    let emitter = EventEmitter::Tauri(app);
    opencode_install_plugins_core(names, task_id, &emitter).await
}

pub(crate) async fn opencode_uninstall_plugin_core(
    name: String,
) -> Result<PluginCheckSummary, AcpError> {
    let _storage_work_guard = crate::acp::agent_storage_work::begin_agent_storage_work().await;
    opencode_plugins::uninstall_plugin(name)
        .await
        .map_err(AcpError::Protocol)
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn opencode_uninstall_plugin(name: String) -> Result<PluginCheckSummary, AcpError> {
    opencode_uninstall_plugin_core(name).await
}

// ─── Codex Device Code OAuth ───

const CODEX_OAUTH_ISSUER: &str = "https://auth.openai.com";
const CODEX_OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexDeviceCodeResponse {
    pub user_code: String,
    pub verification_url: String,
    pub device_auth_id: String,
    pub interval: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexDeviceCodePollResult {
    pub status: String,
    pub message: Option<String>,
    pub id_token: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub account_id: Option<String>,
}

#[derive(Deserialize)]
struct DeviceCodeUserCodeResp {
    device_auth_id: String,
    #[serde(alias = "usercode")]
    user_code: String,
    #[serde(
        default = "default_interval",
        deserialize_with = "deserialize_interval"
    )]
    interval: u64,
}

fn default_interval() -> u64 {
    5
}

fn extract_jwt_account_id(jwt: &str) -> Option<String> {
    let payload = jwt.split('.').nth(1)?;
    let decoded =
        base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, payload).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    value
        .get("https://api.openai.com/auth")
        .and_then(|auth| auth.get("chatgpt_account_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn deserialize_interval<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;
    let value = serde_json::Value::deserialize(deserializer)?;
    match &value {
        serde_json::Value::Number(n) => n
            .as_u64()
            .ok_or_else(|| de::Error::custom(format!("invalid interval number: {n}"))),
        serde_json::Value::String(s) => s.trim().parse::<u64>().map_err(de::Error::custom),
        _ => Err(de::Error::custom(format!(
            "unexpected interval type: {value}"
        ))),
    }
}

#[derive(Deserialize)]
struct DeviceCodeTokenResp {
    authorization_code: String,
    #[allow(dead_code)]
    code_challenge: String,
    code_verifier: String,
}

#[derive(Deserialize)]
struct OAuthTokenResp {
    id_token: String,
    access_token: String,
    refresh_token: String,
}

pub(crate) async fn codex_request_device_code_core() -> Result<CodexDeviceCodeResponse, AcpError> {
    let client = reqwest::Client::new();
    let url = format!("{CODEX_OAUTH_ISSUER}/api/accounts/deviceauth/usercode");
    let body = serde_json::json!({ "client_id": CODEX_OAUTH_CLIENT_ID });

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| AcpError::protocol(format!("device code request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(AcpError::protocol(format!(
            "device code request returned {status}: {text}"
        )));
    }

    let raw_body = resp
        .text()
        .await
        .map_err(|e| AcpError::protocol(format!("read device code response failed: {e}")))?;
    let uc: DeviceCodeUserCodeResp = serde_json::from_str(&raw_body).map_err(|e| {
        AcpError::protocol(format!(
            "parse device code response failed: {e} | body: {raw_body}"
        ))
    })?;

    Ok(CodexDeviceCodeResponse {
        user_code: uc.user_code,
        verification_url: format!("{CODEX_OAUTH_ISSUER}/codex/device"),
        device_auth_id: uc.device_auth_id,
        interval: uc.interval,
    })
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn codex_request_device_code() -> Result<CodexDeviceCodeResponse, AcpError> {
    codex_request_device_code_core().await
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn codex_poll_device_code(
    device_auth_id: String,
    user_code: String,
) -> Result<CodexDeviceCodePollResult, AcpError> {
    codex_poll_device_code_core(device_auth_id, user_code).await
}

pub(crate) async fn codex_poll_device_code_core(
    device_auth_id: String,
    user_code: String,
) -> Result<CodexDeviceCodePollResult, AcpError> {
    let client = reqwest::Client::new();
    let poll_url = format!("{CODEX_OAUTH_ISSUER}/api/accounts/deviceauth/token");
    let poll_body = serde_json::json!({
        "device_auth_id": device_auth_id,
        "user_code": user_code,
    });

    let resp = client
        .post(&poll_url)
        .json(&poll_body)
        .send()
        .await
        .map_err(|e| AcpError::protocol(format!("device code poll failed: {e}")))?;

    if !resp.status().is_success() {
        return Ok(CodexDeviceCodePollResult {
            status: "pending".into(),
            message: None,
            id_token: None,
            access_token: None,
            refresh_token: None,
            account_id: None,
        });
    }

    let code_resp: DeviceCodeTokenResp = resp
        .json()
        .await
        .map_err(|e| AcpError::protocol(format!("parse poll response failed: {e}")))?;

    let redirect_uri = format!("{CODEX_OAUTH_ISSUER}/deviceauth/callback");
    let token_url = format!("{CODEX_OAUTH_ISSUER}/oauth/token");

    let token_resp = client
        .post(&token_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!(
            "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&code_verifier={}",
            urlencoding::encode(&code_resp.authorization_code),
            urlencoding::encode(&redirect_uri),
            urlencoding::encode(CODEX_OAUTH_CLIENT_ID),
            urlencoding::encode(&code_resp.code_verifier),
        ))
        .send()
        .await
        .map_err(|e| AcpError::protocol(format!("token exchange failed: {e}")))?;

    if !token_resp.status().is_success() {
        let status = token_resp.status();
        let text = token_resp.text().await.unwrap_or_default();
        return Ok(CodexDeviceCodePollResult {
            status: "error".into(),
            message: Some(format!("token exchange returned {status}: {text}")),
            id_token: None,
            access_token: None,
            refresh_token: None,
            account_id: None,
        });
    }

    let tokens: OAuthTokenResp = token_resp
        .json()
        .await
        .map_err(|e| AcpError::protocol(format!("parse token response failed: {e}")))?;

    let account_id = extract_jwt_account_id(&tokens.id_token).unwrap_or_default();

    Ok(CodexDeviceCodePollResult {
        status: "success".into(),
        message: None,
        id_token: Some(tokens.id_token),
        access_token: Some(tokens.access_token),
        refresh_token: Some(tokens.refresh_token),
        account_id: Some(account_id),
    })
}
