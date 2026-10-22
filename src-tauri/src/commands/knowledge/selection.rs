use std::collections::BTreeSet;
use std::sync::OnceLock;

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, TransactionTrait};
use sha2::{Digest, Sha256};

use super::models::{Choice, ChoiceKind, Selection, UpdateSelection, MAX_SELECTION};
use crate::app_error::AppCommandError;
use crate::db::{entities::conversation, service::app_metadata_service};

const PREFIX: &str = "knowledge.conversation.v1.";

pub async fn account_scope(conn: &DatabaseConnection) -> Result<String, AppCommandError> {
    if crate::commands::iyw_account::workspace_switching() {
        return Err(AppCommandError::permission_denied(
            "空间正在切换，请稍后重试",
        ));
    }
    let raw = app_metadata_service::get_value(conn, "iyw_account_session")
        .await
        .map_err(AppCommandError::db)?;
    let session: serde_json::Value = raw
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .ok_or_else(|| AppCommandError::authentication_failed("请先登录后使用知识库"))?;
    let session_id = session["session_id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| AppCommandError::authentication_failed("请重新登录后使用知识库"))?;
    if session["token"].is_null() || session["workspace_pending"] == true {
        return Err(AppCommandError::authentication_failed(
            "请先登录并选择企业空间",
        ));
    }
    Ok(format!(
        "{:x}",
        Sha256::digest(
            format!(
                "{session_id}:{}",
                session["org_code"].as_str().unwrap_or_default()
            )
            .as_bytes()
        )
    ))
}

pub async fn load(
    conn: &DatabaseConnection,
    id: Option<i32>,
) -> Result<Selection, AppCommandError> {
    let scope = account_scope(conn).await?;
    let Some(id) = id else {
        return Ok(Selection {
            scope,
            ..Selection::default()
        });
    };
    require_conversation(conn, id).await?;
    load_scoped(conn, id, &scope).await
}

pub async fn load_scoped(
    conn: &DatabaseConnection,
    id: i32,
    scope: &str,
) -> Result<Selection, AppCommandError> {
    let mut selection = decode(
        app_metadata_service::get_value(conn, &key(id, scope))
            .await
            .map_err(AppCommandError::db)?,
    )?;
    selection.scope = scope.to_string();
    Ok(selection)
}

fn decode(raw: Option<String>) -> Result<Selection, AppCommandError> {
    raw.map(|raw| {
        serde_json::from_str(&raw)
            .map_err(|_| AppCommandError::configuration_invalid("知识库选择记录损坏，请重新选择"))
    })
    .transpose()
    .map(Option::unwrap_or_default)
}

pub async fn update(
    conn: &DatabaseConnection,
    input: UpdateSelection,
) -> Result<Selection, AppCommandError> {
    validate(&input.selected)?;
    require_conversation(conn, input.conversation_id).await?;
    let scope = account_scope(conn).await?;
    let _guard = operation_lock().lock().await;
    if scope != input.scope || account_scope(conn).await? != scope {
        return Err(AppCommandError::permission_denied(
            "账号或空间已变更，请重新打开知识库",
        ));
    }
    let transaction = conn
        .begin()
        .await
        .map_err(|error| AppCommandError::db(error.into()))?;
    let id = input.conversation_id;
    let next = persist(&transaction, input, scope).await?;
    transaction
        .commit()
        .await
        .map_err(|error| AppCommandError::db(error.into()))?;
    tracing::info!(
        conversation_id = id,
        count = next.selected.len(),
        revision = next.revision,
        "[knowledge] selection saved"
    );
    Ok(next)
}

async fn persist(
    transaction: &sea_orm::DatabaseTransaction,
    input: UpdateSelection,
    scope: String,
) -> Result<Selection, AppCommandError> {
    let key = key(input.conversation_id, &scope);
    let current = decode(
        app_metadata_service::get_value_conn(transaction, &key)
            .await
            .map_err(AppCommandError::db)?,
    )?;
    if current.revision != input.revision {
        return Err(AppCommandError::new(
            crate::app_error::AppErrorCode::Conflict,
            "知识库选择已变更，请重新打开后保存",
        ));
    }
    let next = Selection {
        scope,
        revision: current.revision.saturating_add(1),
        selected: input.selected,
    };
    let value = serde_json::to_string(&next)
        .map_err(|_| AppCommandError::invalid_input("无法保存知识库选择"))?;
    app_metadata_service::upsert_value(transaction, &key, &value)
        .await
        .map_err(AppCommandError::db)?;
    Ok(next)
}

fn validate(selected: &[Choice]) -> Result<(), AppCommandError> {
    if selected.len() > MAX_SELECTION {
        return Err(AppCommandError::invalid_input(
            "最多选择 32 个知识库文件夹或文件",
        ));
    }
    let mut seen = BTreeSet::new();
    for item in selected {
        super::models::validate_category(item.category)?;
        if !super::models::valid_id(&item.id)
            || !super::models::valid_id(&item.folder_id)
            || (item.kind == ChoiceKind::Folder && item.id != item.folder_id)
            || item.name.trim().is_empty()
            || item.name.chars().count() > 512
            || !seen.insert((item.category, format!("{:?}", item.kind), &item.id))
        {
            return Err(AppCommandError::invalid_input("知识库选择无效，请重新选择"));
        }
    }
    Ok(())
}

pub async fn require_conversation(
    conn: &DatabaseConnection,
    id: i32,
) -> Result<(), AppCommandError> {
    conversation::Entity::find_by_id(id)
        .filter(conversation::Column::DeletedAt.is_null())
        .one(conn)
        .await
        .map_err(|error| AppCommandError::db(error.into()))?
        .ok_or_else(|| AppCommandError::not_found("对话不存在"))?;
    Ok(())
}

fn key(id: i32, scope: &str) -> String {
    format!("{PREFIX}{scope}.{id}")
}

fn operation_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

pub async fn prompt_hint(
    conn: &DatabaseConnection,
    id: Option<i32>,
) -> Option<std::sync::Arc<str>> {
    let selection = load(conn, id).await.ok()?;
    if selection.selected.is_empty() {
        return None;
    }
    Some(std::sync::Arc::from(
        "The user selected cloud knowledge sources for this conversation. When document evidence is relevant, use search_iyw_knowledge with a focused query. The host enforces the selected files and folders (including descendants); omit category/folderId/fileId overrides. Cite returned document names. No matches means no evidence in the selected sources; do not broaden the search automatically."
    ))
}
