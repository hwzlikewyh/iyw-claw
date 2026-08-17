use std::path::{Path, PathBuf};

use crate::acp::agent_storage::{AgentProfilePaths, AgentStorageConfig, AgentStoragePaths};
use crate::models::agent::AgentType;

const LOG_EXTENSIONS: &[&str] = &["log", "txt", "trace", "jsonl"];
const JSON_EXTENSIONS: &[&str] = &["json"];

#[derive(Clone, Copy)]
pub(super) enum AgentLogRule {
    CodexDatabase,
    Extensions(&'static [&'static str]),
}

pub(super) struct AgentLogTarget {
    pub agent_type: AgentType,
    pub group: &'static str,
    pub profile_root: PathBuf,
    pub path: PathBuf,
    pub rule: AgentLogRule,
}

impl AgentLogTarget {
    pub fn accepts_file(&self, name: &str) -> bool {
        match self.rule {
            AgentLogRule::CodexDatabase => codex_group_name(name).is_some(),
            AgentLogRule::Extensions(extensions) => Path::new(name)
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|ext| extensions.iter().any(|item| ext.eq_ignore_ascii_case(item))),
        }
    }

    pub fn codex_group(&self, name: &str) -> Option<String> {
        matches!(self.rule, AgentLogRule::CodexDatabase)
            .then(|| codex_group_name(name))
            .flatten()
    }
}

pub(super) fn targets(
    paths: &AgentStoragePaths,
    config: &AgentStorageConfig,
) -> Vec<AgentLogTarget> {
    crate::acp::registry::all_acp_agents()
        .into_iter()
        .flat_map(|agent_type| policy_for(paths, config, agent_type))
        .collect()
}

fn policy_for(
    paths: &AgentStoragePaths,
    config: &AgentStorageConfig,
    agent_type: AgentType,
) -> Vec<AgentLogTarget> {
    let profile = effective_profile(paths, config, agent_type);
    match agent_type {
        AgentType::ClaudeCode => vec![
            directory(
                &profile,
                agent_type,
                "claude_debug",
                &["debug"],
                LOG_EXTENSIONS,
            ),
            directory(
                &profile,
                agent_type,
                "claude_telemetry",
                &["telemetry"],
                JSON_EXTENSIONS,
            ),
        ],
        AgentType::Codex => vec![codex_database(&profile)],
        AgentType::OpenCode => open_code_targets(&profile),
        AgentType::Gemini => vec![gemini_logs(&profile)],
        AgentType::OpenClaw => vec![logs_directory(&profile, agent_type, "openclaw_logs")],
        AgentType::Cline => vec![logs_directory(&profile, agent_type, "cline_logs")],
        AgentType::Hermes => vec![logs_directory(&profile, agent_type, "hermes_logs")],
        AgentType::CodeBuddy => vec![logs_directory(&profile, agent_type, "codebuddy_logs")],
        AgentType::KimiCode => vec![logs_directory(&profile, agent_type, "kimi_code_logs")],
        AgentType::Pi => vec![logs_directory(&profile, agent_type, "pi_logs")],
        AgentType::Grok => vec![logs_directory(&profile, agent_type, "grok_logs")],
    }
}

fn effective_profile(
    paths: &AgentStoragePaths,
    config: &AgentStorageConfig,
    agent_type: AgentType,
) -> AgentProfilePaths {
    let registry_id = crate::acp::registry::registry_id_for(agent_type);
    match config.profile_overrides.get(registry_id) {
        Some(root) => AgentProfilePaths {
            root: root.clone(),
            env: crate::acp::agent_profile::override_profile_env(agent_type, root),
        },
        None => paths.profile(agent_type),
    }
}

fn open_code_targets(profile: &AgentProfilePaths) -> Vec<AgentLogTarget> {
    vec![
        directory(
            profile,
            AgentType::OpenCode,
            "opencode_logs",
            &["data", "opencode", "log"],
            LOG_EXTENSIONS,
        ),
        directory(
            profile,
            AgentType::OpenCode,
            "opencode_xet_logs",
            &["cache", "huggingface", "xet", "logs"],
            LOG_EXTENSIONS,
        ),
    ]
}

fn codex_database(profile: &AgentProfilePaths) -> AgentLogTarget {
    AgentLogTarget {
        agent_type: AgentType::Codex,
        group: "codex_log_database",
        profile_root: profile.root.clone(),
        path: profile.root.clone(),
        rule: AgentLogRule::CodexDatabase,
    }
}

fn gemini_logs(profile: &AgentProfilePaths) -> AgentLogTarget {
    let home = profile
        .env
        .get("GEMINI_CLI_HOME")
        .cloned()
        .unwrap_or_else(|| profile.root.clone());
    directory_at(
        profile,
        AgentType::Gemini,
        "gemini_logs",
        home.join(".gemini").join("logs"),
        LOG_EXTENSIONS,
    )
}

fn logs_directory(
    profile: &AgentProfilePaths,
    agent_type: AgentType,
    group: &'static str,
) -> AgentLogTarget {
    directory(profile, agent_type, group, &["logs"], LOG_EXTENSIONS)
}

fn directory(
    profile: &AgentProfilePaths,
    agent_type: AgentType,
    group: &'static str,
    segments: &[&str],
    extensions: &'static [&'static str],
) -> AgentLogTarget {
    let path = segments
        .iter()
        .fold(profile.root.clone(), |path, segment| path.join(segment));
    directory_at(profile, agent_type, group, path, extensions)
}

fn directory_at(
    profile: &AgentProfilePaths,
    agent_type: AgentType,
    group: &'static str,
    path: PathBuf,
    extensions: &'static [&'static str],
) -> AgentLogTarget {
    AgentLogTarget {
        agent_type,
        group,
        profile_root: profile.root.clone(),
        path,
        rule: AgentLogRule::Extensions(extensions),
    }
}

fn codex_group_name(name: &str) -> Option<String> {
    let base = name
        .strip_suffix("-wal")
        .or_else(|| name.strip_suffix("-shm"))
        .unwrap_or(name);
    let version = base.strip_prefix("logs_")?.strip_suffix(".sqlite")?;
    (!version.is_empty() && version.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| base.to_string())
}
