//! Built-in expert skills management.
//!
//! Experts are curated skills (from obra/superpowers) that iyw-claw bundles
//! into its binary via `include_dir!`. On startup they are extracted to a
//! central directory `~/.iyw-claw/skills/<id>/`. Users can then enable an
//! expert for any ACP agent by creating a symbolic link (or Windows
//! junction) from the agent's skill directory into the central copy.
//!
//! The central store is the single source of truth. Enabling/disabling is
//! purely "does a link exist in the agent's skill dir" — there is no
//! database state, and updates propagate automatically when iyw-claw upgrades
//! and re-extracts the bundled files.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use chrono::Utc;
use include_dir::{include_dir, Dir, DirEntry};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::acp::types::AgentSkillScope;
use crate::commands::acp::{
    preferred_scope_skill_dir, remove_skill_entry, scoped_skill_dirs, validate_skill_id,
};
use crate::models::agent::AgentType;

// ─── Embedded bundle ────────────────────────────────────────────────────

static WRITING_PLANS_BUNDLE: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/experts/skills/writing-plans");
static EXECUTING_PLANS_BUNDLE: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/experts/skills/executing-plans");
static USING_SUPERPOWERS_BUNDLE: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/experts/skills/using-superpowers");
static WRITING_SKILLS_BUNDLE: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/experts/skills/writing-skills");
static IMAGEGEN_BUNDLE: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/experts/skills/imagegen");
static PLUGIN_CREATOR_BUNDLE: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/experts/skills/plugin-creator");
static SKILL_CREATOR_BUNDLE: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/experts/skills/skill-creator");
static SKILL_INSTALLER_BUNDLE: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/experts/skills/skill-installer");
static IYW_IMAGE_WORKFLOWS_BUNDLE: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/experts/skills/iyw-image-workflows");
static LIXIAO_WORKFLOWS_BUNDLE: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/experts/skills/lixiao-workflows");
static SELF_IMPROVING_BUNDLE: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/experts/skills/self-improving");
static OPEN_COMPUTER_USE_BUNDLE: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/experts/skills/open-computer-use");
static EXPERTS_TOML_CONTENT: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/experts/experts.toml"));

const CENTRAL_DIR_NAME: &str = ".iyw-claw";
const CENTRAL_SKILLS_SUBDIR: &str = "skills";
const MANIFEST_FILE: &str = ".manifest.json";
const EXPERTS_TOML: &str = "experts.toml";
const MANAGED_COPY_MARKER_FILE: &str = ".iyw-claw-managed-copy.json";
const MANAGED_COPY_MARKER_VERSION: u8 = 1;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagedCopyMarker {
    version: u8,
    expected_target: PathBuf,
}

// ─── Error type ─────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum ExpertsError {
    #[error("expert not found: {0}")]
    NotFound(String),
    #[error("agent does not support skills: {0:?}")]
    UnsupportedAgent(AgentType),
    #[error("a real directory already exists at '{path}' — delete or rename it first")]
    NameCollision { path: String },
    #[error("a different link already exists at '{path}' (points to '{found}') — remove it first")]
    ForeignLink { path: String, found: String },
    #[error("io error: {0}")]
    Io(String),
    #[error("metadata error: {0}")]
    Metadata(String),
    #[error("central expert store is unavailable: {0}")]
    CentralUnavailable(String),
    #[error(
        "Agent storage is not initialized. Choose a private storage directory in Agent Settings."
    )]
    AgentStorageNotInitialized,
}

impl Serialize for ExpertsError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<io::Error> for ExpertsError {
    fn from(err: io::Error) -> Self {
        ExpertsError::Io(err.to_string())
    }
}

// ─── Public types ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ExpertMetadata {
    pub id: String,
    pub category: String,
    pub icon: Option<String>,
    pub sort_order: i32,
    pub display_name: BTreeMap<String, String>,
    pub description: BTreeMap<String, String>,
    pub bundled_hash: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExpertListItem {
    pub metadata: ExpertMetadata,
    pub installed_centrally: bool,
    pub user_modified: bool,
    pub central_path: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExpertLinkState {
    NotLinked,
    LinkedToIywClaw,
    LinkedElsewhere,
    BlockedByRealDirectory,
    Broken,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpertInstallStatus {
    pub expert_id: String,
    pub agent_type: AgentType,
    pub state: ExpertLinkState,
    pub link_path: String,
    pub target_path: Option<String>,
    pub expected_target_path: String,
    pub copy_mode: bool,
}

/// A single enable/disable request for one (skill, agent) pair, used by the
/// batch `*_apply_links` commands. `expert_id` is the central-store id — for
/// office tools it carries the office skill id (mirroring how
/// `ExpertInstallStatus.expert_id` already doubles as the office skill id).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkOp {
    pub expert_id: String,
    pub agent_type: AgentType,
    pub enable: bool,
}

/// Per-op outcome of a batch apply. A failed op never aborts the rest of the
/// batch; the caller inspects `ok`/`error` per entry and re-fetches the
/// authoritative snapshot afterwards.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkOpResult {
    pub expert_id: String,
    pub agent_type: AgentType,
    pub ok: bool,
    /// Present on a successful enable; `None` for disables and failures.
    pub status: Option<ExpertInstallStatus>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct InstallReport {
    pub installed_count: usize,
    pub updated_count: usize,
    pub pending_user_review: Vec<String>,
    pub errors: Vec<String>,
}

// ─── Manifest ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Manifest {
    #[serde(default)]
    iyw_claw_version: String,
    #[serde(default)]
    installed_at: String,
    #[serde(default)]
    experts: BTreeMap<String, ManifestEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ManifestEntry {
    #[serde(default)]
    hash: String,
    #[serde(default)]
    installed_at: String,
    #[serde(default)]
    pending_user_review: bool,
}

// ─── Concurrency ────────────────────────────────────────────────────────

fn mutation_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

// ─── Paths ──────────────────────────────────────────────────────────────

fn home_dir_or_default() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

pub(crate) fn central_experts_dir() -> PathBuf {
    home_dir_or_default()
        .join(CENTRAL_DIR_NAME)
        .join(CENTRAL_SKILLS_SUBDIR)
}

fn manifest_path() -> PathBuf {
    central_experts_dir().join(MANIFEST_FILE)
}

fn expert_central_path(expert_id: &str) -> PathBuf {
    central_experts_dir().join(expert_id)
}

fn agent_link_path(agent: AgentType, expert_id: &str) -> Result<PathBuf, ExpertsError> {
    let dir = preferred_scope_skill_dir(agent, AgentSkillScope::Global, None)
        .map_err(|_| ExpertsError::UnsupportedAgent(agent))?;
    Ok(dir.join(expert_id))
}

// ─── Metadata loading ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ExpertsTomlRoot {
    #[serde(default)]
    expert: Vec<ExpertTomlEntry>,
}

#[derive(Debug, Deserialize)]
struct ExpertTomlEntry {
    id: String,
    category: String,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    sort_order: i32,
    #[serde(default)]
    display_name: BTreeMap<String, String>,
    #[serde(default)]
    description: BTreeMap<String, String>,
}

fn bundled_metadata() -> &'static [ExpertMetadata] {
    static METADATA: OnceLock<Vec<ExpertMetadata>> = OnceLock::new();
    METADATA.get_or_init(|| match load_bundled_metadata_inner() {
        Ok(list) => list,
        Err(err) => {
            tracing::error!("[Experts] failed to load bundled metadata: {err}");
            Vec::new()
        }
    })
}

fn load_bundled_metadata_inner() -> Result<Vec<ExpertMetadata>, ExpertsError> {
    let root: ExpertsTomlRoot = toml::from_str(EXPERTS_TOML_CONTENT)
        .map_err(|e| ExpertsError::Metadata(format!("failed to parse {EXPERTS_TOML}: {e}")))?;

    let mut out = Vec::with_capacity(root.expert.len());
    for entry in root.expert {
        let bundled_hash = hash_bundled_expert(&entry.id)?;
        out.push(ExpertMetadata {
            id: entry.id,
            category: entry.category,
            icon: entry.icon,
            sort_order: entry.sort_order,
            display_name: entry.display_name,
            description: entry.description,
            bundled_hash,
        });
    }
    out.sort_by(|a, b| {
        a.sort_order
            .cmp(&b.sort_order)
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(out)
}

fn find_metadata(expert_id: &str) -> Result<&'static ExpertMetadata, ExpertsError> {
    bundled_metadata()
        .iter()
        .find(|m| m.id == expert_id)
        .ok_or_else(|| ExpertsError::NotFound(expert_id.to_string()))
}

fn require_private_agent_storage_for_write() -> Result<(), ExpertsError> {
    let paths = crate::acp::agent_storage::AgentStoragePaths::active()
        .ok_or(ExpertsError::AgentStorageNotInitialized)?;
    crate::acp::agent_storage::startup_profile_env_is_complete(&paths, |key| std::env::var_os(key))
        .then_some(())
        .ok_or(ExpertsError::AgentStorageNotInitialized)
}

pub(crate) fn is_bundled_expert_id(expert_id: &str) -> bool {
    bundled_metadata().iter().any(|m| m.id == expert_id)
}

// ─── Hashing ────────────────────────────────────────────────────────────

fn hash_bundled_expert(expert_id: &str) -> Result<String, ExpertsError> {
    let dir = bundled_skill_dir(expert_id)
        .ok_or_else(|| ExpertsError::NotFound(expert_id.to_string()))?;
    let mut files: Vec<(&str, &[u8])> = Vec::new();
    collect_bundle_files(dir, &mut files);
    files.sort_by_key(|(path, _)| *path);
    let mut hasher = Sha256::new();
    for (path, contents) in files {
        hasher.update(format!("skills/{expert_id}/{path}").as_bytes());
        hasher.update(b"\0");
        hasher.update(contents);
        hasher.update(b"\0");
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn bundled_skill_dir(expert_id: &str) -> Option<&'static Dir<'static>> {
    match expert_id {
        "writing-plans" => Some(&WRITING_PLANS_BUNDLE),
        "executing-plans" => Some(&EXECUTING_PLANS_BUNDLE),
        "using-superpowers" => Some(&USING_SUPERPOWERS_BUNDLE),
        "writing-skills" => Some(&WRITING_SKILLS_BUNDLE),
        "imagegen" => Some(&IMAGEGEN_BUNDLE),
        "plugin-creator" => Some(&PLUGIN_CREATOR_BUNDLE),
        "skill-creator" => Some(&SKILL_CREATOR_BUNDLE),
        "skill-installer" => Some(&SKILL_INSTALLER_BUNDLE),
        "iyw-image-workflows" => Some(&IYW_IMAGE_WORKFLOWS_BUNDLE),
        "lixiao-workflows" => Some(&LIXIAO_WORKFLOWS_BUNDLE),
        "self-improving" => Some(&SELF_IMPROVING_BUNDLE),
        "open-computer-use" => Some(&OPEN_COMPUTER_USE_BUNDLE),
        _ => None,
    }
}

fn collect_bundle_files<'a>(dir: &'a Dir<'a>, out: &mut Vec<(&'a str, &'a [u8])>) {
    for entry in dir.entries() {
        match entry {
            DirEntry::File(f) => {
                let rel = f.path().to_str().unwrap_or("");
                out.push((rel, f.contents()));
            }
            DirEntry::Dir(d) => collect_bundle_files(d, out),
        }
    }
}

fn hash_disk_directory(path: &Path) -> Result<String, ExpertsError> {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    collect_disk_files(path, path, &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = Sha256::new();
    for (rel_path, contents) in files {
        // Mirror the bundled hash format: relative path includes the
        // leading `skills/<id>/` prefix from bundled view.
        let logical = format!(
            "skills/{}/{}",
            path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default(),
            rel_path
        );
        hasher.update(logical.as_bytes());
        hasher.update(b"\0");
        hasher.update(&contents);
        hasher.update(b"\0");
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_disk_files(
    base: &Path,
    current: &Path,
    out: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), ExpertsError> {
    if !current.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let child = entry.path();
        if file_type.is_dir() {
            collect_disk_files(base, &child, out)?;
        } else if file_type.is_file() {
            let rel = child
                .strip_prefix(base)
                .map_err(|e| ExpertsError::Io(e.to_string()))?
                .to_string_lossy()
                .replace('\\', "/");
            let contents = fs::read(&child)?;
            out.push((rel, contents));
        }
    }
    Ok(())
}

// ─── Manifest I/O ───────────────────────────────────────────────────────

fn load_manifest() -> Manifest {
    let path = manifest_path();
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str::<Manifest>(&content).unwrap_or_default(),
        Err(_) => Manifest::default(),
    }
}

fn save_manifest(manifest: &Manifest) -> Result<(), ExpertsError> {
    let path = manifest_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let serialized = serde_json::to_string_pretty(manifest)
        .map_err(|e| ExpertsError::Metadata(format!("failed to serialize manifest: {e}")))?;
    fs::write(&path, serialized)?;
    Ok(())
}

// ─── Link operations ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ManagedLinkChange {
    Unchanged,
    Linked { copy_mode: bool },
    Removed,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ManagedLinkEntryError {
    #[error("a real directory already exists at the managed link path")]
    NameCollision,
    #[error("a different link already exists (points to '{found}')")]
    ForeignLink { found: String },
    #[error("io error: {0}")]
    Io(String),
}

fn foreign_link_error(link_path: &Path) -> ManagedLinkEntryError {
    ManagedLinkEntryError::ForeignLink {
        found: read_link_target(link_path)
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|| "<unknown>".into()),
    }
}

fn enable_managed_link_entry(
    expected_target: &Path,
    link_path: &Path,
) -> Result<ManagedLinkChange, ManagedLinkEntryError> {
    if managed_copy_is_owned(expected_target, link_path) {
        remove_skill_entry(link_path)
            .map_err(|error| ManagedLinkEntryError::Io(error.to_string()))?;
        return create_link_raw(expected_target, link_path)
            .map(|copy_mode| ManagedLinkChange::Linked { copy_mode })
            .map_err(|error| ManagedLinkEntryError::Io(error.to_string()));
    }
    match classify_link(link_path, expected_target) {
        ExpertLinkState::LinkedToIywClaw => return Ok(ManagedLinkChange::Unchanged),
        ExpertLinkState::BlockedByRealDirectory => {
            return Err(ManagedLinkEntryError::NameCollision);
        }
        ExpertLinkState::LinkedElsewhere | ExpertLinkState::Broken => {
            return Err(foreign_link_error(link_path));
        }
        ExpertLinkState::NotLinked => {}
    }
    if let Some(parent) = link_path.parent() {
        fs::create_dir_all(parent).map_err(|error| ManagedLinkEntryError::Io(error.to_string()))?;
    }
    create_link_raw(expected_target, link_path)
        .map(|copy_mode| ManagedLinkChange::Linked { copy_mode })
        .map_err(|error| ManagedLinkEntryError::Io(error.to_string()))
}

fn raw_link_targets(link_path: &Path, expected_target: &Path) -> bool {
    let Some(target) = read_link_target(link_path) else {
        return false;
    };
    let target = if target.is_absolute() {
        target
    } else {
        link_path.parent().unwrap_or(Path::new("")).join(target)
    };
    paths_equivalent(&target, expected_target)
}

pub(crate) fn managed_link_is_owned(expected_target: &Path, link_path: &Path) -> bool {
    let state = classify_link(link_path, expected_target);
    state == ExpertLinkState::LinkedToIywClaw
        || (state == ExpertLinkState::Broken && raw_link_targets(link_path, expected_target))
}

pub(crate) fn managed_copy_is_owned(expected_target: &Path, copy_path: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(copy_path) else {
        return false;
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() || path_is_reparse_point(copy_path) {
        return false;
    }
    let marker_path = copy_path.join(MANAGED_COPY_MARKER_FILE);
    let Ok(marker_metadata) = fs::symlink_metadata(&marker_path) else {
        return false;
    };
    if !marker_metadata.file_type().is_file() {
        return false;
    }
    let Ok(bytes) = fs::read(marker_path) else {
        return false;
    };
    let Ok(marker) = serde_json::from_slice::<ManagedCopyMarker>(&bytes) else {
        return false;
    };
    marker.version == MANAGED_COPY_MARKER_VERSION
        && paths_equivalent(&marker.expected_target, expected_target)
}

#[cfg(any(windows, test))]
fn write_managed_copy_marker(copy_path: &Path, expected_target: &Path) -> io::Result<()> {
    let marker = ManagedCopyMarker {
        version: MANAGED_COPY_MARKER_VERSION,
        expected_target: expected_target.to_path_buf(),
    };
    let bytes = serde_json::to_vec(&marker).map_err(io::Error::other)?;
    fs::write(copy_path.join(MANAGED_COPY_MARKER_FILE), bytes)
}

pub(crate) fn reconcile_managed_link_entry(
    expected_target: &Path,
    link_path: &Path,
    enable: bool,
) -> Result<ManagedLinkChange, ManagedLinkEntryError> {
    if enable {
        return enable_managed_link_entry(expected_target, link_path);
    }
    if !managed_link_is_owned(expected_target, link_path) {
        return Ok(ManagedLinkChange::Unchanged);
    }
    remove_skill_entry(link_path).map_err(|error| ManagedLinkEntryError::Io(error.to_string()))?;
    Ok(ManagedLinkChange::Removed)
}

pub(crate) type ManagedLinkPathChange = (PathBuf, ManagedLinkChange);
pub(crate) type ManagedLinkPathError = (PathBuf, ManagedLinkEntryError);

pub(crate) fn reconcile_managed_link_paths(
    expected_target: &Path,
    preferred_link_path: &Path,
    all_link_paths: &[PathBuf],
    enable: bool,
) -> Result<Vec<ManagedLinkPathChange>, ManagedLinkPathError> {
    if enable {
        let link_path = all_link_paths
            .iter()
            .find(|path| managed_link_is_owned(expected_target, path))
            .map(PathBuf::as_path)
            .unwrap_or(preferred_link_path);
        let change = reconcile_managed_link_entry(expected_target, link_path, true)
            .map_err(|error| (link_path.to_path_buf(), error))?;
        return Ok(match change {
            ManagedLinkChange::Unchanged => Vec::new(),
            change => vec![(link_path.to_path_buf(), change)],
        });
    }

    let mut seen = BTreeSet::new();
    let mut changes = Vec::new();
    let mut first_error = None;
    for link_path in
        std::iter::once(preferred_link_path.to_path_buf()).chain(all_link_paths.iter().cloned())
    {
        if !seen.insert(link_path.clone()) {
            continue;
        }
        match reconcile_managed_link_entry(expected_target, &link_path, false) {
            Ok(ManagedLinkChange::Unchanged) => {}
            Ok(change) => changes.push((link_path, change)),
            Err(error) if first_error.is_none() => first_error = Some((link_path, error)),
            Err(_) => {}
        }
    }
    match first_error {
        Some(error) => Err(error),
        None => Ok(changes),
    }
}

fn experts_error_from_managed(error: ManagedLinkEntryError, link_path: &Path) -> ExpertsError {
    let path = link_path.to_string_lossy().to_string();
    match error {
        ManagedLinkEntryError::NameCollision => ExpertsError::NameCollision { path },
        ManagedLinkEntryError::ForeignLink { found } => ExpertsError::ForeignLink { path, found },
        ManagedLinkEntryError::Io(message) => ExpertsError::Io(message),
    }
}

#[cfg(unix)]
pub(crate) fn create_link_raw(src: &Path, dst: &Path) -> io::Result<bool> {
    std::os::unix::fs::symlink(src, dst).map(|_| false)
}

#[cfg(windows)]
pub(crate) fn create_link_raw(src: &Path, dst: &Path) -> io::Result<bool> {
    match junction::create(src, dst) {
        Ok(_) => Ok(false),
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => Err(err),
        Err(junction_err) => {
            let copy_result =
                copy_dir_recursive(src, dst).and_then(|_| write_managed_copy_marker(dst, src));
            copy_result.map_err(|copy_err| {
                let _ = fs::remove_dir_all(dst);
                io::Error::other(format!(
                    "junction failed ({junction_err}); copy fallback failed ({copy_err})"
                ))
            })?;
            Ok(true)
        }
    }
}

#[cfg(windows)]
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

/// Best-effort human-readable link target. On Windows, `fs::read_link`
/// does not resolve junctions in all stdlib versions — prefer the
/// `junction` crate when the path is a reparse point.
pub(crate) fn read_link_target(path: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        if path_is_reparse_point(path) {
            if let Ok(target) = junction::get_target(path) {
                return Some(target);
            }
        }
    }
    fs::read_link(path).ok()
}

pub(crate) fn path_is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

/// On Windows a junction is *not* a symlink — it is a directory reparse
/// point. `symlink_metadata` reports it as a directory. So we also need to
/// ask the OS whether the directory is a reparse point.
#[cfg(windows)]
fn path_is_reparse_point(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    fs::symlink_metadata(path)
        .map(|m| m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        .unwrap_or(false)
}

#[cfg(not(windows))]
fn path_is_reparse_point(_path: &Path) -> bool {
    false
}

/// Equality check for two already-canonicalized paths. On Windows the
/// filesystem is case-insensitive but `Path` comparison is not — canonical
/// forms can still differ in drive-letter case or user-supplied casing.
fn paths_equivalent(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    #[cfg(windows)]
    {
        let a_s = a.as_os_str().to_string_lossy();
        let b_s = b.as_os_str().to_string_lossy();
        a_s.eq_ignore_ascii_case(b_s.as_ref())
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Resolve a path while following symlinks and Windows junctions.
/// Returns `None` if the path does not exist or cannot be resolved (e.g.
/// dangling link).
fn resolve_real_path(path: &Path) -> Option<PathBuf> {
    fs::canonicalize(path).ok()
}

pub(crate) fn classify_link(link_path: &Path, expected_target: &Path) -> ExpertLinkState {
    // No entry at all (not even a dangling link) → not linked.
    let meta = match fs::symlink_metadata(link_path) {
        Ok(m) => m,
        Err(_) => return ExpertLinkState::NotLinked,
    };

    let is_link_like = meta.file_type().is_symlink() || path_is_reparse_point(link_path);
    if !is_link_like {
        if managed_copy_is_owned(expected_target, link_path) {
            return ExpertLinkState::LinkedToIywClaw;
        }
        // A user-owned real directory (or file) sits where we'd put our link.
        return ExpertLinkState::BlockedByRealDirectory;
    }

    // `fs::canonicalize` transparently follows both symlinks and Windows
    // junctions, so comparing the two canonical forms is the single
    // source of truth for "does this link point at our central store?".
    // We intentionally do *not* rely on `fs::read_link`'s string output
    // for equality — on Windows junctions its output format is
    // stdlib-version-dependent and often fails to round-trip through
    // `canonicalize` cleanly.
    let resolved_link = resolve_real_path(link_path);
    let resolved_expected = resolve_real_path(expected_target);

    match (resolved_link, resolved_expected) {
        (None, _) => ExpertLinkState::Broken,
        (Some(l), Some(e)) if paths_equivalent(&l, &e) => ExpertLinkState::LinkedToIywClaw,
        _ => ExpertLinkState::LinkedElsewhere,
    }
}

// ─── Central store installation ────────────────────────────────────────

pub async fn ensure_central_experts_installed() -> InstallReport {
    let _guard = mutation_lock().lock().await;
    tokio::task::spawn_blocking(ensure_central_experts_installed_blocking)
        .await
        .unwrap_or_else(|e| {
            let mut r = InstallReport::default();
            r.errors.push(format!("join error: {e}"));
            r
        })
}

fn ensure_central_experts_installed_blocking() -> InstallReport {
    let mut report = InstallReport::default();

    let central = central_experts_dir();
    if let Err(e) = fs::create_dir_all(&central) {
        report
            .errors
            .push(format!("failed to create central dir: {e}"));
        return report;
    }

    let mut manifest = load_manifest();
    let meta_list = bundled_metadata();

    for meta in meta_list {
        match install_or_refresh_expert(meta, &mut manifest) {
            Ok(InstallAction::Skipped) => {}
            Ok(InstallAction::Installed) => {
                report.installed_count += 1;
            }
            Ok(InstallAction::Updated) => {
                report.updated_count += 1;
            }
            Ok(InstallAction::BackedUp) => {
                report.updated_count += 1;
                report.pending_user_review.push(meta.id.clone());
            }
            Err(e) => {
                report.errors.push(format!("{}: {}", meta.id, e));
            }
        }
    }

    manifest.iyw_claw_version = env!("CARGO_PKG_VERSION").to_string();
    manifest.installed_at = Utc::now().to_rfc3339();
    if let Err(e) = save_manifest(&manifest) {
        report.errors.push(format!("save manifest: {e}"));
    }

    report
}

enum InstallAction {
    Skipped,
    Installed,
    Updated,
    BackedUp,
}

fn install_or_refresh_expert(
    meta: &ExpertMetadata,
    manifest: &mut Manifest,
) -> Result<InstallAction, ExpertsError> {
    let central_path = expert_central_path(&meta.id);
    let bundled_hash = &meta.bundled_hash;
    let manifest_entry = manifest.experts.get(&meta.id).cloned().unwrap_or_default();

    if central_path.exists() {
        let on_disk_hash = hash_disk_directory(&central_path).unwrap_or_default();
        if &on_disk_hash == bundled_hash {
            // Up-to-date and pristine. Ensure manifest matches.
            if manifest_entry.hash != *bundled_hash {
                manifest.experts.insert(
                    meta.id.clone(),
                    ManifestEntry {
                        hash: bundled_hash.clone(),
                        installed_at: Utc::now().to_rfc3339(),
                        pending_user_review: false,
                    },
                );
            }
            return Ok(InstallAction::Skipped);
        }

        // Content differs. Was the user the one who changed it, or is
        // the bundle itself newer?
        let user_modified = manifest_entry.hash.is_empty() || on_disk_hash != manifest_entry.hash;
        if user_modified {
            // Preserve user work: move aside, install fresh.
            let backup_name = format!(
                "{}.user-backup-{}",
                meta.id,
                Utc::now().format("%Y%m%d-%H%M%S")
            );
            let backup_path = central_experts_dir().join(backup_name);
            fs::rename(&central_path, &backup_path)?;
            extract_expert_to_disk(meta, &central_path)?;
            manifest.experts.insert(
                meta.id.clone(),
                ManifestEntry {
                    hash: bundled_hash.clone(),
                    installed_at: Utc::now().to_rfc3339(),
                    pending_user_review: true,
                },
            );
            return Ok(InstallAction::BackedUp);
        }

        // Pristine but outdated → overwrite.
        remove_skill_entry(&central_path)
            .map_err(|e| ExpertsError::Io(format!("remove stale expert: {e}")))?;
        extract_expert_to_disk(meta, &central_path)?;
        manifest.experts.insert(
            meta.id.clone(),
            ManifestEntry {
                hash: bundled_hash.clone(),
                installed_at: Utc::now().to_rfc3339(),
                pending_user_review: false,
            },
        );
        Ok(InstallAction::Updated)
    } else {
        extract_expert_to_disk(meta, &central_path)?;
        manifest.experts.insert(
            meta.id.clone(),
            ManifestEntry {
                hash: bundled_hash.clone(),
                installed_at: Utc::now().to_rfc3339(),
                pending_user_review: false,
            },
        );
        Ok(InstallAction::Installed)
    }
}

fn extract_expert_to_disk(meta: &ExpertMetadata, target: &Path) -> Result<(), ExpertsError> {
    let dir = bundled_skill_dir(&meta.id).ok_or_else(|| ExpertsError::NotFound(meta.id.clone()))?;
    fs::create_dir_all(target)?;
    extract_bundle_dir(dir, "", target)?;
    Ok(())
}

fn extract_bundle_dir(
    dir: &Dir<'_>,
    bundle_prefix: &str,
    target: &Path,
) -> Result<(), ExpertsError> {
    for entry in dir.entries() {
        match entry {
            DirEntry::File(f) => {
                let rel = f
                    .path()
                    .to_str()
                    .ok_or_else(|| ExpertsError::Io("non-utf8 path in bundle".into()))?;
                let rel_within = rel
                    .strip_prefix(bundle_prefix)
                    .and_then(|s| s.strip_prefix('/'))
                    .unwrap_or(rel);
                let out_path = target.join(rel_within);
                if let Some(parent) = out_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&out_path, f.contents())?;
                // `include_dir!` does not carry Unix permission bits. Restore
                // the execute bit for bundled scripts that declare a shebang.
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if f.contents().starts_with(b"#!") {
                        let mut perms = fs::metadata(&out_path)?.permissions();
                        perms.set_mode(perms.mode() | 0o111);
                        fs::set_permissions(&out_path, perms)?;
                    }
                }
            }
            DirEntry::Dir(d) => {
                extract_bundle_dir(d, bundle_prefix, target)?;
            }
        }
    }
    Ok(())
}

// ─── Commands: list / status ────────────────────────────────────────────

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn experts_list() -> Result<Vec<ExpertListItem>, ExpertsError> {
    let meta_list = bundled_metadata().to_vec();
    let manifest = load_manifest();
    let mut out = Vec::with_capacity(meta_list.len());
    for meta in meta_list {
        let central_path = expert_central_path(&meta.id);
        let installed_centrally = central_path.exists();
        let user_modified = manifest
            .experts
            .get(&meta.id)
            .map(|e| e.pending_user_review)
            .unwrap_or(false);
        out.push(ExpertListItem {
            metadata: meta,
            installed_centrally,
            user_modified,
            central_path: central_path.to_string_lossy().to_string(),
        });
    }
    Ok(out)
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn experts_get_install_status(
    expert_id: String,
) -> Result<Vec<ExpertInstallStatus>, ExpertsError> {
    let expert_id =
        validate_skill_id(&expert_id).map_err(|e| ExpertsError::Metadata(e.to_string()))?;
    let _ = find_metadata(&expert_id)?; // ensure it exists in the bundle
    let expected = expert_central_path(&expert_id);
    let agents = supported_agents();

    let mut out = Vec::with_capacity(agents.len());
    for agent in agents {
        let link_path = match agent_link_path(agent, &expert_id) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let state = classify_link(&link_path, &expected);
        let target_path = read_link_target(&link_path).map(|p| p.to_string_lossy().to_string());
        out.push(ExpertInstallStatus {
            expert_id: expert_id.clone(),
            agent_type: agent,
            state,
            link_path: link_path.to_string_lossy().to_string(),
            target_path,
            expected_target_path: expected.to_string_lossy().to_string(),
            copy_mode: managed_copy_is_owned(&expected, &link_path),
        });
    }
    Ok(out)
}

fn supported_agents() -> Vec<AgentType> {
    crate::commands::managed_skills::supported_skill_agent_types()
}

/// Bundled skills are split into managed families by `experts.toml` category.
/// Internal skills are hidden and always published, while `computer_use` is
/// the one bundled family users can opt into. Storage is shared by all groups.
pub(crate) const CODEX_NATIVE_CATEGORY: &str = "codex_native";
pub(crate) const COMPUTER_USE_CATEGORY: &str = "computer_use";

fn is_codex_native(metadata: &ExpertMetadata) -> bool {
    metadata.category == CODEX_NATIVE_CATEGORY
}

fn is_computer_use(metadata: &ExpertMetadata) -> bool {
    metadata.category == COMPUTER_USE_CATEGORY
}

pub(crate) fn managed_expert_ids() -> Vec<String> {
    bundled_metadata()
        .iter()
        .filter(|metadata| !is_codex_native(metadata) && !is_computer_use(metadata))
        .map(|metadata| metadata.id.clone())
        .collect()
}

pub(crate) fn managed_ready_expert_ids() -> Vec<String> {
    bundled_metadata()
        .iter()
        .filter(|metadata| !is_codex_native(metadata) && !is_computer_use(metadata))
        .filter(|metadata| expert_central_path(&metadata.id).exists())
        .map(|metadata| metadata.id.clone())
        .collect()
}

pub(crate) fn managed_codex_native_ids() -> Vec<String> {
    bundled_metadata()
        .iter()
        .filter(|metadata| is_codex_native(metadata))
        .map(|metadata| metadata.id.clone())
        .collect()
}

pub(crate) fn managed_ready_codex_native_ids() -> Vec<String> {
    bundled_metadata()
        .iter()
        .filter(|metadata| is_codex_native(metadata))
        .filter(|metadata| expert_central_path(&metadata.id).exists())
        .map(|metadata| metadata.id.clone())
        .collect()
}

pub(crate) fn managed_computer_use_ids() -> Vec<String> {
    bundled_metadata()
        .iter()
        .filter(|metadata| is_computer_use(metadata))
        .map(|metadata| metadata.id.clone())
        .collect()
}

pub(crate) fn managed_ready_computer_use_ids() -> Vec<String> {
    bundled_metadata()
        .iter()
        .filter(|metadata| is_computer_use(metadata))
        .filter(|metadata| expert_central_path(&metadata.id).exists())
        .map(|metadata| metadata.id.clone())
        .collect()
}

pub(crate) fn managed_expert_has_owned_link(expert_id: &str, agents: &[AgentType]) -> bool {
    let expected = expert_central_path(expert_id);
    agents.iter().any(|agent_type| {
        scoped_skill_dirs(*agent_type, AgentSkillScope::Global, None).is_ok_and(|dirs| {
            dirs.into_iter()
                .any(|dir| managed_link_is_owned(&expected, &dir.join(expert_id)))
        })
    })
}

// ─── Commands: link / unlink ────────────────────────────────────────────

/// Link one expert into one agent's skill dir. **Assumes the mutation lock is
/// already held** by the caller — `tokio::sync::Mutex` is not reentrant, so the
/// batch path (`experts_apply_links`) locks once and calls this directly rather
/// than the public command (which would self-deadlock).
fn link_one_locked(
    expert_id: &str,
    agent_type: AgentType,
) -> Result<ExpertInstallStatus, ExpertsError> {
    let expert_id =
        validate_skill_id(expert_id).map_err(|e| ExpertsError::Metadata(e.to_string()))?;
    let _ = find_metadata(&expert_id)?;
    let central = expert_central_path(&expert_id);
    if !central.exists() {
        return Err(ExpertsError::CentralUnavailable(format!(
            "expert '{expert_id}' is not installed in central store"
        )));
    }

    require_private_agent_storage_for_write()?;
    let link_path = agent_link_path(agent_type, &expert_id)?;
    let change = reconcile_managed_link_entry(&central, &link_path, true)
        .map_err(|error| experts_error_from_managed(error, &link_path))?;
    let copy_mode = matches!(change, ManagedLinkChange::Linked { copy_mode: true });

    let state = classify_link(&link_path, &central);
    let target_path = read_link_target(&link_path).map(|p| p.to_string_lossy().to_string());
    Ok(ExpertInstallStatus {
        expert_id: expert_id.clone(),
        agent_type,
        state,
        link_path: link_path.to_string_lossy().to_string(),
        target_path,
        expected_target_path: central.to_string_lossy().to_string(),
        copy_mode,
    })
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn experts_link_to_agent(
    expert_id: String,
    agent_type: AgentType,
) -> Result<ExpertInstallStatus, ExpertsError> {
    let _guard = mutation_lock().lock().await;
    link_one_locked(&expert_id, agent_type)
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn experts_unlink_from_agent(
    expert_id: String,
    agent_type: AgentType,
) -> Result<(), ExpertsError> {
    let _guard = mutation_lock().lock().await;
    unlink_one_locked(&expert_id, agent_type)
}

/// Remove one expert's link from one agent's skill dirs. **Assumes the mutation
/// lock is already held** (see `link_one_locked`).
fn unlink_one_locked(expert_id: &str, agent_type: AgentType) -> Result<(), ExpertsError> {
    let expert_id =
        validate_skill_id(expert_id).map_err(|e| ExpertsError::Metadata(e.to_string()))?;

    // Scan ALL global dirs for this agent to handle shared-dir agents
    // (Codex, Gemini and Cline all also point at `~/.agents/skills/`).
    // Remove the link wherever it is found.
    let dirs = scoped_skill_dirs(agent_type, AgentSkillScope::Global, None)
        .map_err(|_| ExpertsError::UnsupportedAgent(agent_type))?;

    let central = expert_central_path(&expert_id);
    let mut removed = false;
    for dir in dirs {
        let candidate = dir.join(&expert_id);
        if !candidate.exists() && !path_is_symlink(&candidate) {
            continue;
        }
        let state = classify_link(&candidate, &central);
        if managed_link_is_owned(&central, &candidate) {
            require_private_agent_storage_for_write()?;
            remove_skill_entry(&candidate).map_err(|e| {
                ExpertsError::Io(format!("remove link {}: {e}", candidate.display()))
            })?;
            removed = true;
        } else if state == ExpertLinkState::LinkedElsewhere {
            return Err(ExpertsError::ForeignLink {
                path: candidate.to_string_lossy().to_string(),
                found: read_link_target(&candidate)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| "<unknown>".into()),
            });
        } else if state == ExpertLinkState::BlockedByRealDirectory {
            // Not ours; leave alone.
            continue;
        }
    }

    if !removed {
        // It was already unlinked — treat as idempotent success.
    }
    Ok(())
}

fn expert_status_from_link_change(
    expert_id: &str,
    agent_type: AgentType,
    link_change: (PathBuf, bool),
) -> ExpertInstallStatus {
    let (link_path, copy_mode) = link_change;
    let central = expert_central_path(expert_id);
    ExpertInstallStatus {
        expert_id: expert_id.to_string(),
        agent_type,
        state: classify_link(&link_path, &central),
        link_path: link_path.to_string_lossy().to_string(),
        target_path: read_link_target(&link_path).map(|path| path.to_string_lossy().to_string()),
        expected_target_path: central.to_string_lossy().to_string(),
        copy_mode,
    }
}

fn managed_expert_link_paths(
    expert_id: &str,
    agent_type: AgentType,
) -> Result<(PathBuf, Vec<PathBuf>), ExpertsError> {
    let preferred = agent_link_path(agent_type, expert_id)?;
    let paths = scoped_skill_dirs(agent_type, AgentSkillScope::Global, None)
        .map_err(|_| ExpertsError::UnsupportedAgent(agent_type))?
        .into_iter()
        .map(|directory| directory.join(expert_id))
        .collect();
    Ok((preferred, paths))
}

fn managed_expert_pair_result(
    expert_id: &str,
    agent_type: AgentType,
    enable: bool,
) -> Option<LinkOpResult> {
    let central = expert_central_path(expert_id);
    if enable && !central.exists() {
        return None;
    }
    let (preferred, paths) = match managed_expert_link_paths(expert_id, agent_type) {
        Ok(paths) => paths,
        Err(error) => return Some(link_failure(expert_id, agent_type, error.to_string())),
    };
    let owned = paths
        .iter()
        .find(|path| managed_link_is_owned(&central, path));
    if enable && (owned.is_none() || owned.is_some_and(|p| managed_copy_is_owned(&central, p))) {
        if let Err(error) = require_private_agent_storage_for_write() {
            return Some(link_failure(expert_id, agent_type, error.to_string()));
        }
    }
    match reconcile_managed_link_paths(&central, &preferred, &paths, enable) {
        Ok(changes) if changes.is_empty() => None,
        Ok(_) if !enable => Some(link_success(expert_id, agent_type, None)),
        Ok(changes) => changes.into_iter().find_map(|(path, change)| {
            let ManagedLinkChange::Linked { copy_mode } = change else {
                return None;
            };
            let status = expert_status_from_link_change(expert_id, agent_type, (path, copy_mode));
            Some(link_success(expert_id, agent_type, Some(status)))
        }),
        Err((path, error)) => Some(link_failure(
            expert_id,
            agent_type,
            experts_error_from_managed(error, &path).to_string(),
        )),
    }
}

fn link_success(
    expert_id: &str,
    agent_type: AgentType,
    status: Option<ExpertInstallStatus>,
) -> LinkOpResult {
    LinkOpResult {
        expert_id: expert_id.to_string(),
        agent_type,
        ok: true,
        status,
        error: None,
    }
}

fn link_failure(expert_id: &str, agent_type: AgentType, error: String) -> LinkOpResult {
    LinkOpResult {
        expert_id: expert_id.to_string(),
        agent_type,
        ok: false,
        status: None,
        error: Some(error),
    }
}

pub(crate) async fn reconcile_managed_experts(
    targets: &[(AgentType, String, bool)],
) -> Vec<LinkOpResult> {
    let _guard = mutation_lock().lock().await;
    targets
        .iter()
        .filter_map(|(agent_type, expert_id, enable)| {
            managed_expert_pair_result(expert_id, *agent_type, *enable)
        })
        .collect()
}

/// Apply a batch of enable/disable operations under a single lock acquisition.
///
/// Each op is applied independently: a failing op records `ok: false` with its
/// error and the batch continues, so a partial failure never aborts the rest.
/// The frontend computes the minimal delta of changed cells, calls this, then
/// re-fetches the authoritative snapshot via `experts_list_all_install_statuses`
/// to reconcile (necessary because shared agent dirs make per-op state
/// non-local — see the office/experts shared-dir note).
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn experts_apply_links(ops: Vec<LinkOp>) -> Result<Vec<LinkOpResult>, ExpertsError> {
    let _guard = mutation_lock().lock().await;
    let mut out = Vec::with_capacity(ops.len());
    for op in ops {
        let LinkOp {
            expert_id,
            agent_type,
            enable,
        } = op;
        let res = if enable {
            link_one_locked(&expert_id, agent_type).map(Some)
        } else {
            unlink_one_locked(&expert_id, agent_type).map(|()| None)
        };
        out.push(match res {
            Ok(status) => LinkOpResult {
                expert_id,
                agent_type,
                ok: true,
                status,
                error: None,
            },
            Err(err) => LinkOpResult {
                expert_id,
                agent_type,
                ok: false,
                status: None,
                error: Some(err.to_string()),
            },
        });
    }
    Ok(out)
}

/// One-shot snapshot of every (expert, agent) link state — lets the matrix UI
/// render the whole grid from a single round-trip instead of one
/// `experts_get_install_status` call per expert.
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn experts_list_all_install_statuses() -> Result<Vec<ExpertInstallStatus>, ExpertsError> {
    let agents = supported_agents();
    let mut out = Vec::with_capacity(bundled_metadata().len() * agents.len());
    for meta in bundled_metadata() {
        let expected = expert_central_path(&meta.id);
        for &agent in &agents {
            let link_path = match agent_link_path(agent, &meta.id) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let state = classify_link(&link_path, &expected);
            let target_path = read_link_target(&link_path).map(|p| p.to_string_lossy().to_string());
            out.push(ExpertInstallStatus {
                expert_id: meta.id.clone(),
                agent_type: agent,
                state,
                link_path: link_path.to_string_lossy().to_string(),
                target_path,
                expected_target_path: expected.to_string_lossy().to_string(),
                copy_mode: managed_copy_is_owned(&expected, &link_path),
            });
        }
    }
    Ok(out)
}

// ─── Commands: read / open ──────────────────────────────────────────────

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn experts_read_content(expert_id: String) -> Result<String, ExpertsError> {
    let expert_id =
        validate_skill_id(&expert_id).map_err(|e| ExpertsError::Metadata(e.to_string()))?;
    let _ = find_metadata(&expert_id)?;
    let path = expert_central_path(&expert_id).join("SKILL.md");
    if !path.exists() {
        // Fall back to bundled copy when central store isn't populated.
        if let Some(f) = bundled_skill_dir(&expert_id).and_then(|dir| dir.get_file("SKILL.md")) {
            if let Some(text) = f.contents_utf8() {
                return Ok(text.to_string());
            }
        }
        return Err(ExpertsError::CentralUnavailable(format!(
            "expert '{expert_id}' has no SKILL.md on disk"
        )));
    }
    let content = fs::read_to_string(&path)?;
    Ok(content)
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn experts_open_central_dir() -> Result<String, ExpertsError> {
    let dir = central_experts_dir();
    fs::create_dir_all(&dir)?;
    Ok(dir.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::acp::skill_storage_spec;
    use tokio::time::{timeout, Duration};

    fn create_test_dir_link(source: &Path, link: &Path) {
        #[cfg(unix)]
        std::os::unix::fs::symlink(source, link).expect("create test symlink");
        #[cfg(windows)]
        junction::create(source, link).expect("create test junction");
    }

    #[test]
    fn expert_profile_writes_require_initialized_private_storage() {
        temp_env::with_var(
            crate::acp::agent_storage::STORAGE_ROOT_ENV,
            None::<&str>,
            || {
                let error = require_private_agent_storage_for_write()
                    .expect_err("expert links must be blocked before storage initialization");
                assert!(error
                    .to_string()
                    .contains("Agent storage is not initialized"));
            },
        );
    }

    #[test]
    fn managed_expert_ids_exclude_internal_workflow_skills() {
        let ids = managed_expert_ids();
        assert!(!ids.contains(&"systematic-debugging".to_string()));
        assert!(!ids.contains(&"finishing-a-development-branch".to_string()));
    }

    // These tests deliberately use ids that are well-formed but absent from the
    // bundle and unlikely to exist as real links, so they never touch or mutate
    // the developer's real skill directories: a disable of an absent id only
    // performs path-existence reads, and an enable of an unknown id fails at
    // `find_metadata` before any filesystem write.

    #[tokio::test]
    async fn apply_links_does_not_deadlock() {
        // The keystone regression: `experts_apply_links` locks the (non-reentrant)
        // mutation lock once and must call the lock-free inner helpers, NOT the
        // public single commands. If a future refactor reintroduced a re-lock,
        // the second acquisition would hang — caught here as a timeout rather
        // than a wedged CI run.
        let ops = vec![
            LinkOp {
                expert_id: "zzz-iyw-claw-batch-test-absent-aaa".into(),
                agent_type: AgentType::ClaudeCode,
                enable: false,
            },
            LinkOp {
                expert_id: "zzz-iyw-claw-batch-test-absent-bbb".into(),
                agent_type: AgentType::Codex,
                enable: false,
            },
        ];
        let results = timeout(Duration::from_secs(5), experts_apply_links(ops))
            .await
            .expect("experts_apply_links must not deadlock")
            .expect("batch returns Ok");
        assert_eq!(results.len(), 2);
        // Disabling an already-absent link is an idempotent success.
        assert!(results.iter().all(|r| r.ok), "{results:?}");
    }

    #[tokio::test]
    async fn apply_links_collects_per_op_results_without_aborting() {
        let ops = vec![
            LinkOp {
                expert_id: "zzz-iyw-claw-batch-test-absent".into(),
                agent_type: AgentType::ClaudeCode,
                enable: false,
            },
            LinkOp {
                // Unknown expert → enable fails at find_metadata, before any fs write.
                expert_id: "zzz-unknown-expert".into(),
                agent_type: AgentType::ClaudeCode,
                enable: true,
            },
        ];
        let results = experts_apply_links(ops).await.expect("batch returns Ok");
        assert_eq!(results.len(), 2);
        assert!(results[0].ok, "idempotent disable should succeed");
        assert!(!results[1].ok, "unknown expert enable should fail its op");
        assert!(results[1].error.is_some());
        assert!(results[1].status.is_none());
    }

    #[tokio::test]
    async fn list_all_install_statuses_covers_every_expert_agent_pair() {
        let rows = experts_list_all_install_statuses()
            .await
            .expect("snapshot returns Ok");
        let expected = bundled_metadata().len() * supported_agents().len();
        assert_eq!(rows.len(), expected);
    }

    #[test]
    fn supported_agents_follow_registry_skill_capabilities() {
        let expected = crate::acp::registry::all_acp_agents()
            .into_iter()
            .filter(|agent| skill_storage_spec(*agent).is_some())
            .collect::<Vec<_>>();
        assert_eq!(supported_agents(), expected);
    }

    #[test]
    fn managed_ready_expert_ids_match_installed_central_entries() {
        let ready = |codex_native: bool| {
            bundled_metadata()
                .iter()
                .filter(|metadata| is_codex_native(metadata) == codex_native)
                .filter(|metadata| expert_central_path(&metadata.id).exists())
                .map(|metadata| metadata.id.clone())
                .collect::<Vec<_>>()
        };

        assert_eq!(managed_ready_expert_ids(), ready(false));
        assert_eq!(managed_ready_codex_native_ids(), ready(true));
    }

    #[test]
    fn managed_disable_removes_owned_preferred_link() {
        let temp = tempfile::tempdir().expect("tempdir");
        let central = temp.path().join("central");
        let link = temp.path().join("agent").join("skill");
        fs::create_dir_all(&central).expect("create central skill");
        fs::create_dir_all(link.parent().unwrap()).expect("create agent skill root");
        create_test_dir_link(&central, &link);

        let change =
            reconcile_managed_link_entry(&central, &link, false).expect("disable managed link");

        assert_eq!(change, ManagedLinkChange::Removed);
        assert!(fs::symlink_metadata(&link).is_err());
        assert!(central.is_dir());
    }

    #[test]
    fn managed_disable_removes_owned_copy_fallback() {
        let temp = tempfile::tempdir().expect("tempdir");
        let central = temp.path().join("central");
        let copied = temp.path().join("copied-skill");
        fs::create_dir_all(&central).expect("create central skill");
        fs::create_dir_all(&copied).expect("create copied skill");
        fs::write(copied.join("stale.txt"), b"stale").expect("seed copied skill");
        fs::write(
            copied.join(".iyw-claw-managed-copy.json"),
            serde_json::to_vec(&serde_json::json!({
                "version": 1,
                "expectedTarget": central,
            }))
            .expect("serialize marker"),
        )
        .expect("write managed copy marker");

        assert!(managed_link_is_owned(&central, &copied));
        assert_eq!(
            reconcile_managed_link_entry(&central, &copied, false).unwrap(),
            ManagedLinkChange::Removed
        );
        assert!(!copied.exists());
        assert!(central.is_dir());
    }

    #[test]
    fn managed_enable_refreshes_owned_copy_fallback() {
        let temp = tempfile::tempdir().expect("tempdir");
        let central = temp.path().join("central");
        let copied = temp.path().join("copied-skill");
        fs::create_dir_all(&central).expect("create central skill");
        fs::write(central.join("fresh.txt"), b"fresh").expect("seed central skill");
        fs::create_dir_all(&copied).expect("create copied skill");
        fs::write(copied.join("stale.txt"), b"stale").expect("seed stale copy");
        write_managed_copy_marker(&copied, &central).expect("write managed copy marker");

        let change =
            reconcile_managed_link_entry(&central, &copied, true).expect("refresh managed copy");

        assert!(matches!(change, ManagedLinkChange::Linked { .. }));
        assert_eq!(fs::read(copied.join("fresh.txt")).unwrap(), b"fresh");
        assert!(!copied.join("stale.txt").exists());
        assert!(managed_link_is_owned(&central, &copied));
    }

    #[test]
    fn managed_disable_removes_owned_links_from_all_agent_directories() {
        let temp = tempfile::tempdir().expect("tempdir");
        let central = temp.path().join("central");
        let preferred = temp.path().join("preferred-skill");
        let secondary = temp.path().join("secondary-skill");
        fs::create_dir_all(&central).expect("create central skill");
        create_test_dir_link(&central, &preferred);
        create_test_dir_link(&central, &secondary);

        let changes = reconcile_managed_link_paths(
            &central,
            &preferred,
            &[preferred.clone(), secondary.clone()],
            false,
        )
        .expect("disable all owned publications");

        assert_eq!(changes.len(), 2);
        assert!(!preferred.exists());
        assert!(!secondary.exists());
        assert!(central.is_dir());
    }

    #[test]
    fn managed_enable_recognizes_an_owned_secondary_directory() {
        let temp = tempfile::tempdir().expect("tempdir");
        let central = temp.path().join("central");
        let preferred = temp.path().join("preferred-skill");
        let secondary = temp.path().join("secondary-skill");
        fs::create_dir_all(&central).expect("create central skill");
        fs::create_dir_all(&preferred).expect("seed user-owned preferred directory");
        fs::write(preferred.join("keep.txt"), b"keep").expect("seed user content");
        create_test_dir_link(&central, &secondary);

        let changes = reconcile_managed_link_paths(
            &central,
            &preferred,
            &[preferred.clone(), secondary.clone()],
            true,
        )
        .expect("recognize secondary publication");

        assert!(changes.is_empty());
        assert_eq!(fs::read(preferred.join("keep.txt")).unwrap(), b"keep");
        assert!(managed_link_is_owned(&central, &secondary));
    }

    #[test]
    fn managed_disable_preserves_real_directory_and_foreign_link() {
        let temp = tempfile::tempdir().expect("tempdir");
        let central = temp.path().join("central");
        let real = temp.path().join("real-skill");
        let foreign = temp.path().join("foreign");
        let foreign_link = temp.path().join("foreign-link");
        fs::create_dir_all(&central).expect("create central skill");
        fs::create_dir_all(&real).expect("create real skill");
        fs::write(real.join("keep.txt"), b"keep").expect("seed real skill");
        fs::create_dir_all(&foreign).expect("create foreign skill");
        create_test_dir_link(&foreign, &foreign_link);

        assert_eq!(
            reconcile_managed_link_entry(&central, &real, false).unwrap(),
            ManagedLinkChange::Unchanged
        );
        assert_eq!(
            reconcile_managed_link_entry(&central, &foreign_link, false).unwrap(),
            ManagedLinkChange::Unchanged
        );
        assert_eq!(fs::read(real.join("keep.txt")).unwrap(), b"keep");
        assert_eq!(
            classify_link(&foreign_link, &central),
            ExpertLinkState::LinkedElsewhere
        );
    }

    #[test]
    fn migration_ownership_rejects_real_directory_and_foreign_link() {
        let temp = tempfile::tempdir().expect("tempdir");
        let central = temp.path().join("central");
        let owned_link = temp.path().join("owned-link");
        let foreign = temp.path().join("foreign");
        let foreign_link = temp.path().join("foreign-link");
        let real = temp.path().join("real-skill");
        fs::create_dir_all(&central).expect("create central skill");
        fs::create_dir_all(&foreign).expect("create foreign skill");
        fs::create_dir_all(&real).expect("create real skill");
        create_test_dir_link(&central, &owned_link);
        create_test_dir_link(&foreign, &foreign_link);

        assert!(managed_link_is_owned(&central, &owned_link));
        assert!(!managed_link_is_owned(&central, &foreign_link));
        assert!(!managed_link_is_owned(&central, &real));
    }

    #[test]
    fn managed_enable_rejects_real_directory_and_foreign_link() {
        let temp = tempfile::tempdir().expect("tempdir");
        let central = temp.path().join("central");
        let real = temp.path().join("real-skill");
        let foreign = temp.path().join("foreign");
        let foreign_link = temp.path().join("foreign-link");
        fs::create_dir_all(&central).expect("create central skill");
        fs::create_dir_all(&real).expect("create real skill");
        fs::create_dir_all(&foreign).expect("create foreign skill");
        create_test_dir_link(&foreign, &foreign_link);

        assert!(matches!(
            reconcile_managed_link_entry(&central, &real, true),
            Err(ManagedLinkEntryError::NameCollision)
        ));
        assert!(matches!(
            reconcile_managed_link_entry(&central, &foreign_link, true),
            Err(ManagedLinkEntryError::ForeignLink { .. })
        ));
    }
}
