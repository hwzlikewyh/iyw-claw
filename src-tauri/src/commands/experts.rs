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
use std::sync::{Mutex as StdMutex, OnceLock};
#[cfg(windows)]
use std::time::Duration;
use std::time::{Instant, SystemTime};

use chrono::Utc;
use include_dir::{include_dir, Dir, DirEntry};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex as AsyncMutex;

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
static IYW_CAPABILITY_GATEWAY_BUNDLE: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/experts/skills/iyw-capability-gateway");
static WECOM_UNIFIED_BUNDLE: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/experts/skills/wecom-unified");
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
static IYW_COPYRIGHT_REGISTRATION_BUNDLE: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/experts/skills/iyw-copyright-registration");
static IYW_CRM_WORKFLOWS_BUNDLE: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/experts/skills/iyw-crm-workflows");
static IYW_SALES_ASSISTANT_WORKFLOWS_BUNDLE: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/experts/skills/iyw-sales-assistant-workflows");
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
pub(crate) const CAPABILITY_GATEWAY_EXPERT_ID: &str = "iyw-capability-gateway";
pub(crate) const RETIRED_BUNDLED_EXPERT_IDS: [&str; 1] = ["self-improving"];
/// Directories that hold installed runtime dependencies. They are expensive to
/// rebuild (network installs, native compilation), so a superseded system skill
/// directory is only discarded once none of these survive inside it.
pub(crate) const RUNTIME_ENV_DIR_NAMES: [&str; 2] = [".venv", "node_modules"];

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
        "expert '{dependency}' is required by enabled expert '{dependent}' for agent {agent:?}"
    )]
    DependencyInUse {
        dependency: String,
        dependent: String,
        agent: AgentType,
    },
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
    pub package_type: String,
    pub dependencies: Vec<String>,
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Manifest {
    #[serde(default)]
    iyw_claw_version: String,
    #[serde(default)]
    installed_at: String,
    #[serde(default)]
    experts: BTreeMap<String, ManifestEntry>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct ManifestEntry {
    #[serde(default)]
    hash: String,
    #[serde(default)]
    installed_at: String,
    #[serde(default)]
    pending_user_review: bool,
}

// ─── Concurrency ────────────────────────────────────────────────────────

fn mutation_lock() -> &'static AsyncMutex<()> {
    static LOCK: OnceLock<AsyncMutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| AsyncMutex::new(()))
}

/// A successful central-store reconcile is expensive because it hashes every
/// bundled Skill on disk. Keep a process-local, metadata-only fingerprint so
/// repeated ACP connections can prove that no relevant input changed without
/// rereading every Skill file. Failed reconciles are deliberately never cached.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CentralExpertsCache {
    fingerprint: CentralExpertsFingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CentralExpertsFingerprint {
    central_dir: PathBuf,
    bundle_revision: String,
    entries: Vec<SkillTreeEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SkillTreeEntry {
    relative_path: String,
    kind: SkillTreeEntryKind,
    len: u64,
    modified: Option<SystemTime>,
    link_target: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkillTreeEntryKind {
    Missing,
    File,
    Directory,
    Symlink,
    Other,
}

fn central_experts_cache() -> &'static StdMutex<Option<CentralExpertsCache>> {
    static CACHE: OnceLock<StdMutex<Option<CentralExpertsCache>>> = OnceLock::new();
    CACHE.get_or_init(|| StdMutex::new(None))
}

/// Call after a market override, bundled-Skill restore, or Agent configuration
/// mutation that can change the central store. The next reconcile validates the
/// full on-disk state instead of accepting the previous successful cache entry.
pub(crate) fn invalidate_central_experts_cache(reason: &str) {
    let mut cache = central_experts_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *cache = None;
    tracing::debug!(
        target: "system_skills",
        reason,
        "invalidated central Skill reconcile cache"
    );
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
    dependencies: Vec<String>,
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
    validate_expert_entries(&root.expert)?;

    let mut out = Vec::with_capacity(root.expert.len());
    for entry in root.expert {
        let bundled_hash = hash_bundled_expert(&entry.id)?;
        let package_type = package_type(&entry.dependencies).to_string();
        out.push(ExpertMetadata {
            id: entry.id,
            category: entry.category,
            package_type,
            dependencies: entry.dependencies,
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

fn active_metadata() -> Vec<ExpertMetadata> {
    bundled_metadata().to_vec()
}

fn package_type(dependencies: &[String]) -> &'static str {
    if dependencies.is_empty() {
        "skill"
    } else {
        "expert"
    }
}

fn validate_expert_entries(entries: &[ExpertTomlEntry]) -> Result<(), ExpertsError> {
    let mut graph = BTreeMap::new();
    for entry in entries {
        validate_skill_id(&entry.id).map_err(|error| ExpertsError::Metadata(error.to_string()))?;
        if graph
            .insert(entry.id.clone(), entry.dependencies.clone())
            .is_some()
        {
            return Err(ExpertsError::Metadata(format!(
                "duplicate system skill id: {}",
                entry.id
            )));
        }
    }
    for (id, dependencies) in &graph {
        let mut unique = BTreeSet::new();
        for dependency in dependencies {
            validate_skill_id(dependency)
                .map_err(|error| ExpertsError::Metadata(error.to_string()))?;
            if dependency == id {
                return Err(ExpertsError::Metadata(format!(
                    "system skill '{id}' depends on itself"
                )));
            }
            if !unique.insert(dependency) {
                return Err(ExpertsError::Metadata(format!(
                    "system skill '{id}' repeats dependency '{dependency}'"
                )));
            }
            if !graph.contains_key(dependency) {
                return Err(ExpertsError::Metadata(format!(
                    "system skill '{id}' requires missing dependency '{dependency}'"
                )));
            }
        }
    }
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for id in graph.keys() {
        visit_dependency_graph(id, &graph, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn visit_dependency_graph(
    id: &str,
    graph: &BTreeMap<String, Vec<String>>,
    visiting: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
) -> Result<(), ExpertsError> {
    if visited.contains(id) {
        return Ok(());
    }
    if !visiting.insert(id.to_string()) {
        return Err(ExpertsError::Metadata(format!(
            "system skill dependency cycle detected at '{id}'"
        )));
    }
    for dependency in graph.get(id).into_iter().flatten() {
        visit_dependency_graph(dependency, graph, visiting, visited)?;
    }
    visiting.remove(id);
    visited.insert(id.to_string());
    Ok(())
}

fn find_metadata(expert_id: &str) -> Result<ExpertMetadata, ExpertsError> {
    active_metadata()
        .into_iter()
        .find(|metadata| metadata.id == expert_id)
        .ok_or_else(|| ExpertsError::NotFound(expert_id.to_string()))
}

fn bundled_metadata_for_id(expert_id: &str) -> Result<ExpertMetadata, ExpertsError> {
    let root: ExpertsTomlRoot = toml::from_str(EXPERTS_TOML_CONTENT)
        .map_err(|e| ExpertsError::Metadata(format!("failed to parse {EXPERTS_TOML}: {e}")))?;
    validate_expert_entries(&root.expert)?;
    let entry = root
        .expert
        .into_iter()
        .find(|entry| entry.id == expert_id)
        .ok_or_else(|| ExpertsError::NotFound(expert_id.to_string()))?;
    let bundled_hash = hash_bundled_expert(&entry.id)?;
    Ok(ExpertMetadata {
        id: entry.id,
        category: entry.category,
        package_type: package_type(&entry.dependencies).to_string(),
        dependencies: entry.dependencies,
        icon: entry.icon,
        sort_order: entry.sort_order,
        display_name: entry.display_name,
        description: entry.description,
        bundled_hash,
    })
}

fn dependency_order(expert_id: &str) -> Result<Vec<String>, ExpertsError> {
    let metadata = active_metadata();
    let graph = metadata
        .into_iter()
        .map(|value| (value.id, value.dependencies))
        .collect::<BTreeMap<_, _>>();
    if !graph.contains_key(expert_id) {
        return Err(ExpertsError::NotFound(expert_id.to_string()));
    }
    let mut visited = BTreeSet::new();
    let mut ordered = Vec::new();
    collect_dependency_order(expert_id, &graph, &mut visited, &mut ordered);
    Ok(ordered)
}

fn collect_dependency_order(
    expert_id: &str,
    graph: &BTreeMap<String, Vec<String>>,
    visited: &mut BTreeSet<String>,
    ordered: &mut Vec<String>,
) {
    if !visited.insert(expert_id.to_string()) {
        return;
    }
    for dependency in graph.get(expert_id).into_iter().flatten() {
        collect_dependency_order(dependency, graph, visited, ordered);
    }
    ordered.push(expert_id.to_string());
}

fn expert_enabled_for_agent(expert_id: &str, agent_type: AgentType) -> bool {
    let central = expert_central_path(expert_id);
    scoped_skill_dirs(agent_type, AgentSkillScope::Global, None).is_ok_and(|directories| {
        directories
            .into_iter()
            .any(|directory| managed_link_is_owned(&central, &directory.join(expert_id)))
    })
}

fn ensure_dependency_not_in_use(
    expert_id: &str,
    agent_type: AgentType,
) -> Result<(), ExpertsError> {
    for metadata in active_metadata() {
        if metadata.id == expert_id || !expert_enabled_for_agent(&metadata.id, agent_type) {
            continue;
        }
        if dependency_order(&metadata.id)?
            .iter()
            .any(|dependency| dependency == expert_id)
        {
            return Err(ExpertsError::DependencyInUse {
                dependency: expert_id.to_string(),
                dependent: metadata.id,
                agent: agent_type,
            });
        }
    }
    Ok(())
}

fn require_private_agent_storage_for_write() -> Result<(), ExpertsError> {
    let paths = crate::acp::agent_storage::AgentStoragePaths::active()
        .ok_or(ExpertsError::AgentStorageNotInitialized)?;
    crate::acp::agent_storage::startup_profile_env_is_complete(&paths, |key| std::env::var_os(key))
        .then_some(())
        .ok_or(ExpertsError::AgentStorageNotInitialized)
}

pub(crate) fn is_bundled_expert_id(expert_id: &str) -> bool {
    active_metadata()
        .iter()
        .any(|metadata| metadata.id == expert_id)
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
        "iyw-capability-gateway" => Some(&IYW_CAPABILITY_GATEWAY_BUNDLE),
        "wecom-unified" => Some(&WECOM_UNIFIED_BUNDLE),
        "imagegen" => Some(&IMAGEGEN_BUNDLE),
        "plugin-creator" => Some(&PLUGIN_CREATOR_BUNDLE),
        "skill-creator" => Some(&SKILL_CREATOR_BUNDLE),
        "skill-installer" => Some(&SKILL_INSTALLER_BUNDLE),
        "iyw-image-workflows" => Some(&IYW_IMAGE_WORKFLOWS_BUNDLE),
        "lixiao-workflows" => Some(&LIXIAO_WORKFLOWS_BUNDLE),
        "iyw-copyright-registration" => Some(&IYW_COPYRIGHT_REGISTRATION_BUNDLE),
        "iyw-crm-workflows" => Some(&IYW_CRM_WORKFLOWS_BUNDLE),
        "iyw-sales-assistant-workflows" => Some(&IYW_SALES_ASSISTANT_WORKFLOWS_BUNDLE),
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
            DirEntry::Dir(d) if is_runtime_env_dir(d.path()) => {}
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
            if RUNTIME_ENV_DIR_NAMES
                .iter()
                .any(|name| child.file_name().is_some_and(|value| value == *name))
            {
                continue;
            }
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

fn bundled_revision() -> &'static str {
    static REVISION: OnceLock<String> = OnceLock::new();
    REVISION
        .get_or_init(|| {
            let mut hasher = Sha256::new();
            hasher.update(env!("CARGO_PKG_VERSION").as_bytes());
            for meta in bundled_metadata() {
                hasher.update(meta.id.as_bytes());
                hasher.update(b"\0");
                hasher.update(meta.bundled_hash.as_bytes());
                hasher.update(b"\0");
            }
            format!("{:x}", hasher.finalize())
        })
        .as_str()
}

fn current_central_experts_fingerprint() -> Result<CentralExpertsFingerprint, ExpertsError> {
    let central_dir = central_experts_dir();
    let mut entries = Vec::new();
    append_skill_tree_fingerprint(&manifest_path(), ".manifest.json", &mut entries)?;
    for meta in bundled_metadata() {
        let target = central_dir.join(&meta.id);
        append_skill_tree_fingerprint(&target, &meta.id, &mut entries)?;
    }
    Ok(CentralExpertsFingerprint {
        central_dir,
        bundle_revision: bundled_revision().to_string(),
        entries,
    })
}

fn append_skill_tree_fingerprint(
    path: &Path,
    relative_path: &str,
    entries: &mut Vec<SkillTreeEntry>,
) -> Result<(), ExpertsError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            entries.push(SkillTreeEntry {
                relative_path: relative_path.to_string(),
                kind: SkillTreeEntryKind::Missing,
                len: 0,
                modified: None,
                link_target: None,
            });
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };
    let file_type = metadata.file_type();
    let kind = if file_type.is_symlink() {
        SkillTreeEntryKind::Symlink
    } else if file_type.is_dir() {
        SkillTreeEntryKind::Directory
    } else if file_type.is_file() {
        SkillTreeEntryKind::File
    } else {
        SkillTreeEntryKind::Other
    };
    entries.push(SkillTreeEntry {
        relative_path: relative_path.to_string(),
        kind,
        len: metadata.len(),
        modified: Some(metadata.modified()?),
        link_target: file_type
            .is_symlink()
            .then(|| fs::read_link(path).ok())
            .flatten(),
    });
    if kind != SkillTreeEntryKind::Directory || is_runtime_env_dir(path) {
        return Ok(());
    }
    let mut children = fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        let name = child.file_name().to_string_lossy().replace('\\', "/");
        append_skill_tree_fingerprint(&child.path(), &format!("{relative_path}/{name}"), entries)?;
    }
    Ok(())
}

fn is_runtime_env_dir(path: &Path) -> bool {
    path.file_name().is_some_and(|name| {
        RUNTIME_ENV_DIR_NAMES
            .iter()
            .any(|runtime_name| name == *runtime_name)
    })
}

fn central_experts_cache_is_current() -> Result<bool, ExpertsError> {
    if retired_experts_need_reconcile() {
        return Ok(false);
    }
    let fingerprint = current_central_experts_fingerprint()?;
    let cache = central_experts_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    Ok(cache
        .as_ref()
        .is_some_and(|cached| cached.fingerprint == fingerprint))
}

fn cache_successful_central_reconcile() -> Result<(), ExpertsError> {
    let fingerprint = current_central_experts_fingerprint()?;
    let mut cache = central_experts_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *cache = Some(CentralExpertsCache { fingerprint });
    Ok(())
}

pub async fn ensure_central_experts_installed() -> InstallReport {
    let started_at = Instant::now();
    let _guard = mutation_lock().lock().await;
    match central_experts_cache_is_current() {
        Ok(true) => {
            tracing::info!(
                target: "system_skills",
                elapsed_ms = started_at.elapsed().as_millis(),
                cache = "hit",
                "central Skill reconcile skipped"
            );
            return InstallReport::default();
        }
        Ok(false) => tracing::debug!(
            target: "system_skills",
            cache = "miss",
            "central Skill reconcile needs validation"
        ),
        Err(error) => tracing::warn!(
            target: "system_skills",
            error = %error,
            cache = "unavailable",
            "central Skill cache fingerprint failed; reconciling from disk"
        ),
    }
    let report = tokio::task::spawn_blocking(ensure_central_experts_installed_blocking)
        .await
        .unwrap_or_else(|e| {
            let mut r = InstallReport::default();
            r.errors.push(format!("join error: {e}"));
            r
        });
    if report.errors.is_empty() {
        if let Err(error) = cache_successful_central_reconcile() {
            tracing::warn!(
                target: "system_skills",
                error = %error,
                "central Skill reconcile succeeded but its cache was not stored"
            );
        }
    } else {
        invalidate_central_experts_cache("reconcile_failed");
    }
    tracing::info!(
        target: "system_skills",
        elapsed_ms = started_at.elapsed().as_millis(),
        installed = report.installed_count,
        updated = report.updated_count,
        errors = report.errors.len(),
        "central Skill reconcile finished"
    );
    report
}

fn ensure_central_experts_installed_blocking() -> InstallReport {
    let started_at = Instant::now();
    let _shared_guard = crate::commands::acp::shared_skill_mutation_guard();
    let mut report = InstallReport::default();

    let central = central_experts_dir();
    if let Err(e) = fs::create_dir_all(&central) {
        report
            .errors
            .push(format!("failed to create central dir: {e}"));
        return report;
    }

    let mut manifest = load_manifest();
    let original_manifest = manifest.clone();
    retire_bundled_experts(&mut manifest, &mut report);
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
            Err(e) => {
                report.errors.push(format!("{}: {}", meta.id, e));
            }
        }
    }

    manifest.iyw_claw_version = env!("CARGO_PKG_VERSION").to_string();
    if manifest != original_manifest {
        manifest.installed_at = Utc::now().to_rfc3339();
        if let Err(e) = save_manifest(&manifest) {
            report.errors.push(format!("save manifest: {e}"));
        }
    }
    tracing::debug!(
        target: "system_skills",
        elapsed_ms = started_at.elapsed().as_millis(),
        installed = report.installed_count,
        updated = report.updated_count,
        manifest_changed = manifest != original_manifest,
        errors = report.errors.len(),
        "central Skill disk reconcile completed"
    );
    report
}

fn retired_experts_need_reconcile() -> bool {
    let manifest = load_manifest();
    RETIRED_BUNDLED_EXPERT_IDS.iter().any(|id| {
        let central = expert_central_path(id);
        if retired_expert_is_preserved(id, &central, &manifest) {
            return false;
        }
        if manifest.experts.contains_key(id) {
            return true;
        }
        if fs::symlink_metadata(&central).is_ok() {
            return true;
        }
        supported_agents().into_iter().any(|agent| {
            managed_expert_link_paths(id, agent)
                .map(|(_, paths)| {
                    paths
                        .iter()
                        .any(|path| managed_link_is_owned(&central, path))
                })
                .unwrap_or(false)
        })
    })
}

fn retire_bundled_experts(manifest: &mut Manifest, report: &mut InstallReport) {
    for id in RETIRED_BUNDLED_EXPERT_IDS {
        let central = expert_central_path(id);
        if retired_expert_is_preserved(id, &central, manifest) {
            report.pending_user_review.push(id.to_string());
            continue;
        }
        if let Err(error) = remove_retired_expert_links(id, &central) {
            report.errors.push(format!("{id}: {error}"));
            continue;
        }
        match fs::symlink_metadata(&central) {
            Ok(_) => match remove_skill_entry(&central) {
                Ok(()) => tracing::info!(
                    target: "system_skills",
                    skill_id = id,
                    "removed retired bundled Skill central copy"
                ),
                Err(error) => report.errors.push(format!(
                    "{id}: failed to remove retired central copy: {error}"
                )),
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => report.errors.push(format!(
                "{id}: failed to inspect retired central copy: {error}"
            )),
        }
        manifest.experts.remove(id);
    }
}

fn retired_expert_is_preserved(id: &str, central: &Path, manifest: &Manifest) -> bool {
    manifest
        .experts
        .get(id)
        .is_some_and(|entry| entry.pending_user_review)
        || crate::commands::acp::read_market_skill_marker(central).is_some()
        || retained_runtime_env_dir(central).is_some()
}

fn remove_retired_expert_links(id: &str, central: &Path) -> Result<(), ExpertsError> {
    for agent in supported_agents() {
        let (preferred, paths) = managed_expert_link_paths(id, agent)?;
        reconcile_managed_link_paths(central, &preferred, &paths, false)
            .map_err(|(path, error)| experts_error_from_managed(error, &path))?;
    }
    Ok(())
}

enum InstallAction {
    Skipped,
    Installed,
    Updated,
}

fn install_or_refresh_expert(
    meta: &ExpertMetadata,
    manifest: &mut Manifest,
) -> Result<InstallAction, ExpertsError> {
    let central_path = expert_central_path(&meta.id);
    if crate::commands::acp::read_market_skill_marker(&central_path).is_some() {
        tracing::debug!(
            target: "system_skills",
            skill_id = %meta.id,
            "keeping market override for bundled Skill"
        );
        return Ok(InstallAction::Skipped);
    }
    let bundled_hash = &meta.bundled_hash;
    let manifest_entry = manifest.experts.get(&meta.id).cloned().unwrap_or_default();
    let path_exists = fs::symlink_metadata(&central_path).is_ok();
    let legacy_source = crate::system_skills::repository_dir().join(&meta.id);
    let legacy_copy = managed_copy_is_owned(&legacy_source, &central_path);
    let legacy_link = !legacy_copy && managed_link_is_owned(&legacy_source, &central_path);

    if path_exists {
        let on_disk_hash = hash_disk_directory(&central_path).unwrap_or_default();
        if &on_disk_hash == bundled_hash && !legacy_link && !legacy_copy {
            migrate_runtime_envs(&central_path, &legacy_source, &meta.id)?;
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
    }
    refresh_bundled_expert(meta, &central_path, &legacy_source, legacy_link)?;
    manifest.experts.insert(
        meta.id.clone(),
        ManifestEntry {
            hash: bundled_hash.clone(),
            installed_at: Utc::now().to_rfc3339(),
            pending_user_review: false,
        },
    );
    Ok(if path_exists {
        InstallAction::Updated
    } else {
        InstallAction::Installed
    })
}

fn refresh_bundled_expert(
    meta: &ExpertMetadata,
    target: &Path,
    legacy_source: &Path,
    legacy_link: bool,
) -> Result<(), ExpertsError> {
    if legacy_link {
        return refresh_bundled_from_legacy_link(meta, target, legacy_source);
    }
    replace_bundled_contents(meta, target)?;
    migrate_runtime_envs(target, legacy_source, &meta.id)
}

fn refresh_bundled_from_legacy_link(
    meta: &ExpertMetadata,
    target: &Path,
    legacy_source: &Path,
) -> Result<(), ExpertsError> {
    remove_skill_entry(target)
        .map_err(|error| superseded_skill_dir_error(&meta.id, target, error))?;
    if let Err(error) = extract_expert_to_disk(meta, target) {
        return Err(restore_legacy_link(legacy_source, target, &meta.id, error));
    }
    if let Err(error) = migrate_runtime_envs(target, legacy_source, &meta.id) {
        return Err(rollback_legacy_runtime_migration(
            legacy_source,
            target,
            &meta.id,
            error,
        ));
    }
    Ok(())
}

fn rollback_legacy_runtime_migration(
    legacy_source: &Path,
    target: &Path,
    id: &str,
    original_error: ExpertsError,
) -> ExpertsError {
    let restored = restore_runtime_envs(target, legacy_source, id, original_error);
    if retained_runtime_env_dir(target).is_some() {
        return ExpertsError::Io(format!(
            "{restored}; partial bundled Skill for '{id}' was retained for a later migration retry"
        ));
    }
    restore_legacy_link(legacy_source, target, id, restored)
}

fn replace_bundled_contents(meta: &ExpertMetadata, target: &Path) -> Result<(), ExpertsError> {
    match fs::symlink_metadata(target) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            clear_bundled_skill_contents(target, &meta.id)?;
        }
        Ok(_) => {
            remove_skill_entry(target)
                .map_err(|error| superseded_skill_dir_error(&meta.id, target, error))?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    extract_expert_to_disk(meta, target)
}

fn clear_bundled_skill_contents(target: &Path, id: &str) -> Result<(), ExpertsError> {
    let mut preserved_runtime_envs = Vec::new();
    for entry in fs::read_dir(target)? {
        let entry = entry?;
        let path = entry.path();
        if is_runtime_env_dir(&path) {
            preserved_runtime_envs.push(entry.file_name().to_string_lossy().into_owned());
            continue;
        }
        remove_skill_entry(&path).map_err(|error| superseded_skill_dir_error(id, &path, error))?;
    }
    tracing::info!(
        target: "system_skills",
        skill_id = id,
        target = %target.display(),
        preserved_runtime_envs = ?preserved_runtime_envs,
        "cleared bundled Skill contents while preserving runtime environments"
    );
    Ok(())
}

fn restore_legacy_link(
    source: &Path,
    target: &Path,
    id: &str,
    original_error: ExpertsError,
) -> ExpertsError {
    let cleanup = fs::symlink_metadata(target)
        .is_ok()
        .then(|| remove_skill_entry(target))
        .transpose();
    if let Err(error) = cleanup {
        return ExpertsError::Io(format!(
            "{original_error}; failed to clear partial bundled Skill for '{id}': {error}"
        ));
    }
    match create_link_raw(source, target) {
        Ok(_) => original_error,
        Err(error) => ExpertsError::Io(format!(
            "{original_error}; failed to restore legacy link for '{id}': {error}"
        )),
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
                if is_runtime_env_dir(d.path()) {
                    tracing::debug!(
                        target: "system_skills",
                        path = %d.path().display(),
                        "skipping bundled runtime environment"
                    );
                    continue;
                }
                extract_bundle_dir(d, bundle_prefix, target)?;
            }
        }
    }
    Ok(())
}

// ─── Commands: list / status ────────────────────────────────────────────

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn experts_list() -> Result<Vec<ExpertListItem>, ExpertsError> {
    let meta_list = active_metadata();
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
    active_metadata()
        .into_iter()
        .filter(|metadata| !is_codex_native(metadata) && !is_computer_use(metadata))
        .map(|metadata| metadata.id)
        .collect()
}

pub(crate) fn managed_ready_expert_ids() -> Vec<String> {
    active_metadata()
        .into_iter()
        .filter(|metadata| !is_codex_native(metadata) && !is_computer_use(metadata))
        .filter(|metadata| expert_central_path(&metadata.id).exists())
        .map(|metadata| metadata.id)
        .collect()
}

pub(crate) fn managed_codex_native_ids() -> Vec<String> {
    active_metadata()
        .into_iter()
        .filter(is_codex_native)
        .map(|metadata| metadata.id)
        .collect()
}

pub(crate) fn managed_ready_codex_native_ids() -> Vec<String> {
    active_metadata()
        .into_iter()
        .filter(is_codex_native)
        .filter(|metadata| expert_central_path(&metadata.id).exists())
        .map(|metadata| metadata.id)
        .collect()
}

pub(crate) fn managed_computer_use_ids() -> Vec<String> {
    active_metadata()
        .into_iter()
        .filter(is_computer_use)
        .map(|metadata| metadata.id)
        .collect()
}

pub(crate) fn managed_ready_computer_use_ids() -> Vec<String> {
    active_metadata()
        .into_iter()
        .filter(is_computer_use)
        .filter(|metadata| expert_central_path(&metadata.id).exists())
        .map(|metadata| metadata.id)
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
fn link_one_exact_locked(
    expert_id: &str,
    agent_type: AgentType,
) -> Result<ExpertInstallStatus, ExpertsError> {
    let expert_id =
        validate_skill_id(expert_id).map_err(|e| ExpertsError::Metadata(e.to_string()))?;
    let _ = find_metadata(&expert_id)?;
    link_central_skill_locked(&expert_id, agent_type)
}

fn link_central_skill_locked(
    expert_id: &str,
    agent_type: AgentType,
) -> Result<ExpertInstallStatus, ExpertsError> {
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
        expert_id: expert_id.to_string(),
        agent_type,
        state,
        link_path: link_path.to_string_lossy().to_string(),
        target_path,
        expected_target_path: central.to_string_lossy().to_string(),
        copy_mode,
    })
}

/// Make the gateway Skill available before an Agent process is spawned. This
/// path validates and reconciles only the gateway bundle, so first-session
/// startup does not wait for every bundled Skill to be hashed.
pub(crate) async fn ensure_builtin_gateway_skill_ready(
    agent_type: AgentType,
) -> Result<ExpertInstallStatus, ExpertsError> {
    let started_at = Instant::now();
    let _guard = mutation_lock().lock().await;
    let status = tokio::task::spawn_blocking(move || {
        ensure_builtin_gateway_skill_ready_blocking(agent_type)
    })
    .await
    .map_err(|error| ExpertsError::Io(format!("gateway Skill join error: {error}")))??;
    tracing::info!(
        target: "system_skills",
        agent = %agent_type,
        elapsed_ms = started_at.elapsed().as_millis(),
        link_state = ?status.state,
        "gateway Skill ready before Agent spawn"
    );
    Ok(status)
}

fn ensure_builtin_gateway_skill_ready_blocking(
    agent_type: AgentType,
) -> Result<ExpertInstallStatus, ExpertsError> {
    let _shared_guard = crate::commands::acp::shared_skill_mutation_guard();
    let central = central_experts_dir();
    fs::create_dir_all(&central)?;
    let metadata = bundled_metadata_for_id(CAPABILITY_GATEWAY_EXPERT_ID)?;
    let mut manifest = load_manifest();
    let original_manifest = manifest.clone();
    let central_changed = !matches!(
        install_or_refresh_expert(&metadata, &mut manifest)?,
        InstallAction::Skipped
    );
    if manifest != original_manifest {
        manifest.installed_at = Utc::now().to_rfc3339();
        save_manifest(&manifest)?;
    }
    if central_changed || manifest != original_manifest {
        invalidate_central_experts_cache("gateway_skill_ready");
    }
    link_central_skill_locked(CAPABILITY_GATEWAY_EXPERT_ID, agent_type)
}

fn link_with_dependencies_locked(
    expert_id: &str,
    agent_type: AgentType,
) -> Result<ExpertInstallStatus, ExpertsError> {
    let mut root_status = None;
    for dependency in dependency_order(expert_id)? {
        let status = link_one_exact_locked(&dependency, agent_type)?;
        if dependency == expert_id {
            root_status = Some(status);
        }
    }
    root_status.ok_or_else(|| ExpertsError::NotFound(expert_id.to_string()))
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn experts_link_to_agent(
    expert_id: String,
    agent_type: AgentType,
) -> Result<ExpertInstallStatus, ExpertsError> {
    let _guard = mutation_lock().lock().await;
    link_with_dependencies_locked(&expert_id, agent_type)
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
    ensure_dependency_not_in_use(&expert_id, agent_type)?;

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

fn sort_link_operations<T>(values: &mut [(usize, T)], operation: impl Fn(&T) -> (&str, bool)) {
    values.sort_by(|(left_index, left), (right_index, right)| {
        let (left_id, left_enable) = operation(left);
        let (right_id, right_enable) = operation(right);
        match (left_enable, right_enable) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            (true, true) => left_index.cmp(right_index),
            (false, false) => dependency_order(right_id)
                .map(|order| order.len())
                .unwrap_or_default()
                .cmp(
                    &dependency_order(left_id)
                        .map(|order| order.len())
                        .unwrap_or_default(),
                )
                .then_with(|| left_index.cmp(right_index)),
        }
    });
}

fn managed_expert_target_results(
    expert_id: &str,
    agent_type: AgentType,
    enable: bool,
) -> Vec<LinkOpResult> {
    if !enable {
        if let Err(error) = ensure_dependency_not_in_use(expert_id, agent_type) {
            return vec![link_failure(expert_id, agent_type, error.to_string())];
        }
        return managed_expert_pair_result(expert_id, agent_type, false)
            .into_iter()
            .collect();
    }
    let ordered = match dependency_order(expert_id) {
        Ok(ordered) => ordered,
        Err(error) => return vec![link_failure(expert_id, agent_type, error.to_string())],
    };
    let mut results = Vec::new();
    for dependency in ordered {
        if !expert_central_path(&dependency).exists() {
            results.push(link_failure(
                &dependency,
                agent_type,
                ExpertsError::CentralUnavailable(format!(
                    "required expert '{dependency}' is not installed in central store"
                ))
                .to_string(),
            ));
            break;
        }
        if let Some(result) = managed_expert_pair_result(&dependency, agent_type, true) {
            let ok = result.ok;
            results.push(result);
            if !ok {
                break;
            }
        }
    }
    results
}

pub(crate) async fn reconcile_managed_experts(
    targets: &[(AgentType, String, bool)],
) -> Vec<LinkOpResult> {
    let _guard = mutation_lock().lock().await;
    let mut ordered = targets.iter().cloned().enumerate().collect::<Vec<_>>();
    sort_link_operations(&mut ordered, |value| (&value.1, value.2));
    ordered
        .into_iter()
        .flat_map(|(_, (agent_type, expert_id, enable))| {
            managed_expert_target_results(&expert_id, agent_type, enable)
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
    let result_count = ops.len();
    let mut indexed = ops.into_iter().enumerate().collect::<Vec<_>>();
    sort_link_operations(&mut indexed, |value| (&value.expert_id, value.enable));
    let mut out = vec![None; result_count];
    for (index, op) in indexed {
        let LinkOp {
            expert_id,
            agent_type,
            enable,
        } = op;
        let res = if enable {
            link_with_dependencies_locked(&expert_id, agent_type).map(Some)
        } else {
            unlink_one_locked(&expert_id, agent_type).map(|()| None)
        };
        out[index] = Some(match res {
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
    Ok(out.into_iter().flatten().collect())
}

/// One-shot snapshot of every (expert, agent) link state — lets the matrix UI
/// render the whole grid from a single round-trip instead of one
/// `experts_get_install_status` call per expert.
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn experts_list_all_install_statuses() -> Result<Vec<ExpertInstallStatus>, ExpertsError> {
    let agents = supported_agents();
    let metadata = active_metadata();
    let mut out = Vec::with_capacity(metadata.len() * agents.len());
    for meta in metadata {
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

pub(crate) async fn reconcile_system_repo_links(ids: &[String]) -> Result<(), ExpertsError> {
    let _guard = mutation_lock().lock().await;
    let result = reconcile_system_repo_links_locked(ids);
    // A repository reconciliation can replace central Skill directories or
    // migrate their runtime environments. Never reuse a prior startup cache.
    invalidate_central_experts_cache("system_repository_reconciled");
    result
}

fn reconcile_system_repo_links_locked(ids: &[String]) -> Result<(), ExpertsError> {
    let source_root = crate::system_skills::repository_dir();
    for id in ids {
        let source = source_root.join(id);
        if !source.join("SKILL.md").is_file() {
            return Err(ExpertsError::CentralUnavailable(format!(
                "system skill '{id}' is missing SKILL.md"
            )));
        }
        let target = expert_central_path(id);
        match prepare_system_skill_target(&source, &target, id)? {
            SystemSkillTarget::Ready => {}
            // A runtime environment is still in the way. Linking would have to
            // delete it, so this skill keeps serving its own directory and is
            // reconsidered on the next reconcile.
            SystemSkillTarget::KeptForRuntimeEnv => continue,
        }
        reconcile_managed_link_entry(&source, &target, true)
            .map_err(|error| experts_error_from_managed(error, &target))?;
    }
    Ok(())
}

/// Whether `target` is ready to be replaced with a managed link to `source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SystemSkillTarget {
    /// Nothing is in the way: either the link is already in place, or the
    /// previous directory has been cleared out.
    Ready,
    /// A runtime environment could not be moved to safety, so the previous
    /// directory was left untouched and must not be linked over.
    KeptForRuntimeEnv,
}

/// Make `target` safe to replace with a managed link to `source`.
///
/// The skill keeps its own name throughout — the previous directory is never
/// renamed, so no `<id>.user-backup-<timestamp>` sibling ever appears next to
/// it. Its content is superseded by the repository checkout at `source` and is
/// deleted outright, with one exception: the runtime environments named in
/// [`RUNTIME_ENV_DIR_NAMES`] are moved into `source` first, so they survive the
/// switch and stay reachable through the link. If one cannot be moved because
/// the incoming version already ships its own, the whole directory is left
/// alone rather than trading an installed environment for a link.
fn prepare_system_skill_target(
    source: &Path,
    target: &Path,
    id: &str,
) -> Result<SystemSkillTarget, ExpertsError> {
    if managed_link_is_owned(source, target) {
        return Ok(SystemSkillTarget::Ready);
    }
    if managed_copy_is_owned(source, target) {
        refresh_runtime_venv_from_copy(source, target, id)?;
        return Ok(SystemSkillTarget::Ready);
    }
    if fs::symlink_metadata(target).is_err() {
        return Ok(SystemSkillTarget::Ready);
    }
    // Checked up front, not per directory: a partial move would strip one
    // environment and keep another, leaving the skill in a state neither
    // version installed.
    if let Some(blocked) = blocking_runtime_env_dir(source, target) {
        tracing::warn!(
            target: "system_skills",
            skill_id = id,
            target = %target.display(),
            blocked = %blocked.display(),
            "keeping the existing system skill directory: its runtime environment \
             cannot move because the incoming version ships one"
        );
        return Ok(SystemSkillTarget::KeptForRuntimeEnv);
    }
    migrate_runtime_envs(source, target, id)?;
    if let Err(error) = remove_superseded_skill_dir(target, id) {
        return Err(restore_runtime_envs(source, target, id, error));
    }
    tracing::info!(
        target: "system_skills",
        skill_id = id,
        target = %target.display(),
        "replaced the previous system skill directory with the repository copy"
    );
    Ok(SystemSkillTarget::Ready)
}

/// Delete a directory that a managed bundled or repository source supersedes.
///
/// Unlike the rename this replaced, there is no copy left behind to fall back
/// to. Recovery comes from the managed source or the next bundled extraction,
/// so the skill never picks up a second name on disk.
fn remove_superseded_skill_dir(target: &Path, id: &str) -> Result<(), ExpertsError> {
    // Runtime environments are moved out before this point; refuse to delete if
    // one is somehow still here rather than destroy an installed environment.
    if let Some(env) = retained_runtime_env_dir(target) {
        return Err(ExpertsError::Io(format!(
            "refusing to replace system skill {} while a runtime environment is still inside it: {}",
            target.display(),
            env.display()
        )));
    }
    remove_skill_entry(target).map_err(|error| superseded_skill_dir_error(id, target, error))
}

fn superseded_skill_dir_error(id: &str, target: &Path, error: io::Error) -> ExpertsError {
    #[cfg(windows)]
    if is_windows_file_in_use(&error) {
        return ExpertsError::Io(format!(
            "system skill '{id}' is in use; close active Agent sessions or tools using {}, then retry: {error}",
            target.display()
        ));
    }
    let _ = id;
    ExpertsError::Io(format!(
        "replace existing system skill {}: {error}",
        target.display()
    ))
}

/// Undo [`migrate_runtime_envs`] after a later step failed.
///
/// Everything this moves was moved out of `target` a moment ago, so putting it
/// back leaves the skill usable with its environment intact until the next
/// reconcile retries. A failure here is reported alongside the original error
/// instead of replacing it.
fn restore_runtime_envs(
    source: &Path,
    target: &Path,
    id: &str,
    original_error: ExpertsError,
) -> ExpertsError {
    let mut failures = Vec::new();
    if retained_runtime_env_dir(source).is_some() && !target.is_dir() {
        if let Err(error) = fs::create_dir_all(target) {
            return ExpertsError::Io(format!(
                "{original_error}; failed to recreate {} for runtime restoration: {error}",
                target.display()
            ));
        }
    }
    for name in RUNTIME_ENV_DIR_NAMES {
        let moved = source.join(name);
        let original = target.join(name);
        if !moved.is_dir() || original.exists() {
            continue;
        }
        if let Err(error) = rename_system_skill_entry(&moved, &original, id) {
            failures.push(format!(
                "{} -> {}: {error}",
                moved.display(),
                original.display()
            ));
        }
    }
    if failures.is_empty() {
        return original_error;
    }
    ExpertsError::Io(format!(
        "{original_error}; failed to restore runtime environments [{}]",
        failures.join("; ")
    ))
}

#[cfg(not(windows))]
fn rename_system_skill_entry(source: &Path, target: &Path, _id: &str) -> io::Result<()> {
    fs::rename(source, target)
}

#[cfg(windows)]
fn rename_system_skill_entry(source: &Path, target: &Path, id: &str) -> io::Result<()> {
    const RETRY_DELAYS: [Duration; 5] = [
        Duration::from_millis(50),
        Duration::from_millis(100),
        Duration::from_millis(250),
        Duration::from_millis(500),
        Duration::from_millis(1_000),
    ];

    for (index, delay) in RETRY_DELAYS.iter().enumerate() {
        match fs::rename(source, target) {
            Ok(()) => return Ok(()),
            Err(error) if is_windows_file_in_use(&error) => {
                tracing::warn!(
                    target: "system_skills",
                    skill_id = id,
                    source = %source.display(),
                    destination = %target.display(),
                    attempt = index + 1,
                    retry_delay_ms = delay.as_millis(),
                    os_error = ?error.raw_os_error(),
                    "system skill path is busy; retrying rename"
                );
                std::thread::sleep(*delay);
            }
            Err(error) => return Err(error),
        }
    }
    fs::rename(source, target)
}

#[cfg(windows)]
fn is_windows_file_in_use(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(5 | 32 | 33))
}

fn refresh_runtime_venv_from_copy(
    source: &Path,
    target: &Path,
    id: &str,
) -> Result<(), ExpertsError> {
    let active_venv = target.join(".venv");
    if !active_venv.is_dir() {
        return Ok(());
    }
    let source_venv = source.join(".venv");
    let backup = source.join(".venv.system-update-backup");
    if backup.exists() {
        return Err(ExpertsError::Io(format!(
            "runtime environment backup already exists: {}",
            backup.display()
        )));
    }
    if source_venv.exists() {
        rename_system_skill_entry(&source_venv, &backup, id).map_err(|error| {
            ExpertsError::Io(format!(
                "preserve existing runtime environment {}: {error}",
                source_venv.display()
            ))
        })?;
    }
    if let Err(error) = rename_system_skill_entry(&active_venv, &source_venv, id) {
        if backup.exists() {
            let _ = rename_system_skill_entry(&backup, &source_venv, id);
        }
        return Err(ExpertsError::Io(format!(
            "preserve runtime environment {}: {error}",
            active_venv.display()
        )));
    }
    if backup.exists() {
        if let Err(error) = remove_skill_entry(&backup) {
            tracing::warn!(
                target: "system_skills",
                backup = %backup.display(),
                "failed to remove stale runtime environment backup: {error}"
            );
        }
    }
    Ok(())
}

/// Move every runtime environment directory out of `target` and into `source`,
/// so installed dependencies survive managed source replacement.
///
/// Callers must clear this with [`blocking_runtime_env_dir`] first: this moves
/// only what `source` has room for, so a collision it did not screen out would
/// leave one environment moved and another behind.
fn migrate_runtime_envs(source: &Path, target: &Path, id: &str) -> Result<(), ExpertsError> {
    for name in RUNTIME_ENV_DIR_NAMES {
        let old_env = target.join(name);
        let new_env = source.join(name);
        if old_env.is_dir() && !new_env.exists() {
            rename_system_skill_entry(&old_env, &new_env, id).map_err(|error| {
                ExpertsError::Io(format!(
                    "move runtime environment {} to {}: {error}",
                    old_env.display(),
                    new_env.display()
                ))
            })?;
        }
    }
    Ok(())
}

/// The first runtime environment in `target` that [`migrate_runtime_envs`] could
/// not move, because `source` already holds one under that name.
///
/// Some of these are cheap to rebuild and some cost a long native compile, and
/// nothing on disk says which. So the presence of one is enough to abandon the
/// switch to a link and leave the directory as it stands.
fn blocking_runtime_env_dir(source: &Path, target: &Path) -> Option<PathBuf> {
    RUNTIME_ENV_DIR_NAMES
        .iter()
        .map(|name| target.join(name))
        .find(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .map(|name| source.join(name).exists())
                    .unwrap_or(false)
        })
}

/// The first runtime environment directory still inside `path`, if any.
fn retained_runtime_env_dir(path: &Path) -> Option<PathBuf> {
    RUNTIME_ENV_DIR_NAMES
        .iter()
        .map(|name| path.join(name))
        .find(|path| path.is_dir())
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

    fn make_dir(path: &Path) {
        fs::create_dir_all(path).expect("create dir");
    }

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            make_dir(parent);
        }
        fs::write(path, contents).expect("write");
    }

    /// A repository checkout at `<root>/.system-repo/<id>` and the user-facing
    /// skill directory at `<root>/<id>`, mirroring the real central layout.
    fn skill_pair(root: &Path, id: &str) -> (PathBuf, PathBuf) {
        (root.join(".system-repo").join(id), root.join(id))
    }

    /// Names in the central directory that look like the old backup siblings.
    fn backup_siblings(root: &Path) -> Vec<String> {
        fs::read_dir(root)
            .expect("read central dir")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.contains("user-backup"))
            .collect()
    }

    #[test]
    fn retained_runtime_env_dir_ignores_a_dir_without_runtime_envs() {
        let temp = tempfile::tempdir().expect("tempdir");
        let skill_dir = temp.path().join("imagegen");
        write_file(&skill_dir.join("SKILL.md"), "# skill");

        assert_eq!(retained_runtime_env_dir(&skill_dir), None);
    }

    #[test]
    fn retained_runtime_env_dir_finds_each_runtime_dir_name() {
        for name in RUNTIME_ENV_DIR_NAMES {
            let temp = tempfile::tempdir().expect("tempdir");
            let skill_dir = temp.path().join("imagegen");
            make_dir(&skill_dir.join(name));

            assert_eq!(
                retained_runtime_env_dir(&skill_dir),
                Some(skill_dir.join(name)),
                "expected {name} to be detected"
            );
        }
    }

    #[test]
    fn blocking_runtime_env_dir_is_clear_when_the_repository_ships_none() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (source, target) = skill_pair(temp.path(), "imagegen");
        write_file(&source.join("SKILL.md"), "# incoming");
        for name in RUNTIME_ENV_DIR_NAMES {
            make_dir(&target.join(name));
        }

        assert_eq!(blocking_runtime_env_dir(&source, &target), None);
    }

    #[test]
    fn blocking_runtime_env_dir_reports_each_colliding_name() {
        for name in RUNTIME_ENV_DIR_NAMES {
            let temp = tempfile::tempdir().expect("tempdir");
            let (source, target) = skill_pair(temp.path(), "imagegen");
            make_dir(&source.join(name));
            make_dir(&target.join(name));

            assert_eq!(
                blocking_runtime_env_dir(&source, &target),
                Some(target.join(name)),
                "expected the {name} collision to block the switch"
            );
        }
    }

    #[test]
    fn migrate_runtime_envs_moves_every_runtime_dir_into_the_repository_copy() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (source, target) = skill_pair(temp.path(), "imagegen");
        make_dir(&source);
        for name in RUNTIME_ENV_DIR_NAMES {
            write_file(&target.join(name).join("marker"), name);
        }

        migrate_runtime_envs(&source, &target, "imagegen").expect("migrate");

        for name in RUNTIME_ENV_DIR_NAMES {
            assert_eq!(
                fs::read_to_string(source.join(name).join("marker")).expect("read"),
                name,
                "{name} should arrive in the repository copy intact"
            );
            assert!(
                !target.join(name).exists(),
                "{name} should no longer be in the skill directory"
            );
        }
    }

    #[test]
    fn remove_superseded_skill_dir_deletes_content_the_repository_supersedes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("imagegen");
        write_file(&target.join("SKILL.md"), "# skill");
        write_file(&target.join("scripts").join("run.py"), "print()");

        remove_superseded_skill_dir(&target, "imagegen").expect("remove");

        assert!(!target.exists(), "the superseded directory should be gone");
    }

    #[test]
    fn remove_superseded_skill_dir_refuses_to_delete_a_runtime_env() {
        for name in RUNTIME_ENV_DIR_NAMES {
            let temp = tempfile::tempdir().expect("tempdir");
            let target = temp.path().join("imagegen");
            write_file(&target.join(name).join("marker"), name);

            let error = remove_superseded_skill_dir(&target, "imagegen")
                .expect_err("a runtime environment must stop the delete");

            assert!(
                error.to_string().contains("runtime environment"),
                "unexpected error for {name}: {error}"
            );
            assert!(
                target.join(name).is_dir(),
                "{name} must survive the refusal"
            );
        }
    }

    #[test]
    fn restore_runtime_envs_puts_a_migrated_env_back_and_keeps_the_original_error() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (source, target) = skill_pair(temp.path(), "imagegen");
        make_dir(&target);
        write_file(&source.join(".venv").join("marker"), "installed");

        let restored = restore_runtime_envs(
            &source,
            &target,
            "imagegen",
            ExpertsError::Io("delete failed".into()),
        );

        assert!(
            restored.to_string().contains("delete failed"),
            "the original error must survive: {restored}"
        );
        assert_eq!(
            fs::read_to_string(target.join(".venv").join("marker")).expect("read"),
            "installed",
            "the environment should be back in the skill directory"
        );
        assert!(
            !source.join(".venv").exists(),
            "the environment should no longer be in the repository copy"
        );
    }

    #[test]
    fn prepare_system_skill_target_replaces_a_plain_dir_without_renaming_it() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (source, target) = skill_pair(temp.path(), "imagegen");
        write_file(&source.join("SKILL.md"), "# incoming");
        write_file(&target.join("SKILL.md"), "# hand edited");

        let outcome = prepare_system_skill_target(&source, &target, "imagegen").expect("prepare");

        assert_eq!(outcome, SystemSkillTarget::Ready);
        assert!(
            !target.exists(),
            "the directory should be cleared for the link"
        );
        assert!(
            backup_siblings(temp.path()).is_empty(),
            "no backup sibling may be created: {:?}",
            backup_siblings(temp.path())
        );
    }

    #[test]
    fn prepare_system_skill_target_moves_runtime_envs_into_the_repository() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (source, target) = skill_pair(temp.path(), "imagegen");
        write_file(&source.join("SKILL.md"), "# incoming");
        write_file(&target.join("SKILL.md"), "# previous");
        for name in RUNTIME_ENV_DIR_NAMES {
            write_file(&target.join(name).join("marker"), name);
        }

        let outcome = prepare_system_skill_target(&source, &target, "imagegen").expect("prepare");

        assert_eq!(outcome, SystemSkillTarget::Ready);
        for name in RUNTIME_ENV_DIR_NAMES {
            assert_eq!(
                fs::read_to_string(source.join(name).join("marker")).expect("read"),
                name,
                "{name} must survive behind the link"
            );
        }
        assert!(!target.exists(), "the directory should be cleared");
        assert!(
            backup_siblings(temp.path()).is_empty(),
            "no backup sibling may be created"
        );
    }

    #[test]
    fn prepare_system_skill_target_keeps_the_dir_when_a_runtime_env_cannot_move() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (source, target) = skill_pair(temp.path(), "imagegen");
        write_file(&source.join("SKILL.md"), "# incoming");
        write_file(&source.join(".venv").join("marker"), "incoming");
        write_file(&target.join("SKILL.md"), "# previous");
        write_file(&target.join(".venv").join("marker"), "installed");

        let outcome = prepare_system_skill_target(&source, &target, "imagegen").expect("prepare");

        assert_eq!(outcome, SystemSkillTarget::KeptForRuntimeEnv);
        assert_eq!(
            fs::read_to_string(target.join(".venv").join("marker")).expect("read"),
            "installed",
            "the installed environment must not be touched"
        );
        assert_eq!(
            fs::read_to_string(target.join("SKILL.md")).expect("read"),
            "# previous",
            "the directory is left exactly as it stands"
        );
    }

    #[test]
    fn prepare_system_skill_target_is_a_no_op_when_nothing_is_there() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (source, target) = skill_pair(temp.path(), "imagegen");
        write_file(&source.join("SKILL.md"), "# incoming");

        let outcome = prepare_system_skill_target(&source, &target, "imagegen").expect("prepare");

        assert_eq!(outcome, SystemSkillTarget::Ready);
        assert!(!target.exists());
    }
}
