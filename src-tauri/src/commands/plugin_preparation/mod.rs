mod context;
pub mod account;
pub mod configuration;
pub mod mutation;
mod types;

pub use mutation::{change, check};
pub use types::{ChangeInput, ConnectionView, PreparationView, ReadInput, ConfigurationInput};

use crate::app_error::AppCommandError;
use crate::plugin_runtime::registry::{self, PluginDescriptor};
use sea_orm::DatabaseConnection;

pub(super) struct Prepared {
    plugin: PluginDescriptor,
    workspace: Option<context::Workspace>,
    view: PreparationView,
}

pub async fn read(
    conn: &DatabaseConnection,
    input: ReadInput,
) -> Result<PreparationView, AppCommandError> {
    let _storage = crate::acp::agent_storage_work::begin_agent_storage_read().await;
    let _operation = registry::lock_mutation().await;
    Ok(load(conn, &input).await?.view)
}

async fn load(conn: &DatabaseConnection, input: &ReadInput) -> Result<Prepared, AppCommandError> {
    crate::commands::mcp::require_private_agent_storage_for_write()?;
    let (plugin, snapshot) = load_plugin(conn, &input.market_skill_id).await?;
    let workspace = context::load(conn, input.conversation_id).await?;
    let dependencies = match &workspace {
        Some(value) => crate::plugin_runtime::dependency_scope::inspect(
            &plugin,
            &snapshot.plugins,
            (&value.key, value.agent.as_wire().as_ref()),
        ),
        None => crate::plugin_runtime::dependencies::inspect(&plugin, &snapshot.plugins),
    };
    let view = build_view(conn, (&plugin, workspace.as_ref()), dependencies).await?;
    Ok(Prepared {
        plugin,
        workspace,
        view,
    })
}

async fn load_plugin(
    conn: &DatabaseConnection,
    id: &str,
) -> Result<
    (
        PluginDescriptor,
        std::sync::Arc<registry::PluginRegistrySnapshot>,
    ),
    AppCommandError,
> {
    let id = id
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| AppCommandError::invalid_input("插件市场标识无效"))?;
    registry::reconcile_global(conn).await?;
    let snapshot = registry::global_snapshot()
        .ok_or_else(|| AppCommandError::configuration_missing("插件运行时未初始化"))?;
    let plugin = snapshot
        .plugins
        .values()
        .find(|plugin| plugin.market_skill_id == id)
        .cloned()
        .ok_or_else(|| {
            AppCommandError::not_found("插件未安装或安装记录需要修复，请重新安装对应版本")
        })?;
    Ok((plugin, snapshot))
}

async fn build_view(
    conn: &DatabaseConnection,
    input: (&PluginDescriptor, Option<&context::Workspace>),
    dependencies: crate::plugin_runtime::dependencies::DependencyReport,
) -> Result<PreparationView, AppCommandError> {
    let (plugin, workspace) = input;
    let issues = preparation_issues(plugin, workspace, &dependencies);
    let granted = workspace
        .is_some_and(|value| crate::plugin_runtime::policy::permission_granted(plugin, &value.key));
    let connectors = configuration::connectors(plugin, workspace)?;
    let state = preparation_state(plugin, (&issues, workspace.is_some(), granted), &connectors);
    let context_token = match workspace {
        Some(workspace) => Some(context::token(conn, plugin, workspace).await?),
        None => None,
    };
    Ok(PreparationView {
        slug: plugin.slug.clone(),
        installed_version: plugin.version.clone(),
        schema_version: plugin.manifest.schema_version,
        state: state.into(),
        issues,
        dependencies,
        permissions: plugin.manifest.permissions.clone(),
        permission_granted: granted,
        connectors,
        context_token,
        workspace_name: workspace.map(|value| value.name.clone()),
        agent_type: workspace.map(|value| value.agent),
    })
}

fn preparation_issues(
    plugin: &PluginDescriptor,
    workspace: Option<&context::Workspace>,
    dependencies: &crate::plugin_runtime::dependencies::DependencyReport,
) -> Vec<String> {
    let mut issues = dependencies.issues.clone();
    if !plugin.available {
        issues.push("plugin_unavailable".into());
    }
    if workspace
        .is_some_and(|value| !crate::acp::connection::agent_supports_builtin_mcp(value.agent))
    {
        issues.push("agent_unsupported".into());
    }
    issues
}

fn preparation_state(
    plugin: &PluginDescriptor,
    conditions: (&[String], bool, bool),
    connectors: &[types::ConnectorState],
) -> &'static str {
    if !conditions.0.is_empty() {
        return "blocked";
    }
    if plugin.manifest.schema_version < 2 {
        return "legacy";
    }
    if !conditions.1 {
        return "context_required";
    }
    if connectors.iter().any(|connector|!connector.configuration_ready) {return "configuration_required"}
    if !conditions.2 {
        return "permission_required";
    }
    if connectors.iter().any(|connector| !connector.enabled) {
        return "disabled";
    }
    "unchecked"
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn plugin_preparation_get(
    input: ReadInput,
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<PreparationView, AppCommandError> {
    read(&db.conn, input).await
}
