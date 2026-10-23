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
    #[serde(default, deserialize_with = "null_default")]
    pub doc_id: String,
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
    #[serde(default, deserialize_with = "null_default")]
    pub file_url: String,
    #[serde(default, deserialize_with = "null_default")]
    pub tos_url: String,
    #[serde(default, deserialize_with = "null_default")]
    pub sync_message: String,
    #[serde(default)]
    pub sync_status: Option<i32>,
    #[serde(default)]
    pub point_num: Option<u64>,
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
    #[serde(default)]
    pub force_refresh: bool,
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
                "category": input.category, "fileIds": input.file_ids,
                "forceRefresh": input.force_refresh,
            }),
        )
        .await?;
    require_scope(conn, &input.scope).await?;
    Ok(result)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileMutation {
    pub scope: String,
    pub category: u8,
    pub file_id: String,
    #[serde(default)]
    pub file_name: Option<String>,
    #[serde(default)]
    pub folder_id: Option<String>,
}

pub async fn update_file(
    conn: &DatabaseConnection,
    input: FileMutation,
) -> Result<(), AppCommandError> {
    models::validate_category(input.category)?;
    if !models::valid_id(&input.file_id) {
        return Err(AppCommandError::invalid_input("文件 ID 无效"));
    }
    let name = input
        .file_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty());
    if name.is_none() && input.folder_id.is_none() {
        return Err(AppCommandError::invalid_input("没有要修改的文件属性"));
    }
    if name.is_some_and(|name| name.chars().count() > 180 || name.chars().any(|ch| ch.is_control()))
    {
        return Err(AppCommandError::invalid_input("文件名无效或过长"));
    }
    if let Some(folder_id) = input.folder_id.as_deref() {
        if !models::valid_id(folder_id) {
            return Err(AppCommandError::invalid_input("目标目录无效"));
        }
        let client = scoped_client(conn, &input.scope).await?;
        if !contains_folder(&client.folders(input.category).await?, folder_id) {
            return Err(AppCommandError::not_found("目标目录不存在"));
        }
        client
            .post::<serde_json::Value>(
                "files/update",
                json!({
                    "category": input.category, "fileId": input.file_id,
                    "fileName": name, "folderId": folder_id,
                }),
            )
            .await?;
        return require_scope(conn, &input.scope).await;
    }
    let client = scoped_client(conn, &input.scope).await?;
    client
        .post::<serde_json::Value>(
            "files/update",
            json!({
                "category": input.category, "fileId": input.file_id, "fileName": name,
            }),
        )
        .await?;
    require_scope(conn, &input.scope).await
}

pub async fn delete_file(
    conn: &DatabaseConnection,
    input: FileMutation,
) -> Result<(), AppCommandError> {
    models::validate_category(input.category)?;
    if !models::valid_id(&input.file_id) {
        return Err(AppCommandError::invalid_input("文件 ID 无效"));
    }
    let client = scoped_client(conn, &input.scope).await?;
    client
        .post::<serde_json::Value>(
            "files/delete",
            json!({
                "category": input.category, "fileId": input.file_id,
            }),
        )
        .await?;
    require_scope(conn, &input.scope).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PointsQuery {
    pub scope: String,
    pub category: u8,
    pub file_id: String,
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_point_limit")]
    pub limit: usize,
}

fn default_point_limit() -> usize {
    100
}

pub async fn points(
    conn: &DatabaseConnection,
    input: PointsQuery,
) -> Result<serde_json::Value, AppCommandError> {
    models::validate_category(input.category)?;
    if !models::valid_id(&input.file_id) || input.limit == 0 || input.limit > 100 {
        return Err(AppCommandError::invalid_input("切片查询参数无效"));
    }
    let client = scoped_client(conn, &input.scope).await?;
    let result = client
        .post(
            "points/list",
            json!({
                "category": input.category, "fileId": input.file_id,
                "offset": input.offset, "limit": input.limit, "getAttachmentLink": true,
            }),
        )
        .await?;
    require_scope(conn, &input.scope).await?;
    Ok(result)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PointMutation {
    pub scope: String,
    pub category: u8,
    pub file_id: String,
    #[serde(default)]
    pub point_id: Option<String>,
    #[serde(default)]
    pub chunk_type: Option<String>,
    #[serde(default)]
    pub chunk_title: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub question: Option<String>,
}

pub async fn create_point(
    conn: &DatabaseConnection,
    input: PointMutation,
) -> Result<(), AppCommandError> {
    point_mutation(conn, "points/create", input).await
}

pub async fn update_point(
    conn: &DatabaseConnection,
    input: PointMutation,
) -> Result<(), AppCommandError> {
    if input
        .point_id
        .as_deref()
        .is_none_or(|id| !models::valid_id(id))
    {
        return Err(AppCommandError::invalid_input("切片 ID 无效"));
    }
    point_mutation(conn, "points/update", input).await
}

pub async fn delete_point(
    conn: &DatabaseConnection,
    input: PointMutation,
) -> Result<(), AppCommandError> {
    if input
        .point_id
        .as_deref()
        .is_none_or(|id| !models::valid_id(id))
    {
        return Err(AppCommandError::invalid_input("切片 ID 无效"));
    }
    point_mutation(conn, "points/delete", input).await
}

async fn point_mutation(
    conn: &DatabaseConnection,
    operation: &str,
    input: PointMutation,
) -> Result<(), AppCommandError> {
    models::validate_category(input.category)?;
    if !models::valid_id(&input.file_id)
        || input.content.as_deref().unwrap_or("").chars().count() > 5000
    {
        return Err(AppCommandError::invalid_input("切片内容无效"));
    }
    let client = scoped_client(conn, &input.scope).await?;
    let mut body = json!({
        "category": input.category, "fileId": input.file_id,
        "pointId": input.point_id, "chunkType": input.chunk_type,
        "chunkTitle": input.chunk_title, "content": input.content, "question": input.question,
    });
    if operation == "points/delete" {
        body =
            json!({"category": input.category, "fileId": input.file_id, "pointId": input.point_id});
    }
    client.post::<serde_json::Value>(operation, body).await?;
    require_scope(conn, &input.scope).await
}

fn contains_folder(folders: &[models::Folder], id: &str) -> bool {
    folders
        .iter()
        .any(|folder| folder.id == id || contains_folder(&folder.children, id))
}
