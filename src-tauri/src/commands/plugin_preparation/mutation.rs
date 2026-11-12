use sea_orm::DatabaseConnection;
use tokio_util::sync::CancellationToken;

use super::{context, load, ChangeInput, ConnectionView, PreparationView, Prepared, ReadInput};
use crate::app_error::AppCommandError;
use crate::db::service::plugin_runtime_approval_service::{self, PluginApprovalScope};
use crate::plugin_runtime::{global, registry};

static CHECKS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
const CHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);

pub async fn change(
    conn: &DatabaseConnection,
    input: ChangeInput,
    action: (bool, crate::web::event_bridge::EventEmitter),
) -> Result<PreparationView, AppCommandError> {
    let (enabled, emitter) = action;
    let _storage = crate::acp::agent_storage_work::begin_agent_storage_read().await;
    let _operation = registry::lock_mutation().await;
    let query = query(&input);
    let prepared = load(conn, &query).await?;
    require_expected(&prepared, &input.context_token, enabled)?;
    persist_change(conn, &prepared, (enabled, &emitter)).await?;
    Ok(load(conn, &query).await?.view)
}

async fn persist_change(
    conn: &DatabaseConnection,
    prepared: &Prepared,
    action: (bool, &crate::web::event_bridge::EventEmitter),
) -> Result<(), AppCommandError> {
    let (enabled, emitter) = action;
    let workspace = prepared
        .workspace
        .as_ref()
        .ok_or_else(|| AppCommandError::invalid_input("请选择当前对话"))?;
    if !enabled {
        registry::suspend_global(&prepared.plugin.slug);
    }
    let result = plugin_runtime_approval_service::set_plugin_scope(
        conn,
        PluginApprovalScope {
            plugin_slug: &prepared.plugin.slug,
            workspace_key: &workspace.key,
            agent_type: workspace.agent.as_wire().as_ref(),
        },
        (
            &prepared.plugin.version,
            &prepared.plugin.permissions_digest,
            enabled,
        ),
    )
    .await;
    if let Err(error) = result {
        let _ = registry::reconcile_global(conn).await;
        return Err(AppCommandError::from(error));
    }
    if let Err(error) = registry::reconcile_global(conn).await {
        registry::suspend_global(&prepared.plugin.slug);
        global::stop_plugin(&prepared.plugin.slug).await;
        return Err(error);
    }
    if !enabled {
        stop_affected_workspaces(conn, (&prepared.plugin, &workspace.key), emitter).await;
    }
    tracing::info!(plugin=%prepared.plugin.slug, conversation_id=workspace.conversation_id, enabled, "[plugin-prepare] 工作区授权已更新");
    Ok(())
}

pub(crate) async fn stop_affected_workspaces(
    conn: &DatabaseConnection,
    context: (&crate::plugin_runtime::registry::PluginDescriptor, &str),
    emitter: &crate::web::event_bridge::EventEmitter,
) {
    let (plugin, workspace) = context;
    let (Some(supervisor), Some(snapshot)) = (global::supervisor(), registry::global_snapshot())
    else {
        return;
    };
    let id = plugin.market_skill_id.to_string();
    for target in snapshot.plugins.values() {
        let affected = target.slug == plugin.slug
            || crate::plugin_runtime::dependencies::inspect(target, &snapshot.plugins)
                .dependencies
                .iter()
                .any(|dependency| dependency.id == id);
        if affected {
            revoke_apps(conn, (&target.slug, workspace), emitter).await;
            if workspace.is_empty() {global::stop_plugin(&target.slug).await;} else {supervisor.stop_workspace(&target.slug, workspace).await;}
        }
    }
}

pub(crate) async fn stop_plugins_for_update(conn:&DatabaseConnection,slugs:&[String],emitter:&crate::web::event_bridge::EventEmitter) {
    for slug in slugs {
        revoke_apps(conn,(slug,""),emitter).await;
        global::stop_plugin(slug).await;
    }
}

async fn revoke_apps(
    conn: &DatabaseConnection,
    context: (&str, &str),
    emitter: &crate::web::event_bridge::EventEmitter,
) {
    match crate::db::service::plugin_app_instance_service::workspace_plugin_instances(conn, context)
        .await
    {
        Ok(ids) => {
            if let Some(apps) = global::apps() {
                for id in &ids {
                    apps.teardown(id);
                }
            }
            if context.1.is_empty() {global::app_launch_broker().cancel_plugin(context.0);} else {global::app_launch_broker().cancel_workspace(context.0, context.1);}
            crate::web::event_bridge::emit_event(
                emitter,
                "plugin-app://revoked",
                serde_json::json!({"instanceIds":ids}),
            );
        }
        Err(error) => {
            tracing::error!(plugin=context.0, error=%error, "[plugin-prepare] 插件界面撤权通知准备失败")
        }
    }
}

pub async fn check(
    conn: &DatabaseConnection,
    input: ChangeInput,
) -> Result<ConnectionView, AppCommandError> {
    let _permit = CHECKS
        .try_acquire()
        .map_err(|_| AppCommandError::invalid_input("已有插件连接检查正在进行，请稍后重试"))?;
    let _storage = crate::acp::agent_storage_work::begin_agent_storage_read().await;
    let prepared = {
        let _operation = registry::lock_mutation().await;
        load(conn, &query(&input)).await?
    };
    require_expected(&prepared, &input.context_token, true)?;
    if prepared.view.state != "unchecked" {
        return Err(AppCommandError::invalid_input("请先完成当前项目的插件授权"));
    }
    let tools = probe(&prepared).await?;
    let latest = load(conn, &query(&input)).await?;
    if latest.view.context_token != prepared.view.context_token {
        return Err(AppCommandError::invalid_input(
            "检查期间插件、权限或工作区发生变化，请重新检查",
        ));
    }
    tracing::info!(plugin=%prepared.plugin.slug, conversation_id=input.conversation_id, tool_count=tools, "[plugin-prepare] 插件连接检查完成");
    Ok(ConnectionView {
        state: "connection_verified".into(),
        tool_count: tools,
    })
}

async fn probe(prepared: &Prepared) -> Result<usize, AppCommandError> {
    let workspace = prepared
        .workspace
        .as_ref()
        .ok_or_else(|| AppCommandError::invalid_input("请选择当前对话"))?;
    let router = global::router()
        .ok_or_else(|| AppCommandError::configuration_missing("插件运行时未初始化"))?;
    let call = check_call(prepared, workspace);
    let tools = tokio::time::timeout(CHECK_TIMEOUT, async {
        let mut tools = 0;
        for connector in &prepared.view.connectors {
            tools += router
                .check_connector(&call, &connector.key)
                .await
                .map_err(|error| AppCommandError::configuration_invalid(error.message))?;
        }
        Ok::<usize, AppCommandError>(tools)
    })
    .await
    .map_err(|_| {
        AppCommandError::task_execution_failed("插件连接检查超时，请检查服务和运行时后重试")
    })??;
    Ok(tools)
}

fn require_expected(
    prepared: &Prepared,
    token: &str,
    enabling: bool,
) -> Result<(), AppCommandError> {
    if prepared.plugin.manifest.schema_version < 2 {
        return Err(AppCommandError::invalid_input(
            "旧版插件请通过连接器管理配置",
        ));
    }
    if prepared.view.context_token.as_deref() != Some(token) {
        return Err(AppCommandError::invalid_input(
            "插件版本、权限或工作区已变化，请重新核对后确认",
        ));
    }
    if enabling && (!prepared.view.issues.is_empty() || !prepared.view.dependencies.verified) {
        return Err(AppCommandError::configuration_invalid(
            "插件或依赖尚未准备好，请先修复或安装适用版本",
        ));
    }
    Ok(())
}

fn query(input: &ChangeInput) -> ReadInput {
    ReadInput {
        market_skill_id: input.market_skill_id.clone(),
        conversation_id: Some(input.conversation_id),
    }
}

pub(super) fn check_call(
    prepared: &Prepared,
    workspace: &context::Workspace,
) -> crate::plugin_runtime::types::PluginToolCall {
    crate::plugin_runtime::types::PluginToolCall {
        context: crate::plugin_runtime::types::PluginCallContext {
            connection_id: "plugin-preparation".into(),
            conversation_id: None,
            plugin_slug: prepared.plugin.slug.clone(),
            capability_id: String::new(),
            workspace_key: workspace.key.clone(),
            workspace_dir: workspace.directory.clone(),
            agent_type: workspace.agent,
            host_gateway_supported: true,
            cancellation: CancellationToken::new(),
            authority_cancellation: CancellationToken::new(),
            permission_revision: prepared.plugin.permissions_digest.clone(),
        },
        arguments: serde_json::Map::new(),
    }
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn plugin_preparation_approve(
    input: ChangeInput,
    db: tauri::State<'_, crate::db::AppDatabase>,
    app: tauri::AppHandle,
) -> Result<PreparationView, AppCommandError> {
    change(
        &db.conn,
        input,
        (true, crate::web::event_bridge::EventEmitter::Tauri(app)),
    )
    .await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn plugin_preparation_disable(
    input: ChangeInput,
    db: tauri::State<'_, crate::db::AppDatabase>,
    app: tauri::AppHandle,
) -> Result<PreparationView, AppCommandError> {
    change(
        &db.conn,
        input,
        (false, crate::web::event_bridge::EventEmitter::Tauri(app)),
    )
    .await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn plugin_preparation_check(
    input: ChangeInput,
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<ConnectionView, AppCommandError> {
    check(&db.conn, input).await
}
