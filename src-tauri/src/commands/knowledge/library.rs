use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{api::KnowledgeClient, models, selection};
use crate::app_error::AppCommandError;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryFolders {
    pub scope: String,
    pub folders: Vec<models::Folder>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LibraryQuery {
    pub scope: String,
    pub category: u8,
    pub folder_id: String,
    pub page: usize,
    #[serde(default)]
    pub search_text: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryFile {
    pub file_id: String,
    pub file_name: String,
    pub folder_id: String,
    #[serde(default, deserialize_with = "null_default")]
    pub file_size: u64,
    #[serde(default, deserialize_with = "null_default")]
    pub file_ext: String,
    #[serde(default, deserialize_with = "null_default")]
    pub upload_employ_name: String,
    #[serde(default, deserialize_with = "null_default")]
    pub updated_at: String,
    #[serde(default)]
    pub doc_process_status: Option<i32>,
    #[serde(default, deserialize_with = "null_default")]
    pub doc_process_message: String,
}

#[derive(Deserialize, Serialize)]
pub struct LibraryFiles {
    pub list: Vec<LibraryFile>,
    pub total: usize,
}

pub async fn folders(
    conn: &DatabaseConnection,
    category: u8,
) -> Result<LibraryFolders, AppCommandError> {
    let client = KnowledgeClient::new(conn).await?;
    let folders = client.folders(category).await?;
    require_scope(conn, &client.scope).await?;
    Ok(LibraryFolders {
        scope: client.scope,
        folders,
    })
}

pub async fn files(
    conn: &DatabaseConnection,
    input: LibraryQuery,
) -> Result<LibraryFiles, AppCommandError> {
    models::validate_category(input.category)?;
    if !models::valid_id(&input.folder_id)
        || input.page == 0
        || input.search_text.chars().count() > 200
    {
        return Err(AppCommandError::invalid_input("目录、页码或搜索词无效"));
    }
    let client = scoped_client(conn, &input.scope).await?;
    let result = client
        .post(
            "files/list",
            json!({
                "category": input.category, "folderId": input.folder_id,
                "searchText": input.search_text.trim(), "includeChildren": false,
                "sortBy": "name", "sortOrder": "asc", "page": input.page, "pageSize": 50,
            }),
        )
        .await?;
    require_scope(conn, &input.scope).await?;
    Ok(result)
}

pub async fn require_scope(
    conn: &DatabaseConnection,
    expected: &str,
) -> Result<(), AppCommandError> {
    if expected.is_empty() || selection::account_scope(conn).await? != expected {
        return Err(AppCommandError::permission_denied(
            "账号或企业空间已变更，请重新打开资料库",
        ));
    }
    Ok(())
}

pub async fn scoped_client(
    conn: &DatabaseConnection,
    expected: &str,
) -> Result<KnowledgeClient, AppCommandError> {
    let client = KnowledgeClient::new(conn).await?;
    if client.scope != expected {
        return Err(AppCommandError::permission_denied(
            "账号或企业空间已变更，请重试",
        ));
    }
    Ok(client)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RefreshFiles {
    pub scope: String,
    pub category: u8,
    pub file_ids: Vec<String>,
}

#[derive(Deserialize, Serialize)]
pub struct RefreshedFiles {
    pub list: Vec<RefreshedFile>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshedFile {
    pub file_id: String,
    #[serde(default)]
    pub doc_process_status: Option<i32>,
    #[serde(default, deserialize_with = "null_default")]
    pub doc_process_message: String,
}

fn null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

pub async fn refresh(
    conn: &DatabaseConnection,
    input: RefreshFiles,
) -> Result<RefreshedFiles, AppCommandError> {
    models::validate_category(input.category)?;
    if input.file_ids.is_empty()
        || input.file_ids.len() > 10
        || !input.file_ids.iter().all(|id| models::valid_id(id))
    {
        return Err(AppCommandError::invalid_input("待更新文件列表无效"));
    }
    let client = scoped_client(conn, &input.scope).await?;
    let result = client
        .post(
            "files/batch-refresh-status",
            json!({
                "category": input.category, "fileIds": input.file_ids, "forceRefresh": false,
            }),
        )
        .await?;
    require_scope(conn, &input.scope).await?;
    Ok(result)
}
