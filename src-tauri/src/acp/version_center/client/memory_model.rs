use reqwest::Method;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

use super::{config, http_status_error, network_error, AgentPlatformClient, Envelope};
use crate::acp::version_center::capability;
use crate::app_error::AppCommandError;
use crate::update::preferences;

const RESOLVE_PATH: &str = "/app-updates/v1/memory-model/resolve";
const DOWNLOAD_PATH: &str = "/app-updates/v1/memory-model/download";
const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy)]
pub struct MemoryModelQuery<'a> {
    pub model_id: &'a str,
    pub installed_version: &'a str,
    pub channel: &'a str,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryModelOffer {
    pub component_id: String,
    pub model_id: String,
    pub display_name: String,
    pub version: String,
    pub action: String,
    pub artifact: MemoryModelArtifact,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryModelArtifact {
    pub version_id: String,
    pub artifact_id: String,
    pub package_kind: String,
    pub file_name: String,
    pub size: i64,
    pub sha256: String,
    #[serde(default)]
    pub signature: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub expires_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MemoryModelRequest<'a> {
    schema_version: u32,
    installation_id: &'a str,
    client_version: &'static str,
    channel: &'a str,
    runtime: &'static str,
    target: &'static str,
    arch: &'static str,
    model_id: &'a str,
    installed_version: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MemoryModelDownloadRequest<'a> {
    #[serde(flatten)]
    request: MemoryModelRequest<'a>,
    version_id: &'a str,
    artifact_id: &'a str,
}

impl AgentPlatformClient {
    pub async fn resolve_memory_model(
        conn: &DatabaseConnection,
        query: MemoryModelQuery<'_>,
    ) -> Result<MemoryModelOffer, AppCommandError> {
        let installation_id = installation_id(conn).await?;
        let body = request_body(&installation_id, query);
        post_public(RESOLVE_PATH, &body).await
    }

    pub async fn refresh_memory_model_download(
        conn: &DatabaseConnection,
        query: MemoryModelQuery<'_>,
        artifact: &MemoryModelArtifact,
    ) -> Result<MemoryModelArtifact, AppCommandError> {
        let installation_id = installation_id(conn).await?;
        let body = MemoryModelDownloadRequest {
            request: request_body(&installation_id, query),
            version_id: &artifact.version_id,
            artifact_id: &artifact.artifact_id,
        };
        post_public(DOWNLOAD_PATH, &body).await
    }
}

fn request_body<'a>(
    installation_id: &'a str,
    query: MemoryModelQuery<'a>,
) -> MemoryModelRequest<'a> {
    MemoryModelRequest {
        schema_version: SCHEMA_VERSION,
        installation_id,
        client_version: env!("CARGO_PKG_VERSION"),
        channel: query.channel,
        runtime: capability::RUNTIME,
        target: capability::current_target(),
        arch: capability::current_arch(),
        model_id: query.model_id,
        installed_version: query.installed_version,
    }
}

async fn installation_id(conn: &DatabaseConnection) -> Result<String, AppCommandError> {
    let value = preferences::load(conn).await?.installation_id;
    if value.trim().is_empty() {
        return Err(AppCommandError::configuration_invalid(
            "Installation identity is unavailable",
        ));
    }
    Ok(value)
}

async fn post_public<T: Serialize, R: serde::de::DeserializeOwned>(
    path: &str,
    body: &T,
) -> Result<R, AppCommandError> {
    let response = config::http_client()?
        .request(Method::POST, config::endpoint(path)?)
        .json(body)
        .send()
        .await
        .map_err(network_error)?;
    decode_public_response(response).await
}

async fn decode_public_response<R: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<R, AppCommandError> {
    let status = response.status();
    if !status.is_success() {
        tracing::warn!(%status, "[memory-model] Fusion request failed");
        return Err(http_status_error(status));
    }
    let bytes = response.bytes().await.map_err(network_error)?;
    let envelope = serde_json::from_slice::<Envelope>(&bytes).map_err(|error| {
        AppCommandError::configuration_invalid("Invalid memory model response")
            .with_detail(error.to_string())
    })?;
    if envelope.code != 1 {
        tracing::warn!(code = envelope.code, message = %envelope.message, "[memory-model] Fusion rejected request");
        return Err(super::error::envelope_error(envelope));
    }
    serde_json::from_value(envelope.data).map_err(|error| {
        AppCommandError::configuration_invalid("Invalid memory model response")
            .with_detail(error.to_string())
    })
}
