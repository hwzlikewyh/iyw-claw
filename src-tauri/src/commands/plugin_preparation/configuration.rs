use super::types::{ConfigurationInput, ConnectorState, ParameterState};
use super::{context, load, PreparationView, ReadInput};
use crate::app_error::AppCommandError;
use crate::plugin_runtime::{global, registry, setup_store};
use sea_orm::DatabaseConnection;

pub(super) fn connectors(
    plugin: &registry::PluginDescriptor,
    workspace: Option<&context::Workspace>,
) -> Result<Vec<ConnectorState>, AppCommandError> {
    plugin
        .manifest
        .components
        .iter()
        .filter(|component| component.kind == "connector")
        .map(|component| connector(plugin, &component.key, workspace))
        .collect()
}

fn connector(
    plugin: &registry::PluginDescriptor,
    key: &str,
    workspace: Option<&context::Workspace>,
) -> Result<ConnectorState, AppCommandError> {
    let contract = setup_store::contract(plugin, key)?;
    let stored = match workspace {
        Some(workspace) if !contract.parameters.is_empty() => {
            setup_store::stored_values(plugin, (&workspace.key, key))
        }
        _ => Ok(std::collections::BTreeMap::new()),
    };
    let readable = stored.is_ok();
    let values = stored.unwrap_or_default();
    let ready =
        readable && crate::commands::skill_market::plugin_setup::values(&contract, &values).is_ok();
    let parameters = contract
        .parameters
        .into_iter()
        .map(|field| {
            let value = values
                .get(&field.key)
                .cloned()
                .or_else(|| field.default_value.clone());
            ParameterState {
                configured: value.as_ref().is_some_and(|value| !value.trim().is_empty()),
                value: if field.secret { None } else { value },
                declaration: field,
            }
        })
        .collect();
    let auth = contract.authentication;
    Ok(ConnectorState {
        key: key.into(),
        enabled: workspace.is_some_and(|workspace| {
            crate::plugin_runtime::policy::activation_enabled(
                plugin,
                (key, &workspace.key, workspace.agent.as_wire().as_ref()),
            )
        }),
        configuration_ready: ready,
        parameters,
        has_authentication: auth.is_some(),
        can_login: auth.as_ref().is_some_and(|auth| auth.login.is_some()),
        can_logout: auth.as_ref().is_some_and(|auth| auth.logout.is_some()),
    })
}

pub async fn save(
    conn: &DatabaseConnection,
    input: ConfigurationInput,
    emitter: crate::web::event_bridge::EventEmitter,
) -> Result<PreparationView, AppCommandError> {
    let _storage = crate::acp::agent_storage_work::begin_agent_storage_read().await;
    let _operation = registry::lock_mutation().await;
    let query = ReadInput {
        market_skill_id: input.market_skill_id,
        conversation_id: Some(input.conversation_id),
    };
    let prepared = load(conn, &query).await?;
    if prepared.view.context_token.as_deref() != Some(&input.context_token) {
        return Err(AppCommandError::invalid_input(
            "插件、配置或当前项目已变化，请重新填写",
        ));
    }
    let workspace = prepared
        .workspace
        .as_ref()
        .ok_or_else(|| AppCommandError::invalid_input("请选择当前对话项目"))?;
    setup_store::save(
        &prepared.plugin,
        (&workspace.key, &input.connector_key),
        (&input.values, &input.clear_keys),
    )?;
    super::mutation::stop_affected_workspaces(conn, (&prepared.plugin, &workspace.key), &emitter)
        .await;
    if global::supervisor().is_none() {
        return Err(AppCommandError::configuration_missing(
            "插件运行时未初始化，参数已保存，请重启后重试",
        ));
    }
    tracing::info!(plugin=%prepared.plugin.slug,connector=%input.connector_key,conversation_id=input.conversation_id,"[plugin-setup] 插件参数已安全保存，旧运行时已停止");
    Ok(load(conn, &query).await?.view)
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn plugin_preparation_configure(
    input: ConfigurationInput,
    db: tauri::State<'_, crate::db::AppDatabase>,
    app: tauri::AppHandle,
) -> Result<PreparationView, AppCommandError> {
    save(
        &db.conn,
        input,
        crate::web::event_bridge::EventEmitter::Tauri(app),
    )
    .await
}
