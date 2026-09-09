use crate::config_manager::ConfigManager;
use codex_core::CodexThread;
use codex_core::ThreadManager;
use codex_core::config::Config;
use std::io;
use std::sync::Arc;
use tracing::warn;

pub(crate) async fn reload_mcp_config(
    thread_manager: &Arc<ThreadManager>,
    config_manager: &ConfigManager,
) -> io::Result<()> {
    config_manager
        .load_latest_config(/*fallback_cwd*/ None)
        .await?;
    let mut refreshes = Vec::new();
    for thread_id in thread_manager.list_thread_ids().await {
        let thread = thread_manager
            .get_thread(thread_id)
            .await
            .map_err(|err| io::Error::other(format!("failed to load thread {thread_id}: {err}")))?;
        let config = load_refresh_config(thread.as_ref(), config_manager).await?;
        refreshes.push((thread, config));
    }
    for (thread, config) in refreshes {
        thread.refresh_mcp_config(config).await;
    }
    Ok(())
}

pub(crate) async fn reload_mcp_config_best_effort(
    thread_manager: &Arc<ThreadManager>,
    config_manager: &ConfigManager,
) {
    for thread_id in thread_manager.list_thread_ids().await {
        let thread = match thread_manager.get_thread(thread_id).await {
            Ok(thread) => thread,
            Err(err) => {
                warn!(%thread_id, %err, "failed to load thread for MCP configuration refresh");
                continue;
            }
        };
        let config = match load_refresh_config(thread.as_ref(), config_manager).await {
            Ok(config) => config,
            Err(err) => {
                warn!(%thread_id, %err, "failed to load thread MCP configuration");
                continue;
            }
        };
        thread.refresh_mcp_config(config).await;
    }
}

async fn load_refresh_config(
    thread: &CodexThread,
    config_manager: &ConfigManager,
) -> io::Result<Config> {
    let thread_config = thread.config().await;
    config_manager
        .load_latest_config_for_thread(thread_config.as_ref())
        .await
}
