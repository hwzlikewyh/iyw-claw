mod client;
mod install;
mod types;

pub use install::install_core;

pub const MAX_SKILL_MARKET_REQUEST_BYTES: usize = 36 * 1024 * 1024;

use reqwest::Method;
use sea_orm::DatabaseConnection;

use crate::app_error::AppCommandError;
use types::{parse_id, parse_value, FileNode, FileTree, SkillMarketFile, SkillMarketItem};
pub use types::{
    SkillMarketAddVersionRequest, SkillMarketCategory, SkillMarketDetail, SkillMarketListParams,
    SkillMarketListResult, SkillMarketMetadataRequest, SkillMarketPublishRequest,
    SkillMarketVersion,
};

pub async fn list_core(
    conn: &DatabaseConnection,
    params: SkillMarketListParams,
) -> Result<SkillMarketListResult, AppCommandError> {
    let mut query = vec![("view", params.view)];
    push_query(&mut query, "visibility", params.visibility);
    push_query(&mut query, "publisherType", params.publisher_type);
    push_query(&mut query, "category", params.category);
    push_query(&mut query, "q", params.q);
    push_query(
        &mut query,
        "page",
        params.page.map(|value| value.to_string()),
    );
    push_query(
        &mut query,
        "pageSize",
        params.page_size.map(|value| value.to_string()),
    );
    let builder = client::request(conn, Method::GET, "/skills/list")
        .await?
        .query(&query);
    parse_value(client::send(builder).await?, None)
}

pub async fn categories_core(
    conn: &DatabaseConnection,
) -> Result<Vec<SkillMarketCategory>, AppCommandError> {
    let builder = client::request(conn, Method::GET, "/skills/categories").await?;
    parse_value(client::send(builder).await?, Some("items"))
}

pub async fn detail_core(
    conn: &DatabaseConnection,
    id: String,
    version: Option<String>,
) -> Result<SkillMarketDetail, AppCommandError> {
    let id_number = parse_id(&id)?;
    let body = serde_json::json!({ "id": id_number, "version": version });
    let skill_builder = client::request(conn, Method::POST, "/skills/detail")
        .await?
        .json(&body);
    let skill: SkillMarketItem = parse_value(client::send(skill_builder).await?, Some("skill"))?;
    let files_builder = client::request(conn, Method::POST, "/skills/files")
        .await?
        .json(&body);
    let tree: FileTree = parse_value(client::send(files_builder).await?, None)?;
    let mut files = Vec::new();
    flatten_files(tree.tree, &mut files);
    Ok(SkillMarketDetail { skill, files })
}

pub async fn versions_core(
    conn: &DatabaseConnection,
    id: String,
) -> Result<Vec<SkillMarketVersion>, AppCommandError> {
    let builder = client::request(conn, Method::POST, "/skills/versions/list")
        .await?
        .json(&serde_json::json!({ "id": parse_id(&id)? }));
    let value = client::send(builder).await?;
    if value.get("items").is_some() {
        parse_value(value, Some("items"))
    } else {
        parse_value(value, None)
    }
}

pub async fn publish_core(
    conn: &DatabaseConnection,
    request: SkillMarketPublishRequest,
) -> Result<SkillMarketDetail, AppCommandError> {
    let form = client::upload_form(
        vec![
            ("slug", request.slug),
            ("displayName", request.display_name),
            ("summary", request.summary),
            ("category", request.category),
            ("iconUrl", request.icon_url.unwrap_or_default()),
            ("visibility", request.visibility),
            ("version", request.version),
            ("changelog", request.changelog),
        ],
        &request.tags,
        request.files,
    )?;
    let builder = client::request(conn, Method::POST, "/skills")
        .await?
        .multipart(form)
        .timeout(client::transfer_timeout());
    let skill: SkillMarketItem = parse_value(
        client::send(builder).await.map_err(|error| {
            tracing::error!(error = %error, "[skill-market] publish request failed");
            error
        })?,
        Some("skill"),
    )?;
    tracing::info!(skill_id = %skill.id, slug = %skill.slug, "[skill-market] Skill published");
    Ok(refresh_detail_or_minimal(conn, skill).await)
}

pub async fn add_version_core(
    conn: &DatabaseConnection,
    request: SkillMarketAddVersionRequest,
) -> Result<SkillMarketDetail, AppCommandError> {
    let id = request.id;
    let mut fallback = detail_core(conn, id.clone(), None).await?;
    let form = client::upload_form(
        vec![
            ("id", parse_id(&id)?.to_string()),
            ("version", request.version),
            ("changelog", request.changelog),
        ],
        &[],
        request.files,
    )?;
    let builder = client::request(conn, Method::POST, "/skills/versions")
        .await?
        .multipart(form)
        .timeout(client::transfer_timeout());
    let version: SkillMarketVersion = parse_value(
        client::send(builder).await.map_err(|error| {
            tracing::error!(skill_id = %id, error = %error, "[skill-market] add version request failed");
            error
        })?,
        Some("version"),
    )?;
    fallback.skill.current_version = version;
    fallback.files.clear();
    tracing::info!(skill_id = %id, version = %fallback.skill.current_version.version, "[skill-market] Skill version published");
    Ok(refresh_detail_or_fallback(conn, id, fallback).await)
}

pub async fn update_metadata_core(
    conn: &DatabaseConnection,
    request: SkillMarketMetadataRequest,
) -> Result<SkillMarketDetail, AppCommandError> {
    let id = request.id.clone();
    let mut value = serde_json::to_value(request).map_err(|error| {
        AppCommandError::invalid_input("Failed to serialize Skill metadata")
            .with_detail(error.to_string())
    })?;
    value["id"] = serde_json::json!(parse_id(&id)?);
    let builder = client::request(conn, Method::POST, "/skills/update")
        .await?
        .json(&value);
    let skill: SkillMarketItem = parse_value(
        client::send(builder).await.map_err(|error| {
            tracing::error!(skill_id = %id, error = %error, "[skill-market] metadata update failed");
            error
        })?,
        Some("skill"),
    )?;
    tracing::info!(skill_id = %id, "[skill-market] Skill metadata updated");
    Ok(refresh_detail_or_minimal(conn, skill).await)
}

pub async fn delete_core(conn: &DatabaseConnection, id: String) -> Result<(), AppCommandError> {
    let builder = client::request(conn, Method::POST, "/skills/delete")
        .await?
        .json(&serde_json::json!({ "id": parse_id(&id)? }));
    client::send(builder).await.map_err(|error| {
        tracing::error!(skill_id = %id, error = %error, "[skill-market] delete request failed");
        error
    })?;
    tracing::info!(skill_id = %id, "[skill-market] Skill deleted");
    Ok(())
}

async fn refresh_detail_or_minimal(
    conn: &DatabaseConnection,
    skill: SkillMarketItem,
) -> SkillMarketDetail {
    let id = skill.id.clone();
    let fallback = SkillMarketDetail {
        skill,
        files: Vec::new(),
    };
    refresh_detail_or_fallback(conn, id, fallback).await
}

async fn refresh_detail_or_fallback(
    conn: &DatabaseConnection,
    id: String,
    fallback: SkillMarketDetail,
) -> SkillMarketDetail {
    match detail_core(conn, id.clone(), None).await {
        Ok(detail) => detail,
        Err(error) => {
            tracing::warn!(skill_id = %id, error = %error, "[skill-market] mutation committed but detail refresh failed");
            fallback
        }
    }
}

fn push_query(query: &mut Vec<(&'static str, String)>, key: &'static str, value: Option<String>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        query.push((key, value));
    }
}

fn flatten_files(nodes: Vec<FileNode>, files: &mut Vec<SkillMarketFile>) {
    for node in nodes {
        if node.kind == "file" {
            files.push(SkillMarketFile {
                path: node.path,
                size: node.size.unwrap_or_default(),
                sha256: node.sha256,
                mime_type: node.mime_type,
            });
        } else {
            flatten_files(node.children, files);
        }
    }
}

#[cfg(feature = "tauri-runtime")]
mod tauri_commands;

#[cfg(feature = "tauri-runtime")]
pub use tauri_commands::*;
