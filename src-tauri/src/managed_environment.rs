use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

mod repair;
mod snapshot;
mod status;

pub use repair::repair;
pub use status::{init_status_report, ManagedEnvironmentStatusReport};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    schema_version: u8,
    generation: String,
    pc_version: String,
    target: String,
    arch: String,
    catalog_revision: u64,
    #[serde(default)]
    selected_components: Option<Vec<String>>,
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
    let snapshot = snapshot::load(&root).ok()?;
    let component = snapshot
        .components
        .into_iter()
        .find(|item| item.component_id == component)?;
    let relative = component.entrypoints.get(name)?;
    let component_root = snapshot::component_path(&root, &component.relative_path)?;
    let candidate = managed_path(&component_root, relative)?;
    let record = component.files.iter().find(|file| file.path == *relative)?;
    verify_file(&component_root, &candidate, record).then_some(candidate)
}

pub fn component_root(component: &str) -> Option<PathBuf> {
    let root = crate::paths::iyw_claw_user_dir();
    let snapshot = snapshot::load(&root).ok()?;
    let component = snapshot
        .components
        .into_iter()
        .find(|item| item.component_id == component)?;
    let path = snapshot::component_path(&root, &component.relative_path)?;
    path.is_dir().then_some(path)
}

pub fn component_version(component: &str) -> Option<String> {
    let root = crate::paths::iyw_claw_user_dir();
    snapshot::load(&root)
        .ok()?
        .components
        .into_iter()
        .find(|item| item.component_id == component)
        .map(|item| item.version)
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

fn managed_path(root: &Path, relative: &str) -> Option<PathBuf> {
    if relative.is_empty()
        || relative.split('/').any(|part| {
            matches!(part, "" | "." | "..")
                || part.contains(['\\', ':', '\0'])
                || part.ends_with([' ', '.'])
        })
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
