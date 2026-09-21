use std::collections::BTreeMap;
use std::{fs, io::Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use walkdir::WalkDir;

use crate::download::hash_file;
use crate::model::{
    EnvironmentSnapshot, FileRecord, InstalledComponent, InventoryEntry, PreparedState,
};
use crate::paths::{from_slash, slash_relative, Layout};

pub fn load_current(layout: &Layout) -> Result<Option<EnvironmentSnapshot>> {
    let snapshot = read_json_optional(&layout.current_snapshot())?;
    if let Some(snapshot) = &snapshot {
        crate::validation::validate_snapshot(snapshot)?;
    }
    Ok(snapshot)
}

pub fn current_pc_version() -> Option<String> {
    let layout = Layout::resolve().ok()?;
    load_current(&layout)
        .ok()
        .flatten()
        .map(|value| value.pc_version)
}

pub fn snapshot_digest(layout: &Layout) -> Result<String> {
    if !layout.current_snapshot().exists() {
        return Ok(String::new());
    }
    hash_file(&layout.current_snapshot())
}

pub fn healthy_inventory(
    layout: &Layout,
    snapshot: Option<&EnvironmentSnapshot>,
) -> Vec<InventoryEntry> {
    snapshot
        .into_iter()
        .flat_map(|value| &value.components)
        .map(|component| inventory_entry(layout, component))
        .collect()
}

pub fn verify_component(layout: &Layout, component: &InstalledComponent) -> Result<()> {
    let root = from_slash(&layout.root, &component.relative_path)?;
    if !root.is_dir() || component.files.is_empty() {
        bail!("component directory is missing")
    }
    validate_entrypoints(&component.component_id, &component.entrypoints)?;
    let canonical_root = root.canonicalize()?;
    if !canonical_root.starts_with(layout.runtime.canonicalize()?) {
        bail!("component escaped the managed runtime")
    }
    for record in &component.files {
        let path = from_slash(&root, &record.path)?;
        if !path.canonicalize()?.starts_with(&canonical_root) {
            bail!("component file escaped the managed root")
        }
        if let Some(expected) = &record.link_target {
            let metadata = fs::symlink_metadata(&path).context("read component link")?;
            let actual = fs::read_link(&path).context("read component link target")?;
            if !metadata.file_type().is_symlink()
                || actual.to_string_lossy().replace('\\', "/") != *expected
            {
                bail!("installed component link failed verification")
            }
            continue;
        }
        let metadata = fs::metadata(&path).context("read installed component file")?;
        if !metadata.is_file()
            || metadata.len() != record.size
            || hash_file(&path)? != record.sha256
        {
            bail!("installed component file failed verification")
        }
    }
    for relative in component.entrypoints.values() {
        let path = from_slash(&root, relative)?;
        if !path.is_file()
            || !component
                .files
                .iter()
                .any(|record| record.path == *relative)
        {
            bail!("installed component entrypoint is missing or unverified")
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if path.metadata()?.permissions().mode() & 0o111 == 0 {
                bail!("installed component entrypoint is not executable")
            }
        }
    }
    Ok(())
}

fn inventory_entry(layout: &Layout, component: &InstalledComponent) -> InventoryEntry {
    InventoryEntry {
        component_key: component.component_id.clone(),
        component_kind: component.component_kind.clone(),
        current_version: component.version.clone(),
        sha256: component.artifact_sha256.clone(),
        active: true,
        pinned: false,
        healthy: verify_component(layout, component).is_ok(),
        lkg: true,
    }
}

pub fn describe_component(
    layout: &Layout,
    component_id: &str,
    root: &Path,
) -> Result<(Vec<FileRecord>, BTreeMap<String, String>)> {
    let files = file_records(root)?;
    let entrypoints = discover_entrypoints(component_id, root)?;
    validate_entrypoints(component_id, &entrypoints)?;
    for entrypoint in entrypoints.values() {
        let path = from_slash(root, entrypoint)?;
        if !path.is_file() {
            bail!("component entrypoint is missing")
        }
    }
    let _ = slash_relative(&layout.root, root)?;
    Ok((files, entrypoints))
}

fn file_records(root: &Path) -> Result<Vec<FileRecord>> {
    let mut records = Vec::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry.context("walk staged component")?;
        if entry.file_type().is_symlink() {
            records.push(FileRecord {
                path: slash_relative(root, entry.path())?,
                size: 0,
                sha256: String::new(),
                link_target: Some(
                    fs::read_link(entry.path())?
                        .to_string_lossy()
                        .replace('\\', "/"),
                ),
            });
            continue;
        }
        if !entry.path().is_file() {
            continue;
        }
        let path = entry.path();
        records.push(FileRecord {
            path: slash_relative(root, path)?,
            size: entry.metadata()?.len(),
            sha256: hash_file(path)?,
            link_target: None,
        });
    }
    records.sort_by(|left, right| left.path.cmp(&right.path));
    if records.is_empty() {
        bail!("staged component contains no files")
    }
    Ok(records)
}

fn discover_entrypoints(component: &str, root: &Path) -> Result<BTreeMap<String, String>> {
    let mut values = BTreeMap::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry?;
        if !entry.path().is_file() {
            continue;
        }
        let relative = slash_relative(root, entry.path())?;
        let lower = relative.to_ascii_lowercase();
        if let Some(name) = entrypoint_name(component, &lower) {
            values.entry(name.to_string()).or_insert(relative);
        }
    }
    Ok(values)
}

fn entrypoint_name(component: &str, lower: &str) -> Option<&'static str> {
    let file = lower.rsplit('/').next().unwrap_or(lower);
    match component {
        "node" if matches!(file, "node" | "node.exe") => Some("node"),
        "node" if file == if cfg!(windows) { "npm.cmd" } else { "npm" } => Some("npm"),
        "node" if file == if cfg!(windows) { "npx.cmd" } else { "npx" } => Some("npx"),
        "git" if lower.ends_with("cmd/git.exe") || lower.ends_with("bin/git") => Some("git"),
        "uv" if matches!(file, "uv" | "uv.exe") => Some("uv"),
        "uv" if matches!(file, "uvx" | "uvx.exe") => Some("uvx"),
        "chromix" if matches!(file, "chrome" | "chrome.exe" | "chromium") => Some("chromix"),
        "agent-browser" if file.contains("agent-browser") => Some("agent-browser"),
        "officecli" if file.contains("officecli") => Some("officecli"),
        "environment-maintainer" if matches!(file,
            "iyw-environment" | "iyw-environment.exe" | "environment-maintainer" | "environment-maintainer.exe") => {
            Some("environment-maintainer")
        }
        "agent-reach"
            if lower.ends_with("bin/agent-reach") || lower.ends_with("bin/agent-reach.cmd") =>
        {
            Some("agent-reach")
        }
        "open-computer-use" if is_open_computer_use_entrypoint(lower) => Some("open-computer-use"),
        _ => None,
    }
}

fn is_open_computer_use_entrypoint(path: &str) -> bool {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "aarch64") => path.ends_with("dist/windows/arm64/open-computer-use.exe"),
        ("windows", _) => path.ends_with("dist/windows/amd64/open-computer-use.exe"),
        ("linux", "aarch64") => path.ends_with("dist/linux/arm64/open-computer-use"),
        ("linux", _) => path.ends_with("dist/linux/amd64/open-computer-use"),
        ("macos", _) => path.ends_with("open computer use.app/contents/macos/opencomputeruse"),
        _ => false,
    }
}

fn validate_entrypoints(component: &str, values: &BTreeMap<String, String>) -> Result<()> {
    let required: &[&str] = match component {
        "node" => &["node", "npm", "npx"],
        "git" => &["git"],
        "uv" => &["uv", "uvx"],
        "chromix" => &["chromix"],
        "agent-browser" => &["agent-browser"],
        "officecli" => &["officecli"],
        "agent-reach" => &["agent-reach"],
        "open-computer-use" => &["open-computer-use"],
        "environment-maintainer" => &["environment-maintainer"],
        _ => bail!("unknown environment component"),
    };
    if required.iter().all(|name| values.contains_key(*name)) {
        return Ok(());
    }
    bail!("component {component} is missing a required entrypoint")
}

pub fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let parent = path.parent().context("JSON path has no parent")?;
    fs::create_dir_all(parent)?;
    let temporary = temporary_path(path);
    let mut file = fs::File::create(&temporary)?;
    file.write_all(&serde_json::to_vec_pretty(value)?)?;
    file.sync_all()?;
    drop(file);
    replace_file(&temporary, path)?;
    Ok(())
}

pub fn read_prepared(layout: &Layout, transaction: &str) -> Result<PreparedState> {
    let path = layout.transaction_dir(transaction)?.join("prepared.json");
    let bytes = fs::read(path).context("read prepared environment transaction")?;
    serde_json::from_slice(&bytes).context("decode prepared environment transaction")
}

fn read_json_optional<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .context("decode environment inventory"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).context("read environment inventory"),
    }
}

fn temporary_path(path: &Path) -> PathBuf {
    path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4().simple()))
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> Result<()> {
    fs::rename(source, destination).context("publish environment inventory")
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let flags = MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH;
    if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), flags) } == 0 {
        return Err(std::io::Error::last_os_error()).context("publish environment inventory");
    }
    Ok(())
}
