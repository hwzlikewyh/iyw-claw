use std::path::{Path, PathBuf};

use sea_orm::DatabaseConnection;

use crate::app_error::{AppCommandError, AppErrorCode};
use crate::models::agent::AgentType;

use super::account_credentials_formats::{
    patch_codex_auth_json, patch_json_credential, patch_json_gateway_header, patch_toml_credential,
    patch_yaml_credential,
};
use super::provider_overlay_files::{active_profile_root, read_optional, write_if_changed};

pub(crate) struct AccountAccessToken(String);

impl AccountAccessToken {
    pub(crate) fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(Self(trimmed.to_string()))
        }
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

pub(crate) async fn sync_agent_credentials(
    conn: &DatabaseConnection,
    agent: AgentType,
) -> Result<(), AppCommandError> {
    let token = require_access_token(conn).await?;
    let profile = active_profile_root(agent).map_err(profile_resolution_error)?;
    write_agent_credentials_at_profile(agent, &profile, Some(&token))
        .map_err(credential_write_error)
}

pub(crate) async fn sync_agent_credentials_for_acp(
    conn: &DatabaseConnection,
    agent: AgentType,
) -> Result<(), crate::acp::error::AcpError> {
    sync_agent_credentials(conn, agent)
        .await
        .map_err(|error| match error.code {
            AppErrorCode::AuthenticationFailed => {
                crate::acp::error::AcpError::AuthenticationRequired
            }
            _ => crate::acp::error::AcpError::protocol(error.to_string()),
        })
}

pub(crate) async fn sync_existing_agent_credentials(
    conn: &DatabaseConnection,
) -> Result<(), AppCommandError> {
    let token = require_access_token(conn).await?;
    for agent in crate::acp::registry::all_acp_agents() {
        let profile = active_profile_root(agent).map_err(profile_resolution_error)?;
        if profile.exists() {
            write_agent_credentials_at_profile(agent, &profile, Some(&token))
                .map_err(credential_write_error)?;
        }
    }
    Ok(())
}

pub(crate) fn clear_existing_agent_credentials() -> Result<(), String> {
    let mut errors = Vec::new();
    for agent in crate::acp::registry::all_acp_agents() {
        match active_profile_root(agent) {
            Ok(profile) if profile.exists() => {
                if let Err(error) = write_agent_credentials_at_profile(agent, &profile, None) {
                    errors.push(error);
                }
            }
            Ok(_) => {}
            Err(error) => errors.push(error),
        }
    }
    combine_errors(errors)
}

async fn require_access_token(
    conn: &DatabaseConnection,
) -> Result<AccountAccessToken, AppCommandError> {
    crate::commands::iyw_account::iyw_account_access_token_core(conn)
        .await?
        .ok_or_else(|| {
            AppCommandError::authentication_failed(
                "Sign in to iyw-claw before installing or using Agents",
            )
        })
}

pub(crate) fn write_agent_credentials_at_profile(
    agent: AgentType,
    profile: &Path,
    token: Option<&AccountAccessToken>,
) -> Result<(), String> {
    let token = token.map(AccountAccessToken::expose);
    if token.is_none() && !profile.exists() {
        return Ok(());
    }
    match agent {
        AgentType::ClaudeCode | AgentType::CodeBuddy | AgentType::Gemini => {
            patch_file(profile.join("settings.json"), token, |raw| {
                patch_json_credential(agent, raw, token)
            })
        }
        AgentType::Codex => {
            patch_file(profile.join("config.toml"), token, |raw| {
                patch_toml_credential(agent, raw, token)
            })?;
            patch_file(profile.join("auth.json"), token, |raw| {
                patch_codex_auth_json(raw, token)
            })
        }
        AgentType::OpenClaw => patch_file(profile.join("openclaw.json"), token, |raw| {
            patch_json_credential(agent, raw, token)
        }),
        AgentType::OpenCode => {
            patch_file(
                profile.join("data").join("opencode").join("auth.json"),
                token,
                |raw| patch_json_credential(agent, raw, token),
            )?;
            patch_file(
                profile
                    .join("config")
                    .join("opencode")
                    .join("opencode.json"),
                token,
                |raw| patch_json_gateway_header(agent, raw, token),
            )
        }
        AgentType::Cline => {
            patch_file(profile.join("secrets.json"), token, |raw| {
                patch_json_credential(agent, raw, token)
            })?;
            patch_file(profile.join("globalState.json"), token, |raw| {
                patch_json_gateway_header(agent, raw, token)
            })
        }
        AgentType::Hermes => patch_file(profile.join("config.yaml"), token, |raw| {
            patch_yaml_credential(raw, token)
        }),
        AgentType::KimiCode => patch_file(profile.join("config.toml"), token, |raw| {
            patch_toml_credential(agent, raw, token)
        }),
        AgentType::Pi => {
            patch_file(profile.join("auth.json"), token, |raw| {
                patch_json_credential(agent, raw, token)
            })?;
            patch_file(profile.join("models.json"), token, |raw| {
                patch_json_gateway_header(agent, raw, token)
            })
        }
    }
}

fn profile_resolution_error(error: String) -> AppCommandError {
    AppCommandError::agent_storage_not_initialized(
        "Private Agent profile storage is not initialized",
    )
    .with_detail(error)
}

fn credential_write_error(error: String) -> AppCommandError {
    AppCommandError::configuration_invalid("Failed to update private Agent credentials")
        .with_detail(error)
}

fn combine_errors(errors: Vec<String>) -> Result<(), String> {
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn patch_file(
    path: PathBuf,
    token: Option<&str>,
    patch: impl FnOnce(&str) -> Result<String, String>,
) -> Result<(), String> {
    if token.is_none() && !path.exists() {
        return Ok(());
    }
    let raw = read_optional(&path)?;
    let next = patch(&raw).map_err(|error| format!("{}: {error}", path.display()))?;
    write_if_changed(&path, &raw, &next)
}
