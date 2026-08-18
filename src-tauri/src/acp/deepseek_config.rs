use std::collections::BTreeMap;
use std::path::PathBuf;

use semver::Version;

use crate::acp::error::AcpError;
use crate::models::agent::AgentType;

const BASE_URL_ENV: &str = "DEEPSEEK_BASE_URL";
const PROVIDER_ENV: &str = "DEEPSEEK_ACP_PROVIDER";
const HOST_OWNED_ENV: [&str; 2] = ["DSH_HOME", "DEEPSEEK_ACP_SESSIONS_ROOT"];

pub(crate) fn fallback_tool_version(agent_type: AgentType) -> Option<&'static str> {
    if agent_type != AgentType::DeepSeek {
        return None;
    }
    crate::acp::trusted_agents::definition_for_agent(agent_type)
        .map(|definition| definition.version_floor.minimum_tool_version)
}

pub(crate) fn validate_tool_version(agent_type: AgentType, value: &str) -> Result<(), String> {
    if agent_type != AgentType::DeepSeek {
        return Ok(());
    }
    let required = fallback_tool_version(agent_type)
        .ok_or_else(|| "DeepSeek Harness trusted definition is unavailable".to_string())?;
    let offered = Version::parse(value.trim())
        .map_err(|_| "DeepSeek Harness version is not valid SemVer".to_string())?;
    let required = Version::parse(required)
        .map_err(|_| "DeepSeek Harness trusted version is invalid".to_string())?;
    if offered < required {
        return Err(format!(
            "DeepSeek Harness requires version {required} or newer"
        ));
    }
    Ok(())
}

pub(crate) fn managed_file_system_roots(agent_type: AgentType) -> Vec<PathBuf> {
    if agent_type != AgentType::DeepSeek {
        return Vec::new();
    }
    HOST_OWNED_ENV
        .iter()
        .filter_map(|key| std::env::var_os(key).map(PathBuf::from))
        .filter(|path| !path.as_os_str().is_empty())
        .filter(|path| {
            if path.is_absolute() {
                return true;
            }
            tracing::warn!(
                agent = "deepseek",
                path = %path.display(),
                "[ACP] ignored non-absolute managed DeepSeek filesystem root"
            );
            false
        })
        .collect()
}

pub(crate) fn normalize_runtime_env(
    agent_type: AgentType,
    env: &mut BTreeMap<String, String>,
) -> Result<(), AcpError> {
    if agent_type != AgentType::DeepSeek {
        return Ok(());
    }
    let before = env.len();
    env.retain(|key, _| {
        !HOST_OWNED_ENV
            .iter()
            .any(|owned| key.eq_ignore_ascii_case(owned))
    });
    let removed_host_override = env.len() != before;
    if removed_host_override {
        tracing::warn!(
            agent = "deepseek",
            "[ACP] ignored user override for host-owned DeepSeek profile path"
        );
    }
    normalize_base_url(env)?;
    normalize_provider(env)?;
    for key in ["DEEPSEEK_API_KEY", "DEEPSEEK_ACP_MODEL"] {
        normalize_optional_value(env, key);
    }
    Ok(())
}

fn normalize_base_url(env: &mut BTreeMap<String, String>) -> Result<(), AcpError> {
    let Some(raw) = env.get(BASE_URL_ENV) else {
        return Ok(());
    };
    let value = raw.trim().trim_end_matches('/').to_string();
    if value.is_empty() {
        env.remove(BASE_URL_ENV);
        return Ok(());
    }
    let parsed = reqwest::Url::parse(&value)
        .map_err(|_| AcpError::protocol("DeepSeek base URL must be a valid HTTP(S) URL"))?;
    let valid = matches!(parsed.scheme(), "http" | "https")
        && parsed.host_str().is_some()
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.query().is_none()
        && parsed.fragment().is_none();
    if !valid {
        return Err(AcpError::protocol(
            "DeepSeek base URL must be HTTP(S) without credentials, query, or fragment",
        ));
    }
    env.insert(BASE_URL_ENV.to_string(), value);
    Ok(())
}

fn normalize_provider(env: &mut BTreeMap<String, String>) -> Result<(), AcpError> {
    let Some(raw) = env.get(PROVIDER_ENV) else {
        return Ok(());
    };
    let value = raw.trim().to_string();
    if value.is_empty() {
        env.remove(PROVIDER_ENV);
        return Ok(());
    }
    let valid = value.len() <= 64
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if !valid {
        return Err(AcpError::protocol(
            "DeepSeek provider must be a conservative route identifier",
        ));
    }
    env.insert(PROVIDER_ENV.to_string(), value);
    Ok(())
}

fn normalize_optional_value(env: &mut BTreeMap<String, String>, key: &str) {
    let Some(raw) = env.get(key) else {
        return;
    };
    let value = raw.trim().to_string();
    if value.is_empty() {
        env.remove(key);
    } else {
        env.insert(key.to_string(), value);
    }
}
