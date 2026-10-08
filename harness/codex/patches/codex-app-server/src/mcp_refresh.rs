use crate::config_manager::ConfigManager;
use codex_core::CodexThread;
use codex_core::ConfigRefreshOutcome;
use codex_core::ThreadManager;
use codex_core::config::Config;
use std::io;
use std::sync::Arc;
use tracing::warn;

pub(crate) async fn reload_mcp_config(
    thread_manager: &Arc<ThreadManager>,
    config_manager: &ConfigManager,
) -> io::Result<()> {
    // Keep the reload state machine out of the request dispatcher's stack frame.
    Box::pin(reload_mcp_config_with_policy(
        thread_manager,
        config_manager,
        ReloadPolicy::Strict,
    ))
    .await
}

pub(crate) async fn reload_mcp_config_best_effort(
    thread_manager: &Arc<ThreadManager>,
    config_manager: &ConfigManager,
) {
    if let Err(err) = Box::pin(reload_mcp_config_with_policy(
        thread_manager,
        config_manager,
        ReloadPolicy::BestEffort,
    ))
    .await
    {
        warn!(%err, "failed to reload MCP configuration");
    }
}

#[derive(Clone, Copy)]
enum ReloadPolicy {
    Strict,
    BestEffort,
}

async fn reload_mcp_config_with_policy(
    thread_manager: &ThreadManager,
    config_manager: &ConfigManager,
    policy: ReloadPolicy,
) -> io::Result<()> {
    let mut rejected = false;
    let mut load_error = None;
    let mut completed: Vec<Arc<CodexThread>> = Vec::new();
    // Bound both discovery and stale-owner retries so concurrent churn cannot
    // keep the reload request alive indefinitely. Completed publications remain valid.
    for pass in 0..3 {
        let host_loaded = match policy {
            ReloadPolicy::BestEffort => true,
            ReloadPolicy::Strict => match config_manager.load_config_layers(/*cwd*/ None).await {
                Ok(_) => true,
                Err(err) => {
                    load_error.get_or_insert(err);
                    false
                }
            },
        };
        let mut targets = Vec::new();
        for thread_id in thread_manager.list_thread_ids().await {
            match thread_manager.get_thread(thread_id).await {
                Ok(thread) => {
                    if !completed.iter().any(|done| Arc::ptr_eq(done, &thread)) {
                        targets.push((thread_id, thread));
                    }
                }
                Err(err) => {
                    warn!(%thread_id, %err, "failed to load thread for MCP configuration refresh");
                }
            }
        }
        if targets.is_empty() {
            break;
        }
        for (thread_id, target) in targets {
            for attempt in 0..3 {
                let current_config = target.config().await;
                let mut target_load_error = None;
                let next_config = if host_loaded {
                    match load_refresh_config(&current_config, config_manager).await {
                        Ok(next_config) => next_config,
                        Err(err) => {
                            if matches!(policy, ReloadPolicy::BestEffort) {
                                warn!(%thread_id, %err, "failed to load session configuration");
                            }
                            target_load_error = Some(err);
                            disabled_enterprise_config(&current_config)
                        }
                    }
                } else {
                    disabled_enterprise_config(&current_config)
                };
                match target.refresh_mcp_config(current_config, next_config).await {
                    ConfigRefreshOutcome::Published => {}
                    ConfigRefreshOutcome::Rejected => rejected = true,
                    ConfigRefreshOutcome::Stale => {
                        if attempt == 2 {
                            target.disable_mcp_enterprise_auth().await;
                            rejected = true;
                        }
                        continue;
                    }
                }
                if matches!(policy, ReloadPolicy::Strict)
                    && let Some(err) = target_load_error
                {
                    load_error.get_or_insert(err);
                }
                break;
            }
            completed.push(target);
        }
        if pass == 2 {
            for thread_id in thread_manager.list_thread_ids().await {
                if let Ok(thread) = thread_manager.get_thread(thread_id).await
                    && !completed.iter().any(|done| Arc::ptr_eq(done, &thread))
                {
                    thread.disable_mcp_enterprise_auth().await;
                    rejected = true;
                }
            }
        }
    }
    // Finish publishing every session's fail-closed configuration before reporting failure.
    if matches!(policy, ReloadPolicy::Strict)
        && let Some(err) = load_error
    {
        return Err(err);
    }
    if rejected && matches!(policy, ReloadPolicy::Strict) {
        return Err(io::Error::other(
            "enterprise MCP configuration was rejected; affected sessions retained their previous configuration with enterprise MCP disabled",
        ));
    }
    Ok(())
}

fn disabled_enterprise_config(current_config: &Config) -> Config {
    let mut config = current_config.clone();
    config.disable_mcp_enterprise_auth();
    config.config_layer_stack = config
        .config_layer_stack
        .with_cloud_config_binding(/*binding*/ None);
    config
}

async fn load_refresh_config(
    current_config: &Config,
    config_manager: &ConfigManager,
) -> io::Result<Config> {
    config_manager
        .load_latest_config_with_session_layers(
            &current_config.config_layer_stack,
            &current_config.cwd,
        )
        .await
}
