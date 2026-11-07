use super::plugin_app_catalog::{state, LaunchRequest};
use super::plugin_app_context::{self, PluginAppWorkspace};
use super::plugin_app_resources::AppConfig;
use super::plugin_app_types::{PluginAppOpenRequest, PluginAppOpenResponse};
use crate::app_error::AppCommandError;
use crate::plugin_runtime::{app_host::PluginAppRegistry, registry};

const MAX_PANEL_INSTANCES: usize = 8;

pub async fn launch_core(
    db: &crate::db::AppDatabase,
    host: (
        &crate::plugin_runtime::app_host::PluginAppRegistry,
        &crate::plugin_runtime::router::PluginRouter,
    ),
    request: LaunchRequest,
) -> Result<PluginAppOpenResponse, AppCommandError> {
    let (apps, router) = host;
    tracing::info!(conversation_id = request.conversation_id, plugin = %request.plugin_slug, app = %request.app_key, "Opening plugin sidebar app");
    let _storage = crate::acp::agent_storage_work::begin_agent_storage_read().await;
    let _operation = registry::lock_mutation().await;
    let scope = crate::commands::mcp_cli_config::account_scope(&db.conn).await?;
    registry::reconcile_global(&db.conn).await?;
    let prepared = prepare_launch(db, &request).await?;
    check_capacity(db, apps, request.conversation_id).await?;
    let launch = create_launch(db, apps, (&request, prepared)).await?;
    let result = super::plugin_app_open::open_core(
        db,
        apps,
        router,
        PluginAppOpenRequest {
            instance_id: launch.instance_id.clone(),
            conversation_id: request.conversation_id,
            display_mode: None,
        },
    )
    .await;
    finish_open(db, apps, (&request, &launch.instance_id, &scope, result)).await
}

struct PreparedLaunch {
    workspace: PluginAppWorkspace,
    plugin: registry::PluginDescriptor,
    config: AppConfig,
}

async fn prepare_launch(
    db: &crate::db::AppDatabase,
    request: &LaunchRequest,
) -> Result<PreparedLaunch, AppCommandError> {
    let workspace = plugin_app_context::conversation_workspace(db, request.conversation_id).await?;
    let snapshot = registry::global_snapshot().ok_or_else(|| invalid("插件目录未初始化"))?;
    let plugin = snapshot
        .plugins
        .get(&request.plugin_slug)
        .filter(|plugin| plugin.version == request.expected_version)
        .ok_or_else(|| invalid("插件安装已变化，请刷新应用列表"))?;
    let app = plugin
        .manifest
        .components
        .iter()
        .find(|app| app.kind == "app" && app.key == request.app_key)
        .and_then(|app| app.config.as_ref())
        .ok_or_else(|| invalid("插件页面未声明"))?;
    if state(plugin, app, (Some(&workspace), &snapshot.plugins)) != "available" {
        return Err(AppCommandError::permission_denied(
            "请先完成当前项目的插件授权、启用及依赖准备",
        ));
    }
    let config = super::plugin_app_resources::plugin_app_config(plugin, &request.app_key)?;
    Ok(PreparedLaunch {
        workspace,
        plugin: plugin.clone(),
        config,
    })
}

async fn check_capacity(
    db: &crate::db::AppDatabase,
    apps: &PluginAppRegistry,
    conversation_id: i64,
) -> Result<(), AppCommandError> {
    let existing = crate::db::service::plugin_app_instance_service::list_for_conversation(
        &db.conn,
        conversation_id,
    )
    .await
    .map_err(AppCommandError::db)?;
    if existing
        .iter()
        .filter(|instance| {
            instance.state == "active"
                && instance.tool_call_id.starts_with("panel:")
                && apps.has_live_lease(&instance.instance_id)
        })
        .count()
        >= MAX_PANEL_INSTANCES
    {
        return Err(invalid("当前对话已打开8个插件页面，请先关闭不用的页面"));
    }
    Ok(())
}

async fn create_launch(
    db: &crate::db::AppDatabase,
    apps: &PluginAppRegistry,
    context: (&LaunchRequest, PreparedLaunch),
) -> Result<crate::plugin_runtime::app_host::PluginAppLaunch, AppCommandError> {
    let (request, prepared) = context;
    let mode = prepared
        .config
        .display_modes
        .first()
        .cloned()
        .ok_or_else(|| invalid("插件页面没有可用显示方式"))?;
    let launch = apps
        .create_persisted(
            &db.conn,
            crate::plugin_runtime::app_host::PluginAppLaunchInput {
                conversation_id: request.conversation_id,
                tool_call_id: format!("panel:{}", uuid::Uuid::new_v4()),
                plugin_slug: prepared.plugin.slug.clone(),
                plugin_version: prepared.plugin.version.clone(),
                app_key: request.app_key.clone(),
                resource_uri: prepared.config.resource_uri,
                display_mode: mode.clone(),
                workspace_key: prepared.workspace.workspace_key,
                launch_payload: serde_json::json!({"permissionRevision":prepared.plugin.permissions_digest,"displayMode":mode}),
            },
        )
        .await
        .map_err(|error| invalid(&error.message))?;
    Ok(launch)
}

async fn finish_open(
    db: &crate::db::AppDatabase,
    apps: &PluginAppRegistry,
    context: (
        &LaunchRequest,
        &str,
        &str,
        Result<PluginAppOpenResponse, AppCommandError>,
    ),
) -> Result<PluginAppOpenResponse, AppCommandError> {
    let (request, instance_id, scope, result) = context;
    if result.is_err()
        || !crate::commands::mcp_cli_config::account_scope(&db.conn)
            .await
            .is_ok_and(|current| current == scope)
    {
        tracing::warn!(conversation_id = request.conversation_id, plugin = %request.plugin_slug, resource_open_failed = result.is_err(), "Plugin sidebar app launch aborted");
        if let Err(error) = super::teardown_core(
            db,
            apps,
            super::PluginAppTeardownRequest {
                instance_id: instance_id.to_string(),
                conversation_id: Some(request.conversation_id),
            },
        )
        .await
        {
            tracing::error!(conversation_id = request.conversation_id, plugin = %request.plugin_slug, error = %error, "Plugin sidebar app cleanup failed");
        }
        return result.and_then(|_| {
            Err(AppCommandError::permission_denied(
                "账号范围已变化，请重新打开插件",
            ))
        });
    }
    tracing::info!(conversation_id = request.conversation_id, plugin = %request.plugin_slug, "Plugin sidebar app opened");
    result
}
fn invalid(message: &str) -> AppCommandError {
    AppCommandError::configuration_invalid(message)
}
