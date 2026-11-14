use super::plugin_app_context::{self, PluginAppWorkspace};
use crate::app_error::AppCommandError;
use crate::plugin_runtime::{policy, registry};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CatalogRequest {
    pub conversation_id: Option<i64>,
    #[serde(default)]
    pub include_plugins: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LaunchRequest {
    pub conversation_id: i64,
    pub plugin_slug: String,
    pub app_key: String,
    pub expected_version: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppChoice {
    pub plugin_id: String,
    pub plugin_slug: String,
    pub version: String,
    pub app_key: String,
    pub label: String,
    pub state: String,
}
const MAX_APPS: usize = 256;

pub async fn catalog_core(
    db: &crate::db::AppDatabase,
    request: CatalogRequest,
) -> Result<Vec<AppChoice>, AppCommandError> {
    let _storage = crate::acp::agent_storage_work::begin_agent_storage_read().await;
    let scope = crate::commands::mcp_cli_config::account_scope(&db.conn).await?;
    registry::reconcile_global(&db.conn).await?;
    let workspace = match request.conversation_id {
        Some(id) => Some(plugin_app_context::conversation_workspace(db, id).await?),
        None => None,
    };
    let snapshot = registry::global_snapshot().ok_or_else(|| invalid("插件目录未初始化"))?;
    let mut apps = collect_apps(&snapshot, workspace.as_ref())?;
    if request.include_plugins {
        for plugin in snapshot.plugins.values() {
            if apps.iter().any(|app| app.plugin_slug == plugin.slug) {
                continue;
            }
            apps.push(AppChoice {
                plugin_id: plugin.market_skill_id.to_string(),
                plugin_slug: plugin.slug.clone(),
                version: plugin.version.clone(),
                app_key: String::new(),
                label: plugin.manifest.name.clone(),
                state: if plugin.available {
                    "context_required"
                } else {
                    "unavailable"
                }
                .into(),
            });
        }
    }
    if crate::commands::mcp_cli_config::account_scope(&db.conn).await? != scope {
        return Err(AppCommandError::permission_denied(
            "账号范围已变化，请刷新应用列表",
        ));
    }
    Ok(apps)
}

fn collect_apps(
    snapshot: &registry::PluginRegistrySnapshot,
    workspace: Option<&PluginAppWorkspace>,
) -> Result<Vec<AppChoice>, AppCommandError> {
    let mut apps = Vec::new();
    let mut seen = BTreeSet::new();
    for plugin in snapshot.plugins.values() {
        for app in plugin
            .manifest
            .components
            .iter()
            .filter(|component| component.kind == "app")
        {
            let Some(config) = app.config.as_ref() else {
                continue;
            };
            let Some(uri) = config["resourceUri"].as_str() else {
                continue;
            };
            if !seen.insert((
                plugin.slug.clone(),
                config["connectorKey"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                uri.to_string(),
            )) {
                continue;
            }
            if apps.len() >= MAX_APPS {
                return Err(invalid("已安装的插件页面过多，请管理本地安装后重试"));
            }
            apps.push(AppChoice {
                plugin_id: plugin.market_skill_id.to_string(),
                plugin_slug: plugin.slug.clone(),
                version: plugin.version.clone(),
                app_key: app.key.clone(),
                label: app_label(plugin, config),
                state: state(plugin, config, (workspace, &snapshot.plugins)).into(),
            });
        }
    }
    Ok(apps)
}

fn app_label(plugin: &registry::PluginDescriptor, config: &serde_json::Value) -> String {
    let label = config["title"].as_str().map(str::to_string).or_else(|| {
        let native = crate::plugin_runtime::native_compat::connector(
            std::path::Path::new(&plugin.install_root),
            config["connectorKey"].as_str()?,
        )
        .ok()
        .flatten()?;
        native
            .resources
            .iter()
            .find(|resource| resource["uri"] == config["resourceUri"])
            .and_then(|resource| resource["title"].as_str())
            .map(str::to_string)
    });
    label
        .filter(|label| !label.trim().is_empty() && label.len() <= 256)
        .unwrap_or_else(|| plugin.slug.clone())
}

pub(super) fn state(
    plugin: &registry::PluginDescriptor,
    config: &serde_json::Value,
    context: (
        Option<&PluginAppWorkspace>,
        &std::collections::BTreeMap<String, registry::PluginDescriptor>,
    ),
) -> &'static str {
    let (workspace, plugins) = context;
    if !plugin.available {
        return "unavailable";
    }
    let Some(workspace) = workspace else {
        return "context_required";
    };
    if !policy::permission_granted(plugin, &workspace.workspace_key) {
        return "permission_required";
    }
    if crate::plugin_runtime::dependency_scope::require(
        plugin,
        plugins,
        (
            &workspace.workspace_key,
            workspace.agent_type.as_wire().as_ref(),
        ),
    )
    .is_err()
    {
        return "dependency_required";
    }
    if !policy::activation_enabled(
        plugin,
        (
            config["connectorKey"].as_str().unwrap_or_default(),
            &workspace.workspace_key,
            workspace.agent_type.as_wire().as_ref(),
        ),
    ) {
        return "disabled";
    }
    "available"
}

fn invalid(message: &str) -> AppCommandError {
    AppCommandError::configuration_invalid(message)
}
