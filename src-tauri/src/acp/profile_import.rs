use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::acp::agent_storage::AgentStoragePaths;
use crate::models::agent::AgentType;

pub const PROFILE_IMPORT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileSourceRoots {
    pub home: PathBuf,
    pub xdg_config: PathBuf,
    pub xdg_data: PathBuf,
    profile_overrides: BTreeMap<String, PathBuf>,
    claude_mcp_path: PathBuf,
    codebuddy_mcp_path: PathBuf,
    cline_skills_dir: PathBuf,
}

impl ProfileSourceRoots {
    pub fn new(home: PathBuf, xdg_config: PathBuf, xdg_data: PathBuf) -> Self {
        let claude_mcp_path = home.join(".claude.json");
        let codebuddy_mcp_path = home.join(".codebuddy.json");
        let cline_skills_dir = home.join(".cline").join("skills");
        Self {
            home,
            xdg_config,
            xdg_data,
            profile_overrides: BTreeMap::new(),
            claude_mcp_path,
            codebuddy_mcp_path,
            cline_skills_dir,
        }
    }

    pub fn discover() -> Result<Self, ProfileImportError> {
        let home = dirs::home_dir().ok_or(ProfileImportError::HomeDirectoryMissing)?;
        let xdg_config =
            nonempty_env_path("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"));
        let xdg_data =
            nonempty_env_path("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local").join("share"));
        let mut sources = Self::new(home, xdg_config, xdg_data);
        for (agent_type, path) in resolved_profile_paths() {
            sources = sources.with_profile(agent_type, path);
        }
        sources.claude_mcp_path = crate::parsers::profile_paths::claude_mcp_config_path();
        sources.codebuddy_mcp_path = crate::parsers::profile_paths::codebuddy_mcp_config_path();
        sources.cline_skills_dir = crate::parsers::profile_paths::cline_skills_dir();
        Ok(sources)
    }

    pub fn with_profile(mut self, agent_type: AgentType, path: PathBuf) -> Self {
        self.profile_overrides.insert(
            crate::acp::registry::registry_id_for(agent_type).to_string(),
            path,
        );
        self
    }

    pub(crate) fn profile(&self, agent_type: AgentType) -> PathBuf {
        let registry_id = crate::acp::registry::registry_id_for(agent_type);
        self.profile_overrides
            .get(registry_id)
            .cloned()
            .unwrap_or_else(|| self.default_profile(agent_type))
    }

    pub(crate) fn claude_mcp_path(&self) -> &Path {
        &self.claude_mcp_path
    }

    pub(crate) fn codebuddy_mcp_path(&self) -> &Path {
        &self.codebuddy_mcp_path
    }

    pub(crate) fn cline_skills_dir(&self) -> &Path {
        &self.cline_skills_dir
    }

    fn default_profile(&self, agent_type: AgentType) -> PathBuf {
        match agent_type {
            AgentType::ClaudeCode => self.home.join(".claude"),
            AgentType::Codex => self.home.join(".codex"),
            AgentType::Gemini => self.home.join(".gemini"),
            AgentType::OpenClaw => self.home.join(".openclaw"),
            AgentType::OpenCode => self.xdg_config.join("opencode"),
            AgentType::Cline => self.home.join(".cline").join("data"),
            AgentType::Hermes => self.home.join(".hermes"),
            AgentType::CodeBuddy => self.home.join(".codebuddy"),
            AgentType::KimiCode => self.home.join(".kimi-code"),
            AgentType::Pi => self.home.join(".pi").join("agent"),
            AgentType::Grok => self.home.join(".grok"),
        }
    }
}

fn resolved_profile_paths() -> [(AgentType, PathBuf); 10] {
    [
        (
            AgentType::ClaudeCode,
            crate::parsers::claude::resolve_claude_config_dir(),
        ),
        (
            AgentType::Codex,
            crate::parsers::codex::resolve_codex_home_dir(),
        ),
        (
            AgentType::Gemini,
            crate::parsers::gemini::resolve_gemini_base_dir(),
        ),
        (
            AgentType::OpenClaw,
            crate::parsers::profile_paths::openclaw_state_dir(),
        ),
        (
            AgentType::Cline,
            crate::parsers::profile_paths::cline_data_dir(),
        ),
        (
            AgentType::Hermes,
            crate::parsers::hermes::resolve_hermes_home_dir(),
        ),
        (
            AgentType::CodeBuddy,
            crate::parsers::codebuddy::resolve_codebuddy_config_dir(),
        ),
        (
            AgentType::KimiCode,
            crate::parsers::kimi_code::resolve_kimi_code_home_dir(),
        ),
        (AgentType::Pi, crate::parsers::profile_paths::pi_agent_dir()),
        (
            AgentType::Grok,
            crate::parsers::grok::resolve_grok_home_dir(),
        ),
    ]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileImportEntry {
    pub source_root: PathBuf,
    pub source_relative: PathBuf,
    pub destination_relative: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileImportSpec {
    pub agent_type: AgentType,
    pub destination_root: PathBuf,
    pub entries: Vec<ProfileImportEntry>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileImportReport {
    pub imported_files: usize,
    pub skipped_existing: usize,
    pub skipped_unsafe_links: usize,
}

#[derive(Debug, Error)]
pub enum ProfileImportError {
    #[error("the user home directory is unavailable")]
    HomeDirectoryMissing,
    #[error("unsafe profile import path: {0}")]
    UnsafePath(PathBuf),
    #[error("profile import source escapes {root}: {path}")]
    SourceEscapes { root: PathBuf, path: PathBuf },
    #[error("profile import destination is outside private storage: {0}")]
    DestinationOutsideStorage(PathBuf),
    #[error("profile import I/O failed at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("profile import activation failed: {0}")]
    Activation(String),
}

pub fn profile_import_specs(
    paths: &AgentStoragePaths,
    sources: &ProfileSourceRoots,
) -> Vec<ProfileImportSpec> {
    super::profile_import_specs::build_profile_import_specs(paths, sources)
}

pub fn import_existing_profiles(
    paths: &AgentStoragePaths,
    sources: &ProfileSourceRoots,
) -> Result<ProfileImportReport, ProfileImportError> {
    import_profile_specs(paths, &profile_import_specs(paths, sources))
}

pub(crate) fn import_profile_specs(
    paths: &AgentStoragePaths,
    specs: &[ProfileImportSpec],
) -> Result<ProfileImportReport, ProfileImportError> {
    super::profile_import_fs::import_profile_specs(paths, specs)
}

pub(crate) fn import_entry(
    source_root: &Path,
    source_relative: impl Into<PathBuf>,
    destination_relative: impl Into<PathBuf>,
) -> ProfileImportEntry {
    ProfileImportEntry {
        source_root: source_root.to_path_buf(),
        source_relative: source_relative.into(),
        destination_relative: destination_relative.into(),
    }
}

fn nonempty_env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}
