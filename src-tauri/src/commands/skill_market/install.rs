use futures_util::StreamExt;
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE};
use reqwest::Method;
use sea_orm::DatabaseConnection;

use crate::app_error::AppCommandError;
use crate::commands::acp::MarketSkillMarker;
use crate::models::AgentType;

use super::client;
use super::types::{parse_id, parse_value, SkillDownloadInfo};

const MAX_PACKAGE_BYTES: u64 = 30 * 1024 * 1024;

pub async fn install_core(
    conn: &DatabaseConnection,
    id: String,
    version: String,
    agent_type: AgentType,
) -> Result<(), AppCommandError> {
    let requested_id = parse_id(&id)?;
    let requested_version = semver::Version::parse(version.trim())
        .map_err(|error| {
            AppCommandError::invalid_input("Invalid Skill version").with_detail(error.to_string())
        })?
        .to_string();
    let detail = super::detail_core(
        conn,
        requested_id.to_string(),
        Some(requested_version.clone()),
    )
    .await?;
    validate_install_identity(&detail, requested_id, &requested_version)?;
    let metadata = download_metadata(conn, &id, &requested_version).await?;
    if metadata.version != requested_version {
        return Err(AppCommandError::configuration_invalid(
            "Skill download metadata returned a different version",
        ));
    }
    let package_bytes = download_package(&metadata).await.map_err(|error| {
        tracing::error!(skill_id = %id, version = %requested_version, error = %error, "[skill-market] package download failed");
        error
    })?;
    let object_hash = crate::acp::skill_package::hash_bytes(&package_bytes);
    if !object_hash.eq_ignore_ascii_case(&metadata.object_sha256) {
        return Err(AppCommandError::invalid_input(
            "Downloaded Skill package integrity check failed",
        ));
    }
    let package =
        crate::acp::skill_package::validate_zip(&package_bytes, &metadata.content_sha256)?;
    let marker = market_marker(&detail, &metadata, object_hash)?;
    crate::commands::acp::install_market_skill(agent_type, marker, package).map_err(|error| {
        tracing::error!(skill_id = %id, version = %requested_version, agent_type = %agent_type, error = %error, "[skill-market] local installation failed");
        map_local_install_error(error)
    })?;
    tracing::info!(
        skill_id = %id,
        slug = %detail.skill.slug,
        version = %metadata.version,
        agent_type = %agent_type,
        "[skill-market] Skill installed"
    );
    Ok(())
}

fn validate_install_identity(
    detail: &super::types::SkillMarketDetail,
    requested_id: i64,
    requested_version: &str,
) -> Result<(), AppCommandError> {
    if parse_id(&detail.skill.id)? != requested_id
        || detail.skill.current_version.version != requested_version
    {
        return Err(AppCommandError::configuration_invalid(
            "Skill detail response does not match the requested release",
        ));
    }
    Ok(())
}

fn map_local_install_error(error: crate::acp::error::AcpError) -> AppCommandError {
    let detail = error.to_string();
    let lowered = detail.to_ascii_lowercase();
    let result = if lowered.contains("already exists")
        || lowered.contains("different market skill")
        || lowered.contains("uses this skill slug")
        || lowered.contains("will not be overwritten")
    {
        AppCommandError::already_exists("A local Skill already uses this slug")
    } else if lowered.contains("storage") && lowered.contains("not initialized") {
        AppCommandError::agent_storage_not_initialized("Agent storage is not initialized")
    } else if lowered.contains("not supported") {
        AppCommandError::invalid_input("The selected Agent cannot install Skills")
    } else {
        AppCommandError::io_error("Failed to install Skill package")
    };
    result.with_detail(detail)
}

async fn download_metadata(
    conn: &DatabaseConnection,
    id: &str,
    version: &str,
) -> Result<SkillDownloadInfo, AppCommandError> {
    let id = parse_id(id)?;
    let builder = client::request(conn, Method::POST, "/skills/download")
        .await?
        .json(&serde_json::json!({ "id": id, "version": version }));
    parse_value(client::send(builder).await?, None)
}

async fn download_package(metadata: &SkillDownloadInfo) -> Result<Vec<u8>, AppCommandError> {
    if metadata.package_size == 0 || metadata.package_size > MAX_PACKAGE_BYTES {
        return Err(AppCommandError::invalid_input(
            "Skill package size is outside the allowed range",
        ));
    }
    let url = reqwest::Url::parse(&metadata.url)
        .map_err(|_| AppCommandError::configuration_invalid("Invalid Skill download URL"))?;
    if url.scheme() != "https" {
        return Err(AppCommandError::configuration_invalid(
            "Skill download URL must use HTTPS",
        ));
    }
    let response = client::http_client()?
        .get(url)
        .timeout(client::transfer_timeout())
        .send()
        .await
        .map_err(|error| {
            AppCommandError::network("Failed to download Skill package")
                .with_detail(error.to_string())
        })?;
    validate_download_response(&response, metadata.package_size)?;
    read_download(response, metadata.package_size).await
}

fn validate_download_response(
    response: &reqwest::Response,
    expected_size: u64,
) -> Result<(), AppCommandError> {
    if !response.status().is_success() {
        return Err(
            AppCommandError::network("Skill package download was rejected")
                .with_detail(response.status().to_string()),
        );
    }
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default();
    if !matches!(content_type, "application/zip" | "application/octet-stream") {
        return Err(AppCommandError::network(
            "Skill package response has an unsupported content type",
        ));
    }
    if response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|value| value != expected_size)
    {
        return Err(AppCommandError::invalid_input(
            "Skill package size does not match the release metadata",
        ));
    }
    Ok(())
}

async fn read_download(
    response: reqwest::Response,
    expected_size: u64,
) -> Result<Vec<u8>, AppCommandError> {
    let mut bytes = Vec::with_capacity(expected_size as usize);
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| {
            AppCommandError::network("Skill download was interrupted")
                .with_detail(error.to_string())
        })?;
        if bytes.len().saturating_add(chunk.len()) as u64 > expected_size {
            return Err(AppCommandError::invalid_input(
                "Skill package exceeds its declared size",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.len() as u64 != expected_size {
        return Err(AppCommandError::invalid_input(
            "Skill package is incomplete",
        ));
    }
    Ok(bytes)
}

fn market_marker(
    detail: &super::types::SkillMarketDetail,
    metadata: &SkillDownloadInfo,
    object_sha256: String,
) -> Result<MarketSkillMarker, AppCommandError> {
    Ok(MarketSkillMarker {
        schema_version: 1,
        source: "iyw_skill_market".to_string(),
        skill_id: parse_id(&detail.skill.id)?,
        slug: detail.skill.slug.clone(),
        installed_version: metadata.version.clone(),
        content_sha256: metadata.content_sha256.clone(),
        object_sha256,
        visibility: detail.skill.visibility.clone(),
        publisher_type: detail.skill.publisher_type.clone(),
        installed_at: chrono::Utc::now().to_rfc3339(),
    })
}
