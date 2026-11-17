use std::time::Duration;

use futures::{stream, StreamExt, TryStreamExt};
use sea_orm::DatabaseConnection;
use serde_json::{json, Value};

use super::{
    api::KnowledgeClient,
    message_sources,
    models::{Choice, ChoiceKind, Folder, Selection},
    results, selection,
};
use crate::app_error::AppCommandError;

const SEARCH_CONCURRENCY: usize = 4;
const SEARCH_TIMEOUT: Duration = Duration::from_secs(45);

pub async fn search(
    conn: &DatabaseConnection,
    conversation_id: i32,
    body: Value,
) -> Result<Value, AppCommandError> {
    let result = tokio::time::timeout(SEARCH_TIMEOUT, execute(conn, conversation_id, body))
        .await
        .map_err(|_| AppCommandError::network("知识库检索超时，请缩小选择范围后重试"))?;
    if let Err(error) = &result {
        tracing::warn!(conversation_id, code = ?error.code, detail = ?error.detail,
            "[knowledge] search failed");
    }
    result
}

async fn execute(
    conn: &DatabaseConnection,
    id: i32,
    mut body: Value,
) -> Result<Value, AppCommandError> {
    let client = KnowledgeClient::new(conn).await?;
    selection::require_conversation(conn, id).await?;
    let (selected, message_scoped) = scope_for_search(conn, (id, &client.scope), &mut body).await?;
    let value = if selected.selected.is_empty() {
        results::normalize(client.post("search", body).await?)?
    } else {
        search_selected(&client, &body, &selected.selected).await?
    };
    let scope_changed = if message_scoped {
        selection::account_scope(conn).await? != client.scope
    } else {
        let current = selection::load(conn, Some(id)).await?;
        current.scope != client.scope || current.revision != selected.revision
    };
    if scope_changed {
        return Err(AppCommandError::permission_denied(
            "知识库范围或账号已变更，请重新检索",
        ));
    }
    tracing::info!(conversation_id = id, message_scoped, selected_count = selected.selected.len(),
        count = ?value["count"].as_u64(), "[knowledge] search completed");
    Ok(value)
}

async fn scope_for_search(
    conn: &DatabaseConnection,
    context: (i32, &str),
    body: &mut Value,
) -> Result<(Selection, bool), AppCommandError> {
    let references = body
        .as_object_mut()
        .and_then(|body| body.remove("sourceRefs"))
        .unwrap_or(Value::Null);
    if references.is_null() {
        return Ok((
            selection::load_scoped(conn, context.0, context.1).await?,
            false,
        ));
    }
    let references: Vec<String> = serde_json::from_value(references)
        .map_err(|_| AppCommandError::invalid_input("知识库 sourceRefs 无效"))?;
    Ok((
        message_sources::resolve(conn, context, &references).await?,
        true,
    ))
}

async fn search_selected(
    client: &KnowledgeClient,
    body: &Value,
    selected: &[Choice],
) -> Result<Value, AppCommandError> {
    let choices = effective_choices(client, selected).await?;
    let groups = stream::iter(choices)
        .map(|choice: Choice| async move { search_choice(client, body, &choice).await })
        .buffered(SEARCH_CONCURRENCY)
        .try_collect::<Vec<_>>()
        .await?;
    Ok(results::merge(
        groups,
        body["limit"].as_u64().unwrap_or(10) as usize,
    ))
}

async fn search_choice(
    client: &KnowledgeClient,
    body: &Value,
    choice: &Choice,
) -> Result<Value, AppCommandError> {
    let mut body = body.clone();
    body["category"] = json!(choice.category);
    body["folderId"] = if choice.kind == ChoiceKind::Folder {
        json!(choice.id)
    } else {
        Value::Null
    };
    body["fileId"] = if choice.kind == ChoiceKind::File {
        json!(choice.id)
    } else {
        Value::Null
    };
    let mut result = results::normalize(client.post("search", body).await?)?;
    if let Some(items) = result["results"].as_array_mut() {
        for item in items {
            item["category"] = json!(choice.category);
        }
    }
    Ok(result)
}

async fn effective_choices(
    client: &KnowledgeClient,
    choices: &[Choice],
) -> Result<Vec<Choice>, AppCommandError> {
    let mut effective = Vec::new();
    for category in [0, 1] {
        let selected: Vec<_> = choices
            .iter()
            .filter(|item| item.category == category)
            .collect();
        if selected.is_empty() {
            continue;
        }
        let folders = client.folders(category).await?;
        for choice in &selected {
            if !contains(&folders, &choice.folder_id) {
                return Err(AppCommandError::not_found(
                    "已选文件夹不可用，请重新选择知识库",
                ));
            }
            if !selected.iter().any(|parent| {
                parent.kind == ChoiceKind::Folder
                    && parent.id != choice.id
                    && covers(&folders, &parent.id, &choice.folder_id)
            }) {
                effective.push((**choice).clone());
            }
        }
    }
    Ok(effective)
}

fn contains(folders: &[Folder], id: &str) -> bool {
    folders
        .iter()
        .any(|folder| folder.id == id || contains(&folder.children, id))
}

fn covers(folders: &[Folder], parent: &str, target: &str) -> bool {
    folders.iter().any(|folder| {
        if folder.id == parent {
            contains(std::slice::from_ref(folder), target)
        } else {
            covers(&folder.children, parent, target)
        }
    })
}
