//! Managed Open Computer Use runtime and MCP registration.

use std::path::Path;
use std::sync::OnceLock;

use sea_orm::DatabaseConnection;
use serde_json::json;
use tokio::sync::Mutex;

use crate::acp::agent_storage::AgentStoragePaths;
use crate::app_error::AppCommandError;

mod install;

const MCP_SERVER_ID: &str = "open-computer-use";

fn operation_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn command_text(executable: &Path) -> Result<&str, AppCommandError> {
    executable.to_str().ok_or_else(|| {
        AppCommandError::configuration_invalid("Open Computer Use command path is not valid UTF-8")
    })
}

async fn enable_managed_mcp(
    conn: &DatabaseConnection,
    paths: &AgentStoragePaths,
    executable: &Path,
) -> Result<(), AppCommandError> {
    let command = command_text(executable)?;
    let _guard = super::mcp_catalog::lock_operation().await;
    let catalog =
        super::mcp_catalog::load_or_import_unlocked(conn, super::mcp::scan_legacy_server_specs)
            .await?;
    let spec = json!({ "type": "stdio", "command": command, "args": ["mcp"] });
    if catalog
        .servers
        .get(MCP_SERVER_ID)
        .is_some_and(|entry| !entry_is_owned(entry, paths))
    {
        return Err(AppCommandError::already_exists(
            "A different Open Computer Use MCP entry already exists",
        ));
    }

    super::mcp_catalog::upsert_server_unlocked(
        conn,
        MCP_SERVER_ID,
        spec,
        super::mcp::scan_legacy_server_specs,
    )
    .await?;
    super::mcp_catalog::set_server_enabled_unlocked(
        conn,
        MCP_SERVER_ID,
        true,
        super::mcp::scan_legacy_server_specs,
    )
    .await?;
    tracing::info!(
        command,
        version = install::PACKAGE_VERSION,
        "[computer-use] managed MCP enabled"
    );
    super::mcp_sync::reconcile_all_managed_mcp_unlocked(conn).await
}

fn entry_is_owned(
    entry: &super::mcp_catalog::ManagedMcpCatalogEntry,
    paths: &AgentStoragePaths,
) -> bool {
    let Some(spec) = entry.spec.as_object() else {
        return false;
    };
    let command_owned = spec
        .get("command")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|command| install::command_is_managed(paths, command));
    let has_mcp_arg = spec
        .get("args")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|args| args.len() == 1 && args[0].as_str() == Some("mcp"));
    entry.managed
        && spec.get("type").and_then(serde_json::Value::as_str) == Some("stdio")
        && command_owned
        && has_mcp_arg
}

async fn disable_managed_mcp(
    conn: &DatabaseConnection,
    paths: &AgentStoragePaths,
) -> Result<(), AppCommandError> {
    let _guard = super::mcp_catalog::lock_operation().await;
    let catalog =
        super::mcp_catalog::load_or_import_unlocked(conn, super::mcp::scan_legacy_server_specs)
            .await?;
    let owned = catalog
        .servers
        .get(MCP_SERVER_ID)
        .is_some_and(|entry| entry_is_owned(entry, paths));
    if !owned {
        tracing::info!("[computer-use] no owned MCP entry to disable");
        return Ok(());
    }
    super::mcp_catalog::set_server_enabled_unlocked(
        conn,
        MCP_SERVER_ID,
        false,
        super::mcp::scan_legacy_server_specs,
    )
    .await?;
    tracing::info!("[computer-use] managed MCP disabled");
    super::mcp_sync::reconcile_all_managed_mcp_unlocked(conn).await
}

async fn set_enabled_locked(
    conn: &DatabaseConnection,
    enabled: bool,
) -> Result<(), AppCommandError> {
    let paths = AgentStoragePaths::active().ok_or_else(|| {
        AppCommandError::agent_storage_not_initialized(
            "Agent storage is not initialized for Open Computer Use",
        )
    })?;
    if !enabled {
        return disable_managed_mcp(conn, &paths).await;
    }
    let (executable, installed) = install::ensure_private_package(&paths).await?;
    if installed {
        tracing::info!(
            version = install::PACKAGE_VERSION,
            command = %executable.display(),
            "[computer-use] private runtime installed"
        );
    }
    enable_managed_mcp(conn, &paths, &executable).await?;
    Ok(())
}

pub async fn set_enabled_core(
    conn: &DatabaseConnection,
    enabled: bool,
) -> Result<(), AppCommandError> {
    let _guard = operation_lock().lock().await;
    tracing::info!(enabled, "[computer-use] explicit state change started");
    let result = set_enabled_locked(conn, enabled).await;
    match &result {
        Ok(()) => tracing::info!(enabled, "[computer-use] explicit state change finished"),
        Err(error) => tracing::error!(
            enabled,
            error = %error.message,
            detail = ?error.detail,
            "[computer-use] explicit state change failed"
        ),
    }
    result
}
