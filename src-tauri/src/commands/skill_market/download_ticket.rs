use futures_util::StreamExt;
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE};
use reqwest::{Method, Url};
use sea_orm::DatabaseConnection;
use serde::Deserialize;

use crate::app_error::AppCommandError;

use super::client;
use super::types::{parse_id, SkillInstallPlanItem};

const DOWNLOAD_ATTEMPTS: usize = 3;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtifactTicket {
    url: String,
    artifact: TicketArtifact,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TicketArtifact {
    artifact_size: u64,
    artifact_sha256: String,
}

pub(super) async fn download_package(
    conn: &DatabaseConnection,
    item: &SkillInstallPlanItem,
) -> Result<Vec<u8>, AppCommandError> {
    let expected = &item.download;
    if expected.package_size == 0 {
        return Err(AppCommandError::invalid_input(
            "Skill package size is outside the allowed range",
        ));
    }
    for attempt in 1..=DOWNLOAD_ATTEMPTS {
        let ticket = request_ticket(conn, item).await?;
        let size = validate_ticket(item, &ticket)?;
        match download_once(&ticket.url, size).await {
            Ok(bytes) => return Ok(bytes),
            Err(error) if attempt < DOWNLOAD_ATTEMPTS => {
                tracing::warn!(skill_id = %item.skill_id, version = %item.version, attempt,
                    error = %error, "[skill-market] direct artifact download interrupted; retrying");
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("download attempts always return or fail")
}

async fn request_ticket(
    conn: &DatabaseConnection,
    item: &SkillInstallPlanItem,
) -> Result<ArtifactTicket, AppCommandError> {
    let id = parse_id(&item.skill_id)?;
    let builder = client::request(conn, Method::POST, "/skills/artifact-ticket")
        .await?
        .json(&serde_json::json!({
            "id": id.to_string(), "version": item.version,
            "client": super::plugin_client_version::context(),
        }));
    serde_json::from_value(client::send(builder).await?)
        .map_err(|_| AppCommandError::configuration_invalid("Skill artifact ticket is invalid"))
}

fn validate_ticket(
    item: &SkillInstallPlanItem,
    ticket: &ArtifactTicket,
) -> Result<u64, AppCommandError> {
    let artifact = &ticket.artifact;
    let expected = &item.download;
    if artifact.artifact_size == 0
        || (expected.artifact_size > 0 && artifact.artifact_size != expected.artifact_size)
        || !expected.object_sha256.is_empty()
            && !artifact
                .artifact_sha256
                .eq_ignore_ascii_case(&expected.object_sha256)
        || artifact.artifact_sha256.len() != 64
        || !artifact
            .artifact_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(AppCommandError::configuration_invalid(
            "Skill artifact ticket does not match the install plan",
        ));
    }
    Ok(artifact.artifact_size)
}

async fn download_once(url: &str, expected_size: u64) -> Result<Vec<u8>, AppCommandError> {
    let parsed = Url::parse(url)
        .map_err(|_| AppCommandError::configuration_invalid("Invalid artifact URL"))?;
    let local = matches!(parsed.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
    if (parsed.scheme() != "https"
        && !(cfg!(debug_assertions) && local && parsed.scheme() == "http"))
        || parsed.username() != ""
        || parsed.password().is_some()
        || parsed.fragment().is_some()
    {
        return Err(AppCommandError::configuration_invalid(
            "Invalid artifact URL",
        ));
    }
    let response = client::http_client()?
        .get(parsed)
        .timeout(client::transfer_timeout())
        .send()
        .await
        .map_err(|_| AppCommandError::network("Skill artifact download failed"))?;
    validate_response(&response, expected_size)?;
    read_response(response, expected_size).await
}

fn validate_response(
    response: &reqwest::Response,
    expected_size: u64,
) -> Result<(), AppCommandError> {
    if !response.status().is_success() {
        return Err(
            AppCommandError::network("Skill artifact download was rejected")
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
    if !matches!(content_type, "application/zip" | "application/octet-stream")
        || response
            .content_length()
            .is_some_and(|size| size != expected_size)
    {
        return Err(AppCommandError::network(
            "Skill artifact response metadata is invalid",
        ));
    }
    Ok(())
}

async fn read_response(
    response: reqwest::Response,
    expected_size: u64,
) -> Result<Vec<u8>, AppCommandError> {
    let declared_size = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|_| AppCommandError::network("Skill artifact download interrupted"))?;
        if (bytes.len() as u64)
            .checked_add(chunk.len() as u64)
            .is_none_or(|size| size > expected_size)
        {
            return Err(AppCommandError::network(
                "Skill artifact size exceeds the ticket",
            ));
        }
        bytes
            .try_reserve(chunk.len())
            .map_err(|_| AppCommandError::io_error("Insufficient memory for Skill artifact"))?;
        bytes.extend_from_slice(&chunk);
    }
    if bytes.len() as u64 != expected_size
        || declared_size.is_some_and(|size| size != expected_size)
    {
        return Err(AppCommandError::network(
            "Skill artifact download is incomplete",
        ));
    }
    Ok(bytes)
}
