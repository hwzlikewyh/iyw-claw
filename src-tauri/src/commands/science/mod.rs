mod bundle;
mod filesystem;
mod links;
mod metadata;

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde::Serialize;
use tokio::sync::Mutex;

use crate::commands::experts::{ExpertInstallStatus, LinkOpResult};
use crate::models::agent::AgentType;

#[derive(Debug, thiserror::Error)]
pub enum ScienceError {
    #[error("science skill not found: {0}")]
    NotFound(String),
    #[error("science catalog conflicts with another managed skill: {0}")]
    IdCollision(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("metadata error: {0}")]
    Metadata(String),
    #[error("central science store is unavailable: {0}")]
    CentralUnavailable(String),
}

impl Serialize for ScienceError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<io::Error> for ScienceError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ScienceMetadata {
    pub id: String,
    pub category: String,
    pub icon: Option<String>,
    pub sort_order: i32,
    pub featured: bool,
    pub accent: Option<String>,
    pub needs_key: bool,
    pub needs_env: bool,
    pub display_name: BTreeMap<String, String>,
    pub description: BTreeMap<String, String>,
    pub bundled_hash: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScienceListItem {
    pub metadata: ScienceMetadata,
    pub installed_centrally: bool,
    pub user_modified: bool,
    pub central_path: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ScienceInstallReport {
    pub installed_count: usize,
    pub updated_count: usize,
    pub pending_user_review: Vec<String>,
    pub errors: Vec<String>,
}

pub(crate) fn mutation_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub(crate) fn central_path(skill_id: &str) -> PathBuf {
    crate::commands::experts::central_experts_dir().join(skill_id)
}

pub async fn ensure_central_science_installed() -> ScienceInstallReport {
    bundle::ensure_installed().await
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn science_list() -> Result<Vec<ScienceListItem>, ScienceError> {
    bundle::list()
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn science_read_content(skill_id: String) -> Result<String, ScienceError> {
    bundle::read_content(&skill_id)
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn science_list_all_install_statuses() -> Result<Vec<ExpertInstallStatus>, ScienceError> {
    links::list_all_install_statuses()
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn science_open_central_dir() -> Result<String, ScienceError> {
    let directory = crate::commands::experts::central_experts_dir();
    fs::create_dir_all(&directory)?;
    Ok(directory.to_string_lossy().to_string())
}

pub(crate) fn managed_science_ids() -> Vec<String> {
    links::managed_ids()
}

pub(crate) fn managed_ready_science_ids() -> Vec<String> {
    links::managed_ready_ids()
}

pub(crate) fn managed_science_has_owned_link(skill_id: &str, agents: &[AgentType]) -> bool {
    links::has_owned_link(skill_id, agents)
}

pub(crate) async fn reconcile_managed_science(
    targets: &[(AgentType, String, bool)],
) -> Vec<LinkOpResult> {
    links::reconcile(targets).await
}
