use std::collections::BTreeMap;
use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};

use crate::models::agent::AgentType;

const HERMES_CONFIG_DIAGNOSTIC_MAX_BYTES: u64 = 256 * 1024;
const CONFIG_READ_FAILED: &str = "hermes_memory_config_read_failed";
const CONFIG_TOO_LARGE: &str = "hermes_memory_config_too_large";
const CONFIG_INVALID_UTF8: &str = "hermes_memory_config_invalid_utf8";
const CONFIG_INVALID_YAML: &str = "hermes_memory_config_invalid_yaml";
const PROVIDER_INVALID_TYPE: &str = "hermes_memory_provider_invalid_type";

#[derive(Debug, Clone, Copy, Default)]
pub(crate) enum HermesNativeMemoryState {
    #[default]
    NotApplicable,
    No,
    Yes,
    Unknown,
}

impl HermesNativeMemoryState {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::NotApplicable => "not_applicable",
            Self::No => "no",
            Self::Yes => "yes",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct HermesNativeMemoryDiagnostics {
    pub state: HermesNativeMemoryState,
    pub reason_code: Option<&'static str>,
}

pub(crate) struct HermesLaunchMemoryDiagnostics {
    pub native_memory: HermesNativeMemoryDiagnostics,
    pub home_hash: Option<String>,
}

pub(crate) fn diagnose_hermes_memory(
    agent_type: AgentType,
    runtime_env: &BTreeMap<String, String>,
) -> HermesLaunchMemoryDiagnostics {
    if agent_type != AgentType::Hermes {
        return HermesLaunchMemoryDiagnostics {
            native_memory: HermesNativeMemoryDiagnostics::default(),
            home_hash: None,
        };
    }
    let home = crate::commands::acp::hermes_home_for_launch(runtime_env);
    HermesLaunchMemoryDiagnostics {
        native_memory: diagnose_config(&home.join("config.yaml")),
        home_hash: Some(super::identity::hermes_home_hash(&home_identity(&home))),
    }
}

fn diagnose_config(path: &Path) -> HermesNativeMemoryDiagnostics {
    match read_bounded(path) {
        Ok(None) => diagnostics(HermesNativeMemoryState::No, None),
        Ok(Some(raw)) => parse_provider(&raw),
        Err(reason_code) => diagnostics(HermesNativeMemoryState::Unknown, Some(reason_code)),
    }
}

fn read_bounded(path: &Path) -> Result<Option<String>, &'static str> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(CONFIG_READ_FAILED),
    };
    let mut bytes = Vec::new();
    file.take(HERMES_CONFIG_DIAGNOSTIC_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| CONFIG_READ_FAILED)?;
    if bytes.len() as u64 > HERMES_CONFIG_DIAGNOSTIC_MAX_BYTES {
        return Err(CONFIG_TOO_LARGE);
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| CONFIG_INVALID_UTF8)
}

fn parse_provider(raw: &str) -> HermesNativeMemoryDiagnostics {
    if raw.trim().is_empty() {
        return diagnostics(HermesNativeMemoryState::No, None);
    }
    let Ok(root) = serde_yaml::from_str::<serde_yaml::Value>(raw) else {
        return diagnostics(HermesNativeMemoryState::Unknown, Some(CONFIG_INVALID_YAML));
    };
    let Some(root) = root.as_mapping() else {
        return diagnostics(HermesNativeMemoryState::Unknown, Some(CONFIG_INVALID_YAML));
    };
    let Some(memory) = root.get(serde_yaml::Value::String("memory".into())) else {
        return diagnostics(HermesNativeMemoryState::No, None);
    };
    let Some(memory) = memory.as_mapping() else {
        return diagnostics(
            HermesNativeMemoryState::Unknown,
            Some(PROVIDER_INVALID_TYPE),
        );
    };
    let Some(provider) = memory.get(serde_yaml::Value::String("provider".into())) else {
        return diagnostics(HermesNativeMemoryState::No, None);
    };
    match provider.as_str() {
        Some(value) if value.trim().is_empty() => diagnostics(HermesNativeMemoryState::No, None),
        Some(_) => diagnostics(HermesNativeMemoryState::Yes, None),
        None => diagnostics(
            HermesNativeMemoryState::Unknown,
            Some(PROVIDER_INVALID_TYPE),
        ),
    }
}

fn diagnostics(
    state: HermesNativeMemoryState,
    reason_code: Option<&'static str>,
) -> HermesNativeMemoryDiagnostics {
    HermesNativeMemoryDiagnostics { state, reason_code }
}

fn home_identity(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(path))
                .unwrap_or_else(|_| path.to_path_buf())
        }
    })
}
