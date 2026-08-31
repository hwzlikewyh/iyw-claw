use std::fs;
use std::path::{Path, PathBuf};

use crate::acp::agent_storage::AgentStoragePaths;
use crate::models::agent::AgentType;

use super::provider_overlay::{
    model_gateway_base_url_for, patch_grok_toml, patch_hermes_yaml, patch_json_config,
    patch_kimi_toml, patch_pi_models_json,
};

pub fn enforce_all_provider_overlays(paths: &AgentStoragePaths) -> Result<(), String> {
    for agent in crate::acp::registry::all_acp_agents() {
        enforce_provider_overlay(agent, paths)?;
    }
    Ok(())
}

pub fn enforce_existing_provider_overlays(paths: &AgentStoragePaths) -> Result<(), String> {
    for agent in crate::acp::registry::all_acp_agents() {
        let profile = paths.profile(agent).root;
        if profile.exists() {
            enforce_provider_overlay_at_root(agent, &profile)?;
        }
    }
    Ok(())
}

pub fn enforce_active_provider_overlay(agent: AgentType) -> Result<(), String> {
    AgentStoragePaths::active().ok_or_else(|| "Agent storage is not initialized".to_string())?;
    enforce_provider_overlay_at_root(agent, &active_profile_root(agent)?)
}

/// 恢复会话（resume）的 provider overlay 门：与新建会话同一 reconciler，
/// 但走 `reconcile_resumed_session`（保持策略代际，只刷新热更新安全字段）。
/// 由 Task 13 在 `spawn_agent_connection` 按 `session_id` 区分调用。
pub fn enforce_resumed_active_provider_overlay(agent: AgentType) -> Result<(), String> {
    AgentStoragePaths::active().ok_or_else(|| "Agent storage is not initialized".to_string())?;
    enforce_provider_overlay_at_root_with_kind(agent, &active_profile_root(agent)?, true)
}

pub fn enforce_existing_active_provider_overlays() -> Result<(), String> {
    for agent in crate::acp::registry::all_acp_agents() {
        let profile = active_profile_root(agent)?;
        if profile.exists() {
            enforce_provider_overlay_at_root(agent, &profile)?;
        }
    }
    Ok(())
}

pub fn enforce_provider_overlay(agent: AgentType, paths: &AgentStoragePaths) -> Result<(), String> {
    enforce_provider_overlay_at_root(agent, &paths.profile(agent).root)
}

fn enforce_provider_overlay_at_root(agent: AgentType, profile: &Path) -> Result<(), String> {
    enforce_provider_overlay_at_root_with_kind(agent, profile, false)
}

fn enforce_provider_overlay_at_root_with_kind(
    agent: AgentType,
    profile: &Path,
    resumed: bool,
) -> Result<(), String> {
    if !super::provider_overlay::uses_managed_gateway(agent) {
        // Gemini keeps its user-selected endpoint/authentication path, but its
        // native compression threshold is still safe for the host to manage.
        if agent == AgentType::Gemini {
            return patch_json(&profile.join("settings.json"), |value| {
                patch_json_config(agent, value, "")
            });
        }
        return Ok(());
    }
    if crate::acp::model_catalog::has_authoritative_empty_catalog(agent) {
        return Err(format!(
            "no compatible gateway model is configured for {}",
            crate::acp::registry::registry_id_for(agent)
        ));
    }
    // Codex / Claude Code 走统一 reconciler：受控字段幂等写入 + 回读校验 +
    // fingerprint + 诊断；失败会阻止新会话 spawn（不得以未知配置启动）。
    if matches!(agent, AgentType::Codex | AgentType::ClaudeCode) {
        let outcome = if resumed {
            super::session_config_reconciler::reconcile_resumed_session(agent, profile)
        } else {
            super::session_config_reconciler::reconcile_before_spawn(agent, profile)
        };
        return outcome
            .map(|_| ())
            .map_err(|error| format!("session config reconcile failed: {error}"));
    }
    let base_url = model_gateway_base_url_for(agent);
    match agent {
        AgentType::KimiCode => patch_text(&profile.join("config.toml"), |raw| {
            patch_kimi_toml(raw, &base_url)
        }),
        AgentType::Hermes => {
            patch_text(&profile.join("config.yaml"), |raw| {
                patch_hermes_yaml(raw, &base_url)
            })?;
            patch_text(&profile.join(".env"), |raw| {
                Ok(patch_env_value(raw, "OPENAI_BASE_URL", &base_url))
            })
        }
        AgentType::Pi => {
            let mut model = None;
            patch_json(&profile.join("settings.json"), |value| {
                model = value
                    .get("defaultModel")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                patch_json_config(agent, value, &base_url)
            })?;
            patch_json(&profile.join("models.json"), |value| {
                patch_pi_models_json(value, &base_url, model.as_deref())
            })
        }
        AgentType::OpenCode => patch_json(
            &profile
                .join("config")
                .join("opencode")
                .join("opencode.json"),
            |value| patch_json_config(agent, value, &base_url),
        ),
        AgentType::Cline => patch_json(&profile.join("globalState.json"), |value| {
            patch_json_config(agent, value, &base_url)
        }),
        AgentType::Gemini => patch_json(&profile.join("settings.json"), |value| {
            patch_json_config(agent, value, &base_url)
        }),
        AgentType::OpenClaw => patch_json(&profile.join("openclaw.json"), |value| {
            patch_json_config(agent, value, &base_url)
        }),
        AgentType::CodeBuddy => patch_json(&profile.join("settings.json"), |value| {
            patch_json_config(agent, value, &base_url)
        }),
        AgentType::Grok => patch_text(&profile.join("config.toml"), |raw| {
            patch_grok_toml(raw, &base_url)
        }),
        AgentType::Codex | AgentType::ClaudeCode => {
            unreachable!("Codex and ClaudeCode are handled by session_config_reconciler above")
        }
        AgentType::Cursor | AgentType::DeepSeek | AgentType::Custom(_) => Ok(()),
    }
}

pub(crate) fn active_profile_root(agent: AgentType) -> Result<PathBuf, String> {
    match agent {
        AgentType::ClaudeCode => required_env_path("CLAUDE_CONFIG_DIR"),
        AgentType::Codex => required_env_path("CODEX_HOME"),
        AgentType::Gemini => Ok(required_env_path("GEMINI_CLI_HOME")?.join(".gemini")),
        AgentType::OpenClaw => required_env_path("OPENCLAW_STATE_DIR"),
        AgentType::OpenCode => required_env_path("XDG_CONFIG_HOME")?
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| "XDG_CONFIG_HOME has no parent directory".to_string()),
        AgentType::Cline => required_env_path("CLINE_DIR"),
        AgentType::Hermes => required_env_path("HERMES_HOME"),
        AgentType::CodeBuddy => required_env_path("CODEBUDDY_CONFIG_DIR"),
        AgentType::KimiCode => required_env_path("KIMI_CODE_HOME"),
        AgentType::Pi => required_env_path("PI_CODING_AGENT_DIR"),
        AgentType::Grok => required_env_path("GROK_HOME"),
        AgentType::DeepSeek => required_env_path("DSH_HOME"),
        AgentType::Cursor | AgentType::Custom(_) => AgentStoragePaths::active()
            .map(|paths| paths.profile(agent).root)
            .ok_or_else(|| "Agent storage is not initialized".to_string()),
    }
}

fn required_env_path(key: &str) -> Result<PathBuf, String> {
    std::env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| format!("private Agent profile environment is missing {key}"))
}

fn patch_env_value(raw: &str, key: &str, value: &str) -> String {
    let mut output = Vec::new();
    let mut replaced = false;
    for line in raw.lines() {
        let candidate = line
            .trim_start()
            .strip_prefix("export ")
            .unwrap_or(line.trim_start());
        let matches_key = candidate
            .split_once('=')
            .is_some_and(|(name, _)| name.trim() == key);
        if matches_key {
            if !replaced {
                output.push(format!("{key}={value}"));
                replaced = true;
            }
        } else {
            output.push(line.to_string());
        }
    }
    if !replaced {
        output.push(format!("{key}={value}"));
    }
    output.join("\n") + "\n"
}

fn patch_json(
    path: &Path,
    patch: impl FnOnce(serde_json::Value) -> Result<serde_json::Value, String>,
) -> Result<(), String> {
    let raw = read_optional(path)?;
    let value = if raw.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(&raw).map_err(|error| file_error(path, error))?
    };
    let next = patch(value)?;
    let serialized =
        serde_json::to_string_pretty(&next).map_err(|error| file_error(path, error))?;
    write_if_changed(path, &raw, &(serialized + "\n"))
}

fn patch_text(
    path: &Path,
    patch: impl FnOnce(&str) -> Result<String, String>,
) -> Result<(), String> {
    let raw = read_optional(path)?;
    let next = patch(&raw).map_err(|error| format!("{}: {error}", path.display()))?;
    write_if_changed(path, &raw, &next)
}

pub(crate) fn read_optional(path: &Path) -> Result<String, String> {
    match fs::read_to_string(path) {
        Ok(raw) => Ok(raw),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(file_error(path, error)),
    }
}

pub(crate) fn write_if_changed(path: &Path, old: &str, next: &str) -> Result<(), String> {
    if old == next {
        return Ok(());
    }
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent", path.display()))?;
    fs::create_dir_all(parent).map_err(|error| file_error(parent, error))?;
    let temp = temporary_path(path);
    let permissions = fs::metadata(path).ok().map(|value| value.permissions());
    let result = (|| {
        use std::io::Write;

        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)
            .map_err(|error| file_error(&temp, error))?;
        file.write_all(next.as_bytes())
            .map_err(|error| file_error(&temp, error))?;
        file.sync_all().map_err(|error| file_error(&temp, error))?;
        if let Some(permissions) = permissions {
            fs::set_permissions(&temp, permissions).map_err(|error| file_error(&temp, error))?;
        }
        replace_file(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn temporary_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("config");
    path.with_file_name(format!(
        ".{name}.iyw-claw.{}.{}.tmp",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    ))
}

#[cfg(unix)]
fn replace_file(temp: &Path, target: &Path) -> Result<(), String> {
    fs::rename(temp, target).map_err(|error| file_error(target, error))
}

#[cfg(target_os = "windows")]
fn replace_file(temp: &Path, target: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>()
    };
    let source = wide(temp);
    let destination = wide(target);
    let ok = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        return Err(file_error(target, std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(not(any(unix, target_os = "windows")))]
fn replace_file(temp: &Path, target: &Path) -> Result<(), String> {
    fs::rename(temp, target).map_err(|error| file_error(target, error))
}

fn file_error(path: &Path, error: impl std::fmt::Display) -> String {
    format!("provider overlay failed at {}: {error}", path.display())
}
