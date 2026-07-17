use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::acp::agent_storage::{
    resolve_root, AgentStorageConfig, AgentStoragePaths, STORAGE_ROOT_ENV,
};
use crate::models::agent::AgentType;

pub fn effective_startup_config(
    env_override: Option<OsString>,
    persisted: Option<&AgentStorageConfig>,
    server_fallback: Option<PathBuf>,
) -> Option<(AgentStoragePaths, AgentStorageConfig)> {
    let root = resolve_root(env_override, persisted, server_fallback)?;
    let mut config = persisted
        .cloned()
        .unwrap_or_else(|| AgentStorageConfig::confirmed(root.clone()));
    config.root = Some(root.clone());
    config.initialized = true;
    Some((AgentStoragePaths::new(root), config))
}

pub fn activate_startup_profile_env(paths: &AgentStoragePaths, config: &AgentStorageConfig) {
    std::env::set_var(STORAGE_ROOT_ENV, paths.root());
    for (key, value) in startup_profile_env(paths, config) {
        std::env::set_var(key, value);
    }
}

pub fn startup_profile_env(
    paths: &AgentStoragePaths,
    config: &AgentStorageConfig,
) -> BTreeMap<String, OsString> {
    let mut env = BTreeMap::new();
    for agent_type in crate::acp::registry::all_acp_agents() {
        let registry_id = crate::acp::registry::registry_id_for(agent_type);
        let profile_env = config
            .profile_overrides
            .get(registry_id)
            .map(|root| override_profile_env(agent_type, root))
            .unwrap_or_else(|| paths.profile(agent_type).env);
        for (key, value) in profile_env {
            env.insert(key.to_string(), OsString::from(value));
        }
    }
    env
}

pub fn startup_profile_env_matches(
    paths: &AgentStoragePaths,
    config: &AgentStorageConfig,
    mut current: impl FnMut(&str) -> Option<OsString>,
) -> bool {
    startup_profile_env(paths, config)
        .into_iter()
        .all(|(key, expected)| current(&key).as_ref() == Some(&expected))
}

pub fn startup_profile_env_is_complete(
    paths: &AgentStoragePaths,
    mut current: impl FnMut(&str) -> Option<OsString>,
) -> bool {
    crate::acp::registry::all_acp_agents()
        .into_iter()
        .all(|agent_type| {
            paths.profile(agent_type).env.keys().all(|key| {
                current(key)
                    .filter(|value| !value.is_empty())
                    .map(PathBuf::from)
                    .is_some_and(|path| path.is_absolute())
            })
        })
}

fn override_profile_env(agent_type: AgentType, root: &Path) -> BTreeMap<&'static str, PathBuf> {
    match agent_type {
        AgentType::OpenCode => {
            let mut env = BTreeMap::new();
            env.insert("XDG_CONFIG_HOME", root.join("config"));
            env.insert("XDG_DATA_HOME", root.join("data"));
            env.insert("XDG_CACHE_HOME", root.join("cache"));
            env
        }
        AgentType::OpenClaw => {
            let mut env = BTreeMap::new();
            env.insert("OPENCLAW_HOME", root.to_path_buf());
            env.insert("OPENCLAW_STATE_DIR", root.to_path_buf());
            env
        }
        AgentType::Gemini => {
            let mut env = BTreeMap::new();
            env.insert("GEMINI_CLI_HOME", root.to_path_buf());
            env
        }
        AgentType::ClaudeCode => single_profile_env(root, "CLAUDE_CONFIG_DIR"),
        AgentType::Codex => single_profile_env(root, "CODEX_HOME"),
        AgentType::Cline => single_profile_env(root, "CLINE_DIR"),
        AgentType::Hermes => single_profile_env(root, "HERMES_HOME"),
        AgentType::CodeBuddy => single_profile_env(root, "CODEBUDDY_CONFIG_DIR"),
        AgentType::KimiCode => single_profile_env(root, "KIMI_CODE_HOME"),
        AgentType::Pi => single_profile_env(root, "PI_CODING_AGENT_DIR"),
        AgentType::Grok => single_profile_env(root, "GROK_HOME"),
    }
}

fn single_profile_env(root: &Path, env_key: &'static str) -> BTreeMap<&'static str, PathBuf> {
    let mut env = BTreeMap::new();
    env.insert(env_key, root.to_path_buf());
    env
}
