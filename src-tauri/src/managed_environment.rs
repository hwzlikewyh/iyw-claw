use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    catalog_revision: u64,
    components: Vec<ManagedComponent>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagedComponent {
    component_id: String,
    component_kind: String,
    version: String,
    relative_path: String,
    entrypoints: std::collections::BTreeMap<String, String>,
    files: Vec<ManagedFile>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagedFile {
    path: String,
    size: u64,
    sha256: String,
    link_target: Option<String>,
}

pub fn entrypoint(component: &str, name: &str) -> Option<PathBuf> {
    let root = crate::paths::iyw_claw_user_dir();
    let raw = std::fs::read(root.join("inventory/environment-current.json")).ok()?;
    let snapshot = serde_json::from_slice::<Snapshot>(&raw).ok()?;
    let component = snapshot
        .components
        .into_iter()
        .find(|item| item.component_id == component)?;
    let relative = component.entrypoints.get(name)?;
    let component_root = managed_path(&root, &component.relative_path)?;
    let candidate = managed_path(&component_root, relative)?;
    let record = component.files.iter().find(|file| file.path == *relative)?;
    verify_file(&component_root, &candidate, record).then_some(candidate)
}

pub fn component_root(component: &str) -> Option<PathBuf> {
    let root = crate::paths::iyw_claw_user_dir();
    let raw = std::fs::read(root.join("inventory/environment-current.json")).ok()?;
    let snapshot = serde_json::from_slice::<Snapshot>(&raw).ok()?;
    let component = snapshot
        .components
        .into_iter()
        .find(|item| item.component_id == component)?;
    let path = managed_path(&root, &component.relative_path)?;
    path.is_dir().then_some(path)
}

pub fn tool_entrypoint(name: &str) -> Option<PathBuf> {
    match name {
        "node" | "npm" | "npx" => entrypoint("node", name),
        "git" => entrypoint("git", "git"),
        "uv" | "uvx" => entrypoint("uv", name),
        "officecli" => entrypoint("officecli", "officecli"),
        "agent-reach" => entrypoint("agent-reach", "agent-reach"),
        "open-computer-use" => entrypoint("open-computer-use", "open-computer-use"),
        _ => None,
    }
}

pub async fn repair() -> Result<(), String> {
    let helper = environment_helper()
        .ok_or_else(|| "安装目录缺少 iyw-environment 环境修复程序，请重新安装应用".to_string())?;
    let mut command = crate::process::tokio_command(helper);
    command
        .args([
            "repair",
            "--app-version",
            env!("CARGO_PKG_VERSION"),
            "--json",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(30 * 60), command.output())
        .await
        .map_err(|_| "环境修复超时，请检查网络后重试".to_string())?
        .map_err(|error| format!("无法启动环境修复程序：{error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let detail = bounded_tail(&String::from_utf8_lossy(&output.stderr), 2_000);
    Err(format!("环境修复失败（{}）：{detail}", output.status))
}

fn environment_helper() -> Option<PathBuf> {
    let file = format!("iyw-environment{}", std::env::consts::EXE_SUFFIX);
    let mut candidates = std::env::var_os("IYW_CLAW_ENVIRONMENT_HELPER_PATH")
        .map(PathBuf::from)
        .into_iter()
        .collect::<Vec<_>>();
    if let Ok(executable) = std::env::current_exe() {
        if let Some(directory) = executable.parent() {
            candidates.push(directory.join(&file));
        }
    }
    if cfg!(debug_assertions) {
        candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "binaries/iyw-environment-{}{}",
            env!("IYW_CLAW_TARGET_TRIPLE"),
            std::env::consts::EXE_SUFFIX
        )));
    }
    candidates.into_iter().find(|path| path.is_file())
}

fn bounded_tail(value: &str, limit: usize) -> String {
    let count = value.chars().count();
    if count <= limit {
        return value.trim().to_string();
    }
    value.chars().skip(count - limit).collect::<String>()
}

pub fn init_status_report() -> crate::acp::version_center::InitStatusReport {
    let root = crate::paths::iyw_claw_user_dir();
    let path = root.join("inventory/environment-current.json");
    let raw = std::fs::read(&path).unwrap_or_default();
    let snapshot = serde_json::from_slice::<Snapshot>(&raw).ok();
    let components = snapshot
        .as_ref()
        .map(|value| {
            value
                .components
                .iter()
                .map(|component| component_status(&root, component))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let ready = ["node", "git", "uv", "chromix", "agent-browser"]
        .iter()
        .all(|id| {
            components
                .iter()
                .any(|item| item.component_id == *id && item.active)
        });
    crate::acp::version_center::InitStatusReport {
        phase: if ready { "ready" } else { "degraded" }.to_string(),
        components,
        offline: false,
        writer_busy: false,
        pending_activations: Vec::new(),
        manifest_generation: snapshot.map_or(0, |value| value.catalog_revision),
        digest: format!("{:x}", Sha256::digest(&raw)),
        migrated: false,
    }
}

fn component_status(
    root: &Path,
    component: &ManagedComponent,
) -> crate::acp::version_center::ComponentStatusView {
    let active = !component.files.is_empty()
        && required_entrypoints(&component.component_id)
            .iter()
            .all(|name| component_entrypoint(root, component, name).is_some());
    crate::acp::version_center::ComponentStatusView {
        component_id: component.component_id.clone(),
        component_kind: component.component_kind.clone(),
        version: component.version.clone(),
        installed: active,
        active,
        phase: if active { "ready" } else { "degraded" }.to_string(),
        last_error: (!active).then(|| "Managed environment requires repair".to_string()),
    }
}

fn required_entrypoints(component: &str) -> &'static [&'static str] {
    match component {
        "node" => &["node", "npm", "npx"],
        "git" => &["git"],
        "uv" => &["uv", "uvx"],
        "chromix" => &["chromix"],
        "agent-browser" => &["agent-browser"],
        "officecli" => &["officecli"],
        "agent-reach" => &["agent-reach"],
        "open-computer-use" => &["open-computer-use"],
        _ => &[],
    }
}

fn component_entrypoint(root: &Path, component: &ManagedComponent, name: &str) -> Option<PathBuf> {
    let relative = component.entrypoints.get(name)?;
    let component_root = managed_path(root, &component.relative_path)?;
    let candidate = managed_path(&component_root, relative)?;
    let record = component.files.iter().find(|file| file.path == *relative)?;
    verify_file(&component_root, &candidate, record).then_some(candidate)
}

fn managed_path(root: &Path, relative: &str) -> Option<PathBuf> {
    if relative.is_empty()
        || relative
            .split('/')
            .any(|part| matches!(part, "" | "." | ".."))
    {
        return None;
    }
    let path = relative
        .split('/')
        .fold(root.to_path_buf(), |path, part| path.join(part));
    path.components()
        .all(|part| {
            matches!(
                part,
                Component::Prefix(_) | Component::RootDir | Component::Normal(_)
            )
        })
        .then_some(path)
}

fn verify_file(root: &Path, path: &Path, record: &ManagedFile) -> bool {
    if let Some(target) = record.link_target.as_deref() {
        return verify_link(root, path, target);
    }
    verify_regular_file(root, path, record)
}

fn verify_link(root: &Path, path: &Path, expected: &str) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    let Ok(target) = std::fs::read_link(path) else {
        return false;
    };
    metadata.file_type().is_symlink()
        && target.to_string_lossy().replace('\\', "/") == expected
        && canonical_file_is_within(root, path)
}

fn verify_regular_file(root: &Path, path: &Path, record: &ManagedFile) -> bool {
    let Ok(canonical_root) = std::fs::canonicalize(root) else {
        return false;
    };
    let Ok(canonical_path) = std::fs::canonicalize(path) else {
        return false;
    };
    if !canonical_path.starts_with(canonical_root) {
        return false;
    }
    let Ok(metadata) = canonical_path.metadata() else {
        return false;
    };
    metadata.is_file()
        && metadata.len() == record.size
        && hash_file(&canonical_path).is_some_and(|hash| hash == record.sha256)
}

fn canonical_file_is_within(root: &Path, path: &Path) -> bool {
    let Ok(canonical_root) = std::fs::canonicalize(root) else {
        return false;
    };
    let Ok(canonical_path) = std::fs::canonicalize(path) else {
        return false;
    };
    canonical_path.starts_with(canonical_root) && canonical_path.is_file()
}

fn hash_file(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Some(format!("{:x}", hasher.finalize()))
}
