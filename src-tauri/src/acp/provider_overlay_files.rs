use std::fs;
use std::path::{Path, PathBuf};

use crate::acp::agent_storage::AgentStoragePaths;
use crate::models::agent::AgentType;

use super::provider_overlay::{
    patch_codex_toml, patch_hermes_yaml, patch_json_config, patch_kimi_toml, patch_pi_models_json,
    MODEL_GATEWAY_BASE_URL, MODEL_GATEWAY_BASE_URL_ENV,
};

pub fn enforce_all_provider_overlays(paths: &AgentStoragePaths) -> Result<(), String> {
    for agent in crate::acp::registry::all_acp_agents() {
        enforce_provider_overlay(agent, paths)?;
    }
    Ok(())
}

pub fn enforce_active_provider_overlay(agent: AgentType) -> Result<(), String> {
    AgentStoragePaths::active().ok_or_else(|| "Agent storage is not initialized".to_string())?;
    enforce_provider_overlay_at_root(agent, &active_profile_root(agent)?)
}

pub fn enforce_all_active_provider_overlays() -> Result<(), String> {
    for agent in crate::acp::registry::all_acp_agents() {
        enforce_active_provider_overlay(agent)?;
    }
    Ok(())
}

pub fn enforce_provider_overlay(agent: AgentType, paths: &AgentStoragePaths) -> Result<(), String> {
    enforce_provider_overlay_at_root(agent, &paths.profile(agent).root)
}

fn enforce_provider_overlay_at_root(agent: AgentType, profile: &Path) -> Result<(), String> {
    let base_url = gateway_base_url();
    match agent {
        AgentType::Codex => patch_text(&profile.join("config.toml"), |raw| {
            patch_codex_toml(raw, &base_url)
        }),
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
        AgentType::ClaudeCode | AgentType::CodeBuddy => {
            patch_json(&profile.join("settings.json"), |value| {
                patch_json_config(agent, value, &base_url)
            })
        }
    }
}

fn active_profile_root(agent: AgentType) -> Result<PathBuf, String> {
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

fn gateway_base_url() -> String {
    std::env::var(MODEL_GATEWAY_BASE_URL_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| MODEL_GATEWAY_BASE_URL.to_string())
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

fn read_optional(path: &Path) -> Result<String, String> {
    match fs::read_to_string(path) {
        Ok(raw) => Ok(raw),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(file_error(path, error)),
    }
}

fn write_if_changed(path: &Path, old: &str, next: &str) -> Result<(), String> {
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
