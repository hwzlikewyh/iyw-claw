use std::path::Path;
use std::time::Duration;

use sea_orm::DatabaseConnection;

use super::{
    acp_error, log_component_error, version_at_least, RuntimeSeedImport, SEED_ARTIFACT_PREFIX,
    SEED_POLICY,
};
use crate::acp::agent_storage::AgentStoragePaths;
use crate::acp::npm_runtime;
use crate::acp::registry;
use crate::acp::version_center::inventory::{self, ReadyAgentInstallation};
use crate::app_error::AppCommandError;
use crate::db::service::agent_setting_service;
use crate::models::agent::AgentType;

use super::super::runtime::active_tool_is_healthy;
use super::super::runtime_seed_files::stage_component;
use super::super::runtime_seed_manifest::{RuntimeSeedComponent, RuntimeSeedManifest};

const COMPONENT: &str = "codex-acp";
const COMMAND: &str = "codex-acp";
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

pub(super) async fn import(
    request: &RuntimeSeedImport<'_>,
    seed_root: &Path,
    manifest: &RuntimeSeedManifest,
) {
    let Some(component) = manifest.component(COMPONENT) else {
        return;
    };
    if let Err(error) = import_inner(request, seed_root, component).await {
        log_component_error(COMPONENT, "import", &error);
    }
}

async fn import_inner(
    request: &RuntimeSeedImport<'_>,
    seed_root: &Path,
    component: &RuntimeSeedComponent,
) -> Result<(), AppCommandError> {
    validate_version(component)?;
    ensure_managed_node(request).await?;
    let paths = AgentStoragePaths::active().ok_or_else(|| {
        AppCommandError::configuration_invalid("Agent storage is not initialized")
    })?;
    if valid_active(request.conn, &paths, &component.version).await {
        tracing::info!(seed_version = %component.version, "[runtime-seed] keeping active Codex ACP");
        return Ok(());
    }
    let staging = npm_runtime::private_npm_staging_prefix(&paths, AgentType::Codex);
    if let Err(error) = stage(seed_root, component, &staging).await {
        let _ = tokio::fs::remove_dir_all(&staging).await;
        return Err(error);
    }
    if let Err(error) = verify_prefix(&staging, &component.version) {
        let _ = tokio::fs::remove_dir_all(&staging).await;
        return Err(error);
    }
    let final_prefix = npm_runtime::activate_private_npm_runtime(
        &paths,
        AgentType::Codex,
        &component.version,
        &staging,
        &[COMMAND],
    )
    .map_err(acp_error)?;
    if let Err(error) = probe(&paths, &component.version).await {
        let _ = std::fs::remove_dir_all(final_prefix);
        return Err(error);
    }
    record(request.conn, component).await?;
    tracing::info!(version = %component.version, "[runtime-seed] Codex ACP imported and activated");
    Ok(())
}

async fn ensure_managed_node(request: &RuntimeSeedImport<'_>) -> Result<(), AppCommandError> {
    let required =
        crate::acp::trusted_agents::minimum_node_version(AgentType::Codex).ok_or_else(|| {
            AppCommandError::configuration_invalid("Codex Node requirement is missing")
        })?;
    let required = semver::Version::parse(required)
        .map_err(|_| AppCommandError::configuration_invalid("Codex Node requirement is invalid"))?;
    let active = inventory::list_tool_settings(request.conn)
        .await
        .map_err(acp_error)?
        .into_iter()
        .find(|setting| setting.tool_id == "node")
        .and_then(|setting| setting.active_version)
        .ok_or_else(|| AppCommandError::invalid_input("Managed Node is unavailable for Codex"))?;
    let installed = semver::Version::parse(&active)
        .map_err(|_| AppCommandError::invalid_input("Managed Node version is invalid"))?;
    if installed < required || !active_tool_is_healthy(request.data_dir, "node", &active).await {
        return Err(AppCommandError::invalid_input(
            "Managed Node does not satisfy the Codex runtime seed",
        ));
    }
    Ok(())
}

async fn stage(
    seed_root: &Path,
    component: &RuntimeSeedComponent,
    staging: &Path,
) -> Result<(), AppCommandError> {
    let seed_root = seed_root.to_path_buf();
    let component = component.clone();
    let staging = staging.to_path_buf();
    tokio::task::spawn_blocking(move || stage_component(&seed_root, &component, &staging))
        .await
        .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))?
}

fn validate_version(component: &RuntimeSeedComponent) -> Result<(), AppCommandError> {
    let expected = registry::get_agent_meta(AgentType::Codex)
        .registry_version()
        .unwrap_or_default();
    (component.version == expected)
        .then_some(())
        .ok_or_else(|| AppCommandError::invalid_input("Runtime seed Codex version is unsupported"))
}

fn verify_prefix(prefix: &Path, version: &str) -> Result<(), AppCommandError> {
    let manifest = prefix
        .join("node_modules")
        .join("@agentclientprotocol")
        .join("codex-acp")
        .join("package.json");
    let raw = std::fs::read(&manifest).map_err(AppCommandError::io)?;
    let package: serde_json::Value = serde_json::from_slice(&raw).map_err(|error| {
        AppCommandError::invalid_input("Runtime seed Codex package manifest is invalid")
            .with_detail(error.to_string())
    })?;
    let valid = package.get("name").and_then(serde_json::Value::as_str)
        == Some("@agentclientprotocol/codex-acp")
        && package.get("version").and_then(serde_json::Value::as_str) == Some(version);
    if !valid {
        return Err(AppCommandError::invalid_input(
            "Runtime seed Codex package identity is invalid",
        ));
    }
    npm_runtime::verify_host_platform_optional_deps(prefix).map_err(acp_error)
}

async fn probe(paths: &AgentStoragePaths, version: &str) -> Result<(), AppCommandError> {
    let command =
        npm_runtime::resolve_private_npm_command(paths, AgentType::Codex, version, COMMAND)
            .ok_or_else(|| {
                AppCommandError::invalid_input("Runtime seed Codex command is missing")
            })?;
    let output = tokio::time::timeout(PROBE_TIMEOUT, version_output(&command))
        .await
        .map_err(|_| {
            AppCommandError::task_execution_failed("Runtime seed Codex probe timed out")
        })??;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    (output.status.success() && (stdout.contains(version) || stderr.contains(version)))
        .then_some(())
        .ok_or_else(|| AppCommandError::invalid_input("Runtime seed Codex probe failed"))
}

#[cfg(not(windows))]
async fn version_output(command: &Path) -> Result<std::process::Output, AppCommandError> {
    crate::process::tokio_command(command)
        .arg("--version")
        .output()
        .await
        .map_err(AppCommandError::io)
}

#[cfg(windows)]
async fn version_output(command: &Path) -> Result<std::process::Output, AppCommandError> {
    let shell = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string());
    crate::process::tokio_command(shell)
        .args(["/D", "/S", "/C", "\"%IYW_CLAW_SEED_COMMAND%\" --version"])
        .env("IYW_CLAW_SEED_COMMAND", command)
        .output()
        .await
        .map_err(AppCommandError::io)
}

async fn valid_active(conn: &DatabaseConnection, paths: &AgentStoragePaths, seed: &str) -> bool {
    let Ok(setting) = agent_setting_service::get_by_agent_type(conn, AgentType::Codex).await else {
        return false;
    };
    let active = setting.and_then(|item| item.installed_version);
    version_at_least(active.as_deref(), seed)
        && active.as_deref().is_some_and(|version| {
            npm_runtime::resolve_private_npm_command(paths, AgentType::Codex, version, COMMAND)
                .is_some()
        })
}

async fn record(
    conn: &DatabaseConnection,
    component: &RuntimeSeedComponent,
) -> Result<(), AppCommandError> {
    agent_setting_service::ensure_defaults(
        conn,
        &[agent_setting_service::AgentDefaultInput {
            agent_type: AgentType::Codex,
            registry_id: registry::registry_id_for(AgentType::Codex).to_string(),
            default_sort_order: 0,
        }],
    )
    .await
    .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))?;
    inventory::record_agent_ready(
        conn,
        ReadyAgentInstallation {
            agent_type: AgentType::Codex,
            registry_id: registry::registry_id_for(AgentType::Codex),
            version: &component.version,
            delivery_kind: "npm",
            artifact_id: Some(SEED_ARTIFACT_PREFIX),
            source_key: Some(SEED_ARTIFACT_PREFIX),
            expected_sha256: Some(&component.sha256),
        },
    )
    .await
    .map_err(acp_error)?;
    inventory::activate_agent(conn, AgentType::Codex, &component.version, SEED_POLICY, 0)
        .await
        .map_err(acp_error)
}
