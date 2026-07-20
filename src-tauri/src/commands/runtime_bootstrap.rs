//! First-run bootstrap of the managed Node.js and Git runtimes.
//!
//! Historically the NSIS installer bundled and unpacked these runtimes during
//! setup. That coupled a ~90 MB download to the installer build, ran fragile
//! PowerShell in whatever environment launched the installer, and offered no
//! progress UI. The install now happens here, on the app's initialization
//! screen: mirror-first download (npmmirror for mainland-China acceleration,
//! official hosts as fallback), pinned SHA-256 verification, zip extraction
//! into `runtime/staging`, an executable smoke test, then an atomic move into
//! the layout `process::managed_node` / `process::managed_git` already read:
//!
//! ```text
//! <root>/runtime/node/<version>/<platform>/node.exe   + node/current.json
//! <root>/runtime/git/<version>/<platform>/cmd/git.exe + git/current.json
//! ```
//!
//! Progress streams to the UI as `app://runtime-bootstrap` events tagged with
//! the caller's `task_id`, mirroring the OfficeCLI install channel.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use crate::web::event_bridge::EventEmitter;

// ─── Pinned versions and checksums ──────────────────────────────────────
//
// Node.js 24 dropped 32-bit Windows; x86 stays on the 22 LTS line, matching
// what the NSIS installer previously shipped. Hashes come from the official
// SHASUMS256.txt / Git-for-Windows release notes so a tampered mirror can
// never produce an installable runtime.

const NODE_VERSION_X64: &str = "24.0.0";
const NODE_VERSION_X86: &str = "22.23.1";
const GIT_VERSION: &str = "2.55.0.2";
const GIT_RELEASE_TAG: &str = "v2.55.0.windows.2";
const GIT_VERSION_OUTPUT: &str = "git version 2.55.0.windows.2";

const NODE_MIRROR_BASE: &str = "https://registry.npmmirror.com/-/binary/node";
const NODE_OFFICIAL_BASE: &str = "https://nodejs.org/dist";
const GIT_MIRROR_BASE: &str = "https://registry.npmmirror.com/-/binary/git-for-windows";
const GIT_OFFICIAL_BASE: &str = "https://github.com/git-for-windows/git/releases/download";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ComponentKind {
    Node,
    Git,
}

impl ComponentKind {
    fn name(self) -> &'static str {
        match self {
            ComponentKind::Node => "node",
            ComponentKind::Git => "git",
        }
    }
}

#[derive(Debug, Clone)]
struct ComponentSpec {
    kind: ComponentKind,
    version: &'static str,
    platform: &'static str,
    asset: String,
    sha256: &'static str,
    mirror_url: String,
    official_url: String,
    /// Node zips wrap everything in `node-v<V>-<platform>/`; MinGit zips
    /// extract their payload at the archive root.
    archive_root: Option<String>,
}

fn node_spec(version: &'static str, platform: &'static str, sha256: &'static str) -> ComponentSpec {
    let asset = format!("node-v{version}-{platform}.zip");
    ComponentSpec {
        kind: ComponentKind::Node,
        version,
        platform,
        mirror_url: format!("{NODE_MIRROR_BASE}/v{version}/{asset}"),
        official_url: format!("{NODE_OFFICIAL_BASE}/v{version}/{asset}"),
        archive_root: Some(format!("node-v{version}-{platform}")),
        asset,
        sha256,
    }
}

fn git_spec(
    asset_arch: &'static str,
    platform: &'static str,
    sha256: &'static str,
) -> ComponentSpec {
    let asset = format!("MinGit-{GIT_VERSION}-{asset_arch}.zip");
    ComponentSpec {
        kind: ComponentKind::Git,
        version: GIT_VERSION,
        platform,
        mirror_url: format!("{GIT_MIRROR_BASE}/{GIT_RELEASE_TAG}/{asset}"),
        official_url: format!("{GIT_OFFICIAL_BASE}/{GIT_RELEASE_TAG}/{asset}"),
        archive_root: None,
        asset,
        sha256,
    }
}

/// Specs for the CPU architecture this binary was built for, or `None` on
/// architectures without a managed runtime (e.g. non-Windows builds never
/// call this).
fn specs_for_current_arch() -> Option<(ComponentSpec, ComponentSpec)> {
    match std::env::consts::ARCH {
        "x86_64" => Some((
            node_spec(
                NODE_VERSION_X64,
                "win-x64",
                "3d0fff80c87bb9a8d7f49f2f27832aa34a1477d137af46f5b14df5498be81304",
            ),
            git_spec(
                "64-bit",
                "win-x64",
                "e3ea2944cea4b3fabcd69c7c1669ef69b1b66c05ac7806d81224d0abad2dec31",
            ),
        )),
        "aarch64" => Some((
            node_spec(
                NODE_VERSION_X64,
                "win-arm64",
                "03b6676f4872fbe4645113de8e23da834a7c1464045369f2b7a374bf482a5e12",
            ),
            git_spec(
                "arm64",
                "win-arm64",
                "0b2b81fdce284efd174cbb51b886ccea2fd271679c4b5c21f07d9e03bae51413",
            ),
        )),
        "x86" => Some((
            node_spec(
                NODE_VERSION_X86,
                "win-x86",
                "e298b368aad86c571447a3650db3ce19063373ffd39d6d73d014a5d9ad31dc62",
            ),
            git_spec(
                "32-bit",
                "win-x86",
                "04009f6150c1cec2d6779c51406c8c6a3f0133e57fa91c91eb8a030b93e68ccb",
            ),
        )),
        _ => None,
    }
}

// ─── Events ─────────────────────────────────────────────────────────────

const RUNTIME_BOOTSTRAP_EVENT: &str = "app://runtime-bootstrap";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
enum RuntimeBootstrapEventKind {
    Started,
    Log,
    Progress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
struct RuntimeBootstrapEvent {
    task_id: String,
    kind: RuntimeBootstrapEventKind,
    component: Option<&'static str>,
    percent: Option<u8>,
    payload: String,
}

fn emit(
    emitter: &EventEmitter,
    task_id: &str,
    kind: RuntimeBootstrapEventKind,
    component: Option<ComponentKind>,
    percent: Option<u8>,
    payload: impl Into<String>,
) {
    crate::web::event_bridge::emit_event(
        emitter,
        RUNTIME_BOOTSTRAP_EVENT,
        RuntimeBootstrapEvent {
            task_id: task_id.to_string(),
            kind,
            component: component.map(ComponentKind::name),
            percent,
            payload: payload.into(),
        },
    );
}

// ─── Public report ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeComponentStatus {
    /// Already usable (managed runtime or system PATH).
    Ready,
    /// Installed by this bootstrap run.
    Installed,
    /// Not applicable on this platform; nothing was attempted.
    Skipped,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeComponentReport {
    pub status: RuntimeComponentStatus,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeBootstrapReport {
    pub node: RuntimeComponentReport,
    pub git: RuntimeComponentReport,
}

// ─── Concurrency ────────────────────────────────────────────────────────

fn bootstrap_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

// ─── Entry points ───────────────────────────────────────────────────────

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn runtime_bootstrap(
    task_id: String,
    app: tauri::AppHandle,
) -> Result<RuntimeBootstrapReport, String> {
    Ok(runtime_bootstrap_core(task_id, &EventEmitter::Tauri(app)).await)
}

pub async fn runtime_bootstrap_core(
    task_id: String,
    emitter: &EventEmitter,
) -> RuntimeBootstrapReport {
    // Serialize concurrent callers (second webview, retry raced with a slow
    // first run): the loser waits, then sees the winner's runtimes as Ready.
    let _guard = bootstrap_lock().lock().await;
    emit(
        emitter,
        &task_id,
        RuntimeBootstrapEventKind::Started,
        None,
        None,
        "",
    );

    let (node_spec, git_spec) = match specs_for_current_arch() {
        Some(specs) => specs,
        None => {
            let report = RuntimeBootstrapReport {
                node: probe_only_report("node"),
                git: probe_only_report("git"),
            };
            emit(
                emitter,
                &task_id,
                RuntimeBootstrapEventKind::Completed,
                None,
                None,
                "no managed runtime for this platform",
            );
            return report;
        }
    };

    // Node and Git are independent installs into disjoint directories, so
    // download and unpack them concurrently — first launch stays as short as
    // the slower of the two archives instead of their sum.
    let (node, git) = tokio::join!(
        ensure_component(&node_spec, &task_id, emitter),
        ensure_component(&git_spec, &task_id, emitter)
    );

    if node.status == RuntimeComponentStatus::Installed
        || git.status == RuntimeComponentStatus::Installed
    {
        // Make the freshly installed runtimes visible to everything spawned
        // later in this session (the Codex npx install that follows on the
        // init screen needs node on PATH immediately, not after a restart).
        crate::process::ensure_managed_tools_in_path();
    }

    let overall_failed = node.status == RuntimeComponentStatus::Failed
        || git.status == RuntimeComponentStatus::Failed;
    emit(
        emitter,
        &task_id,
        if overall_failed {
            RuntimeBootstrapEventKind::Failed
        } else {
            RuntimeBootstrapEventKind::Completed
        },
        None,
        None,
        "",
    );

    RuntimeBootstrapReport { node, git }
}

/// Non-Windows (and unknown-arch) builds never install a managed runtime;
/// they only report whether the tool is reachable so the UI can proceed.
fn probe_only_report(binary: &str) -> RuntimeComponentReport {
    match which::which(binary) {
        Ok(path) => RuntimeComponentReport {
            status: RuntimeComponentStatus::Ready,
            detail: Some(path.to_string_lossy().into_owned()),
        },
        Err(_) => RuntimeComponentReport {
            status: RuntimeComponentStatus::Skipped,
            detail: Some(format!("{binary} not found in PATH")),
        },
    }
}

// ─── Component install pipeline ─────────────────────────────────────────

async fn ensure_component(
    spec: &ComponentSpec,
    task_id: &str,
    emitter: &EventEmitter,
) -> RuntimeComponentReport {
    let binary = spec.kind.name();
    // Startup already prepends any managed runtime to PATH, so a plain PATH
    // probe covers both a prior managed install and a system-wide tool.
    if let Ok(path) = which::which(binary) {
        return RuntimeComponentReport {
            status: RuntimeComponentStatus::Ready,
            detail: Some(path.to_string_lossy().into_owned()),
        };
    }

    if !cfg!(windows) {
        return probe_only_report(binary);
    }

    let Some(runtime_root) = resolve_runtime_root() else {
        return failed_report(
            spec,
            task_id,
            emitter,
            "no install root or data directory to place the managed runtime in".to_string(),
        );
    };

    match install_component(spec, &runtime_root, task_id, emitter).await {
        Ok(target) => {
            emit(
                emitter,
                task_id,
                RuntimeBootstrapEventKind::Completed,
                Some(spec.kind),
                Some(100),
                target.to_string_lossy().into_owned(),
            );
            RuntimeComponentReport {
                status: RuntimeComponentStatus::Installed,
                detail: Some(target.to_string_lossy().into_owned()),
            }
        }
        Err(error) => failed_report(spec, task_id, emitter, error),
    }
}

fn failed_report(
    spec: &ComponentSpec,
    task_id: &str,
    emitter: &EventEmitter,
    error: String,
) -> RuntimeComponentReport {
    emit(
        emitter,
        task_id,
        RuntimeBootstrapEventKind::Failed,
        Some(spec.kind),
        None,
        error.clone(),
    );
    RuntimeComponentReport {
        status: RuntimeComponentStatus::Failed,
        detail: Some(error),
    }
}

/// Where `runtime/` lives. Installed desktops expose the logical root via
/// `IYW_CLAW_INSTALL_ROOT`; server deployments via `IYW_CLAW_DATA_DIR`; as a
/// last resort derive it from the `<root>/app/iyw-claw.exe` layout.
fn resolve_runtime_root() -> Option<PathBuf> {
    if let Some(root) = std::env::var_os(crate::desktop_bootstrap::INSTALL_ROOT_ENV) {
        if !root.is_empty() {
            return Some(PathBuf::from(root).join("runtime"));
        }
    }
    if let Some(root) = std::env::var_os("IYW_CLAW_DATA_DIR") {
        if !root.is_empty() {
            return Some(PathBuf::from(root).join("runtime"));
        }
    }
    let executable = std::env::current_exe().ok()?;
    crate::desktop_bootstrap::resolve_install_root(&executable).map(|root| root.join("runtime"))
}

async fn install_component(
    spec: &ComponentSpec,
    runtime_root: &Path,
    task_id: &str,
    emitter: &EventEmitter,
) -> Result<PathBuf, String> {
    let downloads_dir = runtime_root.join("downloads");
    let staging_dir = runtime_root.join("staging").join(format!(
        "{}-{}",
        spec.kind.name(),
        uuid::Uuid::new_v4().simple()
    ));
    tokio::fs::create_dir_all(&downloads_dir)
        .await
        .map_err(|e| format!("failed to create {}: {e}", downloads_dir.display()))?;

    let archive_path = downloads_dir.join(&spec.asset);
    let result = async {
        download_verified(spec, &archive_path, task_id, emitter).await?;

        emit(
            emitter,
            task_id,
            RuntimeBootstrapEventKind::Log,
            Some(spec.kind),
            None,
            format!("extracting {}", spec.asset),
        );
        let staged = extract_staged(&archive_path, &staging_dir, spec).await?;
        verify_staged_runtime(&staged, spec).await?;
        finalize_component(&staged, runtime_root, spec)
    }
    .await;

    // Best-effort staging cleanup on both success (dir was moved away, its
    // now-empty parent remains) and failure (partial extraction).
    let _ = tokio::fs::remove_dir_all(&staging_dir).await;
    result
}

// ─── Download ───────────────────────────────────────────────────────────

async fn download_verified(
    spec: &ComponentSpec,
    dest: &Path,
    task_id: &str,
    emitter: &EventEmitter,
) -> Result<(), String> {
    if archive_matches(dest, spec.sha256).await {
        emit(
            emitter,
            task_id,
            RuntimeBootstrapEventKind::Log,
            Some(spec.kind),
            None,
            format!("using cached {}", spec.asset),
        );
        return Ok(());
    }

    let client = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;

    let mut last_error = String::new();
    for (label, url) in [
        ("mirror", &spec.mirror_url),
        ("official", &spec.official_url),
    ] {
        emit(
            emitter,
            task_id,
            RuntimeBootstrapEventKind::Log,
            Some(spec.kind),
            None,
            format!("downloading {} from {label} source", spec.asset),
        );
        match download_once(&client, url, dest, spec, task_id, emitter).await {
            Ok(()) => return Ok(()),
            Err(error) => {
                emit(
                    emitter,
                    task_id,
                    RuntimeBootstrapEventKind::Log,
                    Some(spec.kind),
                    None,
                    format!("{label} source failed: {error}"),
                );
                last_error = format!("{label}: {error}");
            }
        }
    }
    Err(format!(
        "all download sources failed for {} — last error {last_error}",
        spec.asset
    ))
}

async fn download_once(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    spec: &ComponentSpec,
    task_id: &str,
    emitter: &EventEmitter,
) -> Result<(), String> {
    let response = client
        .get(url)
        .timeout(DOWNLOAD_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }

    let total = response.content_length();
    let partial_path = dest.with_extension("part");
    let mut file = tokio::fs::File::create(&partial_path)
        .await
        .map_err(|e| format!("failed to create {}: {e}", partial_path.display()))?;
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let mut last_percent: Option<u8> = None;

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("download interrupted: {e}"))?;
        hasher.update(&chunk);
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("failed to write download: {e}"))?;
        downloaded += chunk.len() as u64;
        if let Some(total) = total.filter(|total| *total > 0) {
            let percent = ((downloaded.min(total) * 100) / total) as u8;
            if last_percent != Some(percent) {
                last_percent = Some(percent);
                emit(
                    emitter,
                    task_id,
                    RuntimeBootstrapEventKind::Progress,
                    Some(spec.kind),
                    Some(percent),
                    String::new(),
                );
            }
        }
    }
    file.flush()
        .await
        .map_err(|e| format!("failed to flush download: {e}"))?;
    drop(file);

    let actual = format!("{:x}", hasher.finalize());
    if !actual.eq_ignore_ascii_case(spec.sha256) {
        let _ = tokio::fs::remove_file(&partial_path).await;
        return Err("SHA-256 checksum mismatch".to_string());
    }
    tokio::fs::rename(&partial_path, dest)
        .await
        .map_err(|e| format!("failed to finalize download: {e}"))
}

async fn archive_matches(path: &Path, expected_sha256: &str) -> bool {
    let path = path.to_path_buf();
    let expected = expected_sha256.to_string();
    tokio::task::spawn_blocking(move || match std::fs::File::open(&path) {
        Ok(mut file) => {
            let mut hasher = Sha256::new();
            if std::io::copy(&mut file, &mut hasher).is_err() {
                return false;
            }
            format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(&expected)
        }
        Err(_) => false,
    })
    .await
    .unwrap_or(false)
}

// ─── Extract, verify, finalize ──────────────────────────────────────────

/// Extract the archive under `staging_dir` and return the directory holding
/// the runtime payload (the wrapping folder for Node zips, `staging_dir`
/// itself for MinGit).
async fn extract_staged(
    archive: &Path,
    staging_dir: &Path,
    spec: &ComponentSpec,
) -> Result<PathBuf, String> {
    let archive = archive.to_path_buf();
    let staging_dir = staging_dir.to_path_buf();
    let archive_root = spec.archive_root.clone();
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&staging_dir)
            .map_err(|e| format!("failed to create staging dir: {e}"))?;
        let file = std::fs::File::open(&archive)
            .map_err(|e| format!("failed to open {}: {e}", archive.display()))?;
        let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file))
            .map_err(|e| format!("failed to read archive: {e}"))?;
        zip.extract(&staging_dir)
            .map_err(|e| format!("failed to extract archive: {e}"))?;
        match archive_root {
            Some(root) => {
                let payload = staging_dir.join(root);
                payload
                    .is_dir()
                    .then_some(payload)
                    .ok_or_else(|| "archive layout unexpected: payload folder missing".to_string())
            }
            None => Ok(staging_dir),
        }
    })
    .await
    .map_err(|e| format!("extraction task failed: {e}"))?
}

/// Smoke-test the staged runtime before it becomes `current`: a truncated or
/// architecture-mismatched download must never be promoted.
async fn verify_staged_runtime(staged: &Path, spec: &ComponentSpec) -> Result<(), String> {
    let (binary, expected) = match spec.kind {
        ComponentKind::Node => (staged.join("node.exe"), format!("v{}", spec.version)),
        ComponentKind::Git => (
            staged.join("cmd").join("git.exe"),
            GIT_VERSION_OUTPUT.to_string(),
        ),
    };
    if spec.kind == ComponentKind::Node && !staged.join("npm.cmd").is_file() {
        return Err("npm.cmd missing from extracted runtime".to_string());
    }

    let output = crate::process::tokio_command(&binary)
        .arg("--version")
        .output()
        .await
        .map_err(|e| format!("failed to run {}: {e}", binary.display()))?;
    let actual = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() || actual != expected {
        return Err(format!(
            "runtime verification failed: expected {expected:?}, got {actual:?}"
        ));
    }
    Ok(())
}

/// Move the verified payload into its final slot and atomically publish
/// `current.json` in the shape `process::managed_node` / `managed_git` read.
fn finalize_component(
    staged: &Path,
    runtime_root: &Path,
    spec: &ComponentSpec,
) -> Result<PathBuf, String> {
    let component_root = runtime_root.join(spec.kind.name());
    let target = component_root.join(spec.version).join(spec.platform);
    std::fs::create_dir_all(target.parent().expect("target has a parent"))
        .map_err(|e| format!("failed to create {}: {e}", component_root.display()))?;
    if target.exists() {
        std::fs::remove_dir_all(&target)
            .map_err(|e| format!("failed to replace {}: {e}", target.display()))?;
    }
    std::fs::rename(staged, &target)
        .map_err(|e| format!("failed to move runtime into place: {e}"))?;

    let state = serde_json::json!({
        "version": spec.version,
        "platform": spec.platform,
        "path": target,
        "installedAt": chrono::Utc::now().to_rfc3339(),
    });
    let state_path = component_root.join("current.json");
    let state_tmp = component_root.join("current.json.tmp");
    std::fs::write(&state_tmp, format!("{state}\n"))
        .map_err(|e| format!("failed to write runtime state: {e}"))?;
    std::fs::rename(&state_tmp, &state_path)
        .map_err(|e| format!("failed to publish runtime state: {e}"))?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_pin_expected_assets_and_mirror_urls() {
        let (node, git) = match std::env::consts::ARCH {
            "x86_64" | "aarch64" | "x86" => specs_for_current_arch().expect("specs"),
            _ => return,
        };

        assert!(node.asset.starts_with("node-v"));
        assert!(node.mirror_url.starts_with(NODE_MIRROR_BASE));
        assert!(node.mirror_url.ends_with(&node.asset));
        assert!(node.official_url.starts_with(NODE_OFFICIAL_BASE));
        assert_eq!(
            node.archive_root.as_deref(),
            Some(node.asset.trim_end_matches(".zip"))
        );

        assert!(git.asset.starts_with("MinGit-"));
        assert!(git.mirror_url.starts_with(GIT_MIRROR_BASE));
        assert!(git.mirror_url.contains(GIT_RELEASE_TAG));
        assert!(git.official_url.starts_with(GIT_OFFICIAL_BASE));
        assert!(git.archive_root.is_none());
    }

    #[test]
    fn finalize_publishes_layout_and_state_the_runtime_readers_accept() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_root = temp.path().join("runtime");
        let staged = temp.path().join("staged");
        std::fs::create_dir_all(&staged).expect("staged dir");
        std::fs::write(staged.join("node.exe"), b"node").expect("node.exe");
        std::fs::write(staged.join("npm.cmd"), b"npm").expect("npm.cmd");

        let (node, _git) = match specs_for_current_arch() {
            Some(specs) => specs,
            None => return,
        };
        let target = finalize_component(&staged, &runtime_root, &node).expect("finalize");

        assert_eq!(
            target,
            runtime_root
                .join("node")
                .join(node.version)
                .join(node.platform)
        );
        assert!(target.join("node.exe").is_file());
        let state: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(runtime_root.join("node/current.json")).expect("state"),
        )
        .expect("json");
        assert_eq!(state["version"], node.version);
        assert_eq!(state["platform"], node.platform);
    }

    #[test]
    fn finalize_replaces_an_existing_runtime_slot() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_root = temp.path().join("runtime");
        let (node, _git) = match specs_for_current_arch() {
            Some(specs) => specs,
            None => return,
        };

        let old_target = runtime_root
            .join("node")
            .join(node.version)
            .join(node.platform);
        std::fs::create_dir_all(&old_target).expect("old target");
        std::fs::write(old_target.join("stale.txt"), b"stale").expect("stale marker");

        let staged = temp.path().join("staged");
        std::fs::create_dir_all(&staged).expect("staged dir");
        std::fs::write(staged.join("node.exe"), b"node").expect("node.exe");

        let target = finalize_component(&staged, &runtime_root, &node).expect("finalize");
        assert!(!target.join("stale.txt").exists());
        assert!(target.join("node.exe").is_file());
    }
}
