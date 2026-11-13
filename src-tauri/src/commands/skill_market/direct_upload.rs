use std::collections::{HashMap, HashSet};

use reqwest::{Method, Url};
use sea_orm::DatabaseConnection;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::app_error::AppCommandError;
use crate::commands::mcp_cli_config::account_scope;

use super::{client, types::deserialize_id, SkillMarketDetail, SkillMarketUploadFile};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadSession {
    #[serde(deserialize_with = "deserialize_id")]
    upload_id: String,
    #[serde(deserialize_with = "deserialize_id")]
    skill_id: String,
    version: String,
    files: Vec<UploadFile>,
}

#[derive(Deserialize)]
struct UploadFile {
    #[serde(deserialize_with = "deserialize_id")]
    id: String,
    path: String,
    size: u64,
    sha256: String,
}

#[derive(Deserialize)]
struct UploadTarget {
    url: String,
    method: String,
    #[serde(default)]
    headers: HashMap<String, String>,
}

pub(super) async fn publish(
    conn: &DatabaseConnection,
    mut metadata: Value,
    files: Vec<SkillMarketUploadFile>,
) -> Result<SkillMarketDetail, AppCommandError> {
    let scope = account_scope(conn).await?;
    let files = client::decode_upload_files(files)?;
    metadata["files"] = json!(files
        .iter()
        .map(|file| json!({
            "path": file.path, "size": file.bytes.len(),
            "sha256": format!("{:x}", Sha256::digest(&file.bytes)),
            "mimeType": "application/octet-stream"
        }))
        .collect::<Vec<_>>());
    let session: UploadSession =
        super::parse_value(api(conn, "/skills/uploads/init", metadata).await?, None)?;
    validate_session(&session, &files)?;
    tracing::info!(upload_id = %session.upload_id, skill_id = %session.skill_id,
        files = files.len(), "[skill-market] TOS upload initialized");
    transfer_files(conn, (&scope, &session), files).await?;
    ensure_scope(conn, &scope).await?;
    complete(conn, &session).await?;
    ensure_scope(conn, &scope).await?;
    super::detail_core(
        conn,
        session.skill_id.clone(),
        Some(session.version.clone()),
    )
    .await
    .map_err(|error| {
        error.with_detail(format!(
            "已提交；uploadId={};skillId={};version={}；请回读原版本，勿重复发布",
            session.upload_id, session.skill_id, session.version
        ))
    })
}

fn validate_session(
    session: &UploadSession,
    files: &[client::DecodedUploadFile],
) -> Result<(), AppCommandError> {
    let mut paths = HashSet::new();
    let mut ids = HashSet::new();
    if session.files.len() != files.len()
        || session.files.iter().any(|remote| {
            !paths.insert(&remote.path)
                || !ids.insert(&remote.id)
                || !files.iter().any(|file| {
                    remote.path == file.path
                        && remote.size == file.bytes.len() as u64
                        && remote.sha256 == format!("{:x}", Sha256::digest(&file.bytes))
                })
        })
    {
        return Err(AppCommandError::configuration_invalid(
            "上传会话与本地文件清单不一致",
        ));
    }
    Ok(())
}

async fn transfer_files(
    conn: &DatabaseConnection,
    context: (&str, &UploadSession),
    files: Vec<client::DecodedUploadFile>,
) -> Result<(), AppCommandError> {
    let (scope, session) = context;
    for file in files {
        ensure_scope(conn, scope).await?;
        let remote = session
            .files
            .iter()
            .find(|item| item.path == file.path)
            .ok_or_else(|| AppCommandError::configuration_invalid("上传文件映射缺失"))?;
        let target = super::parse_value(
            api(
                conn,
                "/skills/uploads/url",
                json!({
                    "uploadId": session.upload_id, "fileId": remote.id
                }),
            )
            .await?,
            None,
        )?;
        put(target, file.bytes).await.map_err(|error| {
            tracing::warn!(upload_id = %session.upload_id, file_id = %remote.id,
                "[skill-market] TOS transfer incomplete; query original upload before retry");
            error.with_detail(format!(
                "uploadId={};fileId={}",
                session.upload_id, remote.id
            ))
        })?;
    }
    Ok(())
}

async fn put(target: UploadTarget, bytes: Vec<u8>) -> Result<(), AppCommandError> {
    let url = Url::parse(&target.url).map_err(|_| upload_error())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || target.method != "PUT"
    {
        return Err(upload_error());
    }
    let mut request = client::http_client()?
        .put(url)
        .timeout(client::transfer_timeout());
    for (name, value) in target.headers {
        if !matches!(
            name.to_ascii_lowercase().as_str(),
            "host"
                | "content-length"
                | "token"
                | "tokeninfo"
                | "admin-token"
                | "authorization"
                | "cookie"
        ) {
            request = request.header(name, value);
        }
    }
    let response = request
        .body(bytes)
        .send()
        .await
        .map_err(|_| upload_error())?;
    if !response.status().is_success() {
        return Err(upload_error());
    }
    Ok(())
}

async fn complete(
    conn: &DatabaseConnection,
    session: &UploadSession,
) -> Result<(), AppCommandError> {
    let body = json!({"uploadId": session.upload_id});
    match api(conn, "/skills/uploads/complete", body.clone()).await {
        Ok(value) if value["status"] == "ready" => Ok(()),
        result => {
            if let Ok(value) = api(conn, "/skills/uploads/status", body).await {
                if value["status"] == "ready" {
                    return Ok(());
                }
            }
            Err(result
                .err()
                .unwrap_or_else(|| AppCommandError::network("上传版本尚未就绪"))
                .with_detail(format!(
                    "uploadId={};skillId={};version={}",
                    session.upload_id, session.skill_id, session.version
                )))
        }
    }
}

async fn ensure_scope(conn: &DatabaseConnection, expected: &str) -> Result<(), AppCommandError> {
    if account_scope(conn).await? != expected {
        return Err(AppCommandError::permission_denied(
            "上传期间账号或企业空间已变化，请重新加载",
        ));
    }
    Ok(())
}

async fn api(conn: &DatabaseConnection, path: &str, body: Value) -> Result<Value, AppCommandError> {
    let request = client::request(conn, Method::POST, path)
        .await?
        .json(&body)
        .timeout(client::transfer_timeout());
    client::send(request).await
}

fn upload_error() -> AppCommandError {
    AppCommandError::network("TOS 文件直传未完成，请查询原上传任务后续传")
}
