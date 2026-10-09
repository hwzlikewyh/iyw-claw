use std::path::{Component, Path};

use sea_orm::DatabaseConnection;

use crate::acp::agent_storage::AgentStoragePaths;
use crate::app_error::AppCommandError;

const SERVER_ID: &str = "open-computer-use";

pub(crate) async fn cleanup(conn: &DatabaseConnection) -> Result<(), AppCommandError> {
    let Some(paths) = AgentStoragePaths::active() else {
        return Ok(());
    };
    let _guard = super::mcp_catalog::lock_operation().await;
    let catalog =
        super::mcp_catalog::load_or_import_unlocked(conn, super::mcp::scan_legacy_server_specs)
            .await?;
    let owned = catalog.servers.get(SERVER_ID).is_some_and(|entry| {
        entry.managed
            && entry.sources.is_empty()
            && entry.spec["type"] == "stdio"
            && entry.spec["args"]
                .as_array()
                .is_some_and(|args| args.len() == 1 && args[0] == "mcp")
            && entry.spec["command"]
                .as_str()
                .is_some_and(|command| command_owned(&paths, command))
    });
    if owned {
        super::mcp_catalog::remove_server_unlocked(
            conn,
            SERVER_ID,
            super::mcp::scan_legacy_server_specs,
        )
        .await?;
        super::mcp_sync::reconcile_all_managed_mcp_unlocked(conn).await?;
        tracing::info!("[computer] retired managed MCP removed");
    }
    if owned || !catalog.servers.contains_key(SERVER_ID) {
        cleanup_private_runtime(&paths)?;
    }
    Ok(())
}

fn cleanup_private_runtime(paths: &AgentStoragePaths) -> Result<(), AppCommandError> {
    let root = paths.npm_runtime_dir().join("tools").join(SERVER_ID);
    let metadata = match std::fs::symlink_metadata(&root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(AppCommandError::io(error)),
    };
    if metadata.is_dir() {
        std::fs::remove_dir_all(&root).map_err(AppCommandError::io)?;
    } else {
        std::fs::remove_file(&root).map_err(AppCommandError::io)?;
    }
    tracing::info!("[computer] retired private runtime removed");
    Ok(())
}

fn command_owned(paths: &AgentStoragePaths, command: &str) -> bool {
    let path = Path::new(command);
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return false;
    }
    let roots = [
        paths.npm_runtime_dir().join("tools").join(SERVER_ID),
        crate::paths::iyw_claw_user_dir()
            .join("runtime")
            .join(SERVER_ID),
    ];
    roots.iter().any(|root| path.starts_with(root))
}
