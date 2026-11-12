use super::{load, ReadInput};
use crate::app_error::AppCommandError;
use crate::plugin_runtime::{
    authentication::{AccountStatus, AuthOperation},
    global, registry,
};
use sea_orm::DatabaseConnection;
use serde::Deserialize;

static ACCOUNT_CALLS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountInput {
    pub market_skill_id: String,
    pub conversation_id: i32,
    pub context_token: String,
    pub connector_key: String,
    pub operation: AuthOperation,
}

pub async fn operate(
    conn: &DatabaseConnection,
    input: AccountInput,
    emitter: crate::web::event_bridge::EventEmitter,
) -> Result<AccountStatus, AppCommandError> {
    let _permit = ACCOUNT_CALLS
        .try_acquire()
        .map_err(|_| AppCommandError::invalid_input("插件账号操作正在进行，请稍后重试"))?;
    let _storage = crate::acp::agent_storage_work::begin_agent_storage_read().await;
    let query = ReadInput {
        market_skill_id: input.market_skill_id,
        conversation_id: Some(input.conversation_id),
    };
    let prepared = {
        let _operation = registry::lock_mutation().await;
        load(conn, &query).await?
    };
    if prepared.view.context_token.as_deref() != Some(input.context_token.as_str()) {
        return Err(AppCommandError::invalid_input(
            "插件、参数或当前项目已变化，请重新打开账号准备",
        ));
    }
    let workspace = prepared
        .workspace
        .as_ref()
        .ok_or_else(|| AppCommandError::invalid_input("请选择当前对话项目"))?;
    let connector = prepared
        .view
        .connectors
        .iter()
        .find(|connector| connector.key == input.connector_key)
        .ok_or_else(|| AppCommandError::not_found("插件连接器不存在"))?;
    if !prepared.plugin.available
        || !prepared.view.permission_granted
        || !connector.enabled
        || !connector.configuration_ready
    {
        return Err(AppCommandError::configuration_invalid(
            "请先完成插件参数、依赖和当前项目权限准备",
        ));
    }
    let call = super::mutation::check_call(&prepared, workspace);
    let result = global::router()
        .ok_or_else(|| AppCommandError::configuration_missing("插件运行时未初始化"))?
        .authentication(&call, (&input.connector_key, input.operation))
        .await
        .map_err(|error| AppCommandError::configuration_invalid(error.message))?;
    let latest = load(conn, &query).await?;
    if latest.view.context_token != prepared.view.context_token {
        return Err(AppCommandError::invalid_input(
            "账号操作期间插件或项目授权已变化，请重新检查账号状态",
        ));
    }
    if matches!(input.operation, AuthOperation::Logout) {
        super::mutation::stop_affected_workspaces(conn, (&prepared.plugin, ""), &emitter).await;
    }
    tracing::info!(plugin=%prepared.plugin.slug,connector=%input.connector_key,state=%result.state,"[plugin-account] 插件账号操作结束");
    Ok(result)
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn plugin_preparation_account(
    input: AccountInput,
    db: tauri::State<'_, crate::db::AppDatabase>,
    app: tauri::AppHandle,
) -> Result<AccountStatus, AppCommandError> {
    operate(
        &db.conn,
        input,
        crate::web::event_bridge::EventEmitter::Tauri(app),
    )
    .await
}
