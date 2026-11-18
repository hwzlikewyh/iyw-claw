use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde::Deserialize;

use crate::app_error::AppCommandError;
use crate::models::AgentType;

#[derive(Clone)]
pub(crate) struct DraftWorkspace {
    pub directory: PathBuf,
    pub agent: AgentType,
    pub conversation_id: Option<i32>,
    account_scope: String,
}

#[derive(Deserialize)]
#[serde(
    tag = "operation",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum DraftRequest {
    Prepare {
        workspace_path: String,
        agent_type: AgentType,
    },
    Bind {
        context_id: i32,
        conversation_id: i32,
    },
    Release {
        context_id: i32,
    },
}

fn workspaces() -> &'static Mutex<BTreeMap<i32, DraftWorkspace>> {
    static WORKSPACES: OnceLock<Mutex<BTreeMap<i32, DraftWorkspace>>> = OnceLock::new();
    WORKSPACES.get_or_init(|| Mutex::new(BTreeMap::new()))
}

pub(crate) async fn workspace(
    conn: &sea_orm::DatabaseConnection,
    id: i64,
) -> Result<DraftWorkspace, AppCommandError> {
    let id = i32::try_from(id).map_err(|_| invalid())?;
    let value = workspaces()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(&id)
        .cloned()
        .ok_or_else(invalid)?;
    let scope = crate::commands::mcp_cli_config::account_scope(conn).await?;
    if value.account_scope != scope {
        return Err(AppCommandError::permission_denied(
            "插件草稿账号已变化，请重新打开插件",
        ));
    }
    Ok(value)
}

pub async fn draft_core(
    db: &crate::db::AppDatabase,
    request: DraftRequest,
) -> Result<i32, AppCommandError> {
    let _storage = crate::acp::agent_storage_work::begin_agent_storage_read().await;
    crate::commands::mcp::require_private_agent_storage_for_write()?;
    match request {
        DraftRequest::Prepare {
            workspace_path,
            agent_type,
        } => prepare(db, workspace_path, agent_type).await,
        DraftRequest::Bind {
            context_id,
            conversation_id,
        } => {
            bind(db, context_id, conversation_id).await?;
            Ok(context_id)
        }
        DraftRequest::Release { context_id } => {
            release(db, context_id).await?;
            Ok(context_id)
        }
    }
}

async fn prepare(
    db: &crate::db::AppDatabase,
    path: String,
    agent: AgentType,
) -> Result<i32, AppCommandError> {
    let _operation = crate::plugin_runtime::registry::lock_mutation().await;
    let directory = std::fs::canonicalize(path).map_err(AppCommandError::io)?;
    if !directory.is_dir() {
        return Err(invalid());
    }
    let scope = crate::commands::mcp_cli_config::account_scope(&db.conn).await?;
    let value = DraftWorkspace {
        directory,
        agent,
        account_scope: scope,
        conversation_id: None,
    };
    let mut contexts = workspaces().lock().unwrap_or_else(|p| p.into_inner());
    let id = loop {
        let candidate = -((rand::random::<u32>() % i32::MAX as u32) as i32) - 1;
        if !contexts.contains_key(&candidate) {
            break candidate;
        }
    };
    contexts.insert(id, value);
    tracing::info!(
        draft_context_id = id,
        "[plugin-draft] 临时工作区已准备，未创建会话"
    );
    Ok(id)
}

async fn bind(
    db: &crate::db::AppDatabase,
    id: i32,
    conversation_id: i32,
) -> Result<(), AppCommandError> {
    if conversation_id <= 0 {
        return Err(invalid());
    }
    let draft = workspace(&db.conn, i64::from(id)).await?;
    let current =
        super::plugin_app_context::conversation_workspace(db, i64::from(conversation_id)).await?;
    let directory = std::fs::canonicalize(current.workspace_dir).map_err(AppCommandError::io)?;
    if draft.directory != directory || draft.agent != current.agent_type {
        return Err(AppCommandError::permission_denied(
            "插件草稿与发送会话的工作区不一致",
        ));
    }
    let _operation = crate::plugin_runtime::registry::lock_mutation().await;
    let mut contexts = workspaces().lock().unwrap_or_else(|p| p.into_inner());
    let current = contexts.get_mut(&id).ok_or_else(invalid)?;
    if current
        .conversation_id
        .is_some_and(|bound| bound != conversation_id)
    {
        return Err(AppCommandError::permission_denied("插件草稿已关联其他会话"));
    }
    super::plugin_app_studio::bind_studio_draft(&current.account_scope, id, conversation_id)?;
    current.conversation_id = Some(conversation_id);
    tracing::info!(
        draft_context_id = id,
        conversation_id,
        "[plugin-draft] 首次发送已关联真实会话"
    );
    Ok(())
}

async fn release(db: &crate::db::AppDatabase, id: i32) -> Result<(), AppCommandError> {
    let _operation = crate::plugin_runtime::registry::lock_mutation().await;
    if !workspaces()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .contains_key(&id)
    {
        return Ok(());
    }
    workspace(&db.conn, i64::from(id)).await?;
    let instances = crate::db::service::plugin_app_instance_service::list_for_conversation(
        &db.conn,
        i64::from(id),
    )
    .await
    .map_err(AppCommandError::db)?;
    for instance in instances {
        if let Some(apps) = crate::plugin_runtime::global::apps() {
            apps.teardown(&instance.instance_id);
        }
        crate::db::service::plugin_app_instance_service::mark_inactive(
            &db.conn,
            &instance.instance_id,
            Some(i64::from(id)),
        )
        .await
        .map_err(AppCommandError::db)?;
    }
    workspaces()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(&id);
    tracing::info!(draft_context_id = id, "[plugin-draft] 临时工作区及租约已释放");
    Ok(())
}

fn invalid() -> AppCommandError {
    AppCommandError::configuration_missing("插件临时工作区已失效，请重新打开插件")
}
