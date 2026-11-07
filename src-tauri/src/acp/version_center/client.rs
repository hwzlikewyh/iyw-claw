use std::time::Duration;

use reqwest::{Method, RequestBuilder};
use sea_orm::DatabaseConnection;
use serde::de::DeserializeOwned;
use serde::Deserialize;

use crate::acp::version_center::capability;
use crate::acp::version_center::types::{
    AgentOffer, DownloadRequest, DownloadTicket, ResolveAgentRequest, ResolveToolRequest,
    ToolOffer, VersionHistory,
};
use crate::app_error::AppCommandError;
use crate::commands::iyw_account::iyw_account_access_token_core;
use crate::update::preferences;

mod capability_policy;
mod catalog;
mod config;
mod error;
pub use capability_policy::CapabilityPolicyHttpFetcher;
pub use catalog::CatalogFetch;
use config::{endpoint, http_client, INSTALLATION_HEADER};
use error::{envelope_error, retryable_agent_resolve_error};

const AGENT_RESOLVE_RETRY_DELAY_MS: u64 = 1_000;

#[derive(Debug, Deserialize)]
struct Envelope {
    code: i32,
    data: serde_json::Value,
    message: String,
}

pub struct AgentPlatformClient;

impl AgentPlatformClient {
    pub async fn fetch_catalog(
        conn: &DatabaseConnection,
        etag: Option<&str>,
    ) -> Result<CatalogFetch, AppCommandError> {
        catalog::fetch(conn, etag).await
    }

    pub async fn agent_history(
        conn: &DatabaseConnection,
        registry_id: &str,
        channel: &str,
    ) -> Result<VersionHistory, AppCommandError> {
        let path = format!("/agent-platforms/v1/agents/{registry_id}/versions");
        Self::history(
            conn,
            &path,
            catalog_query(channel, capability::current_agent_target()),
        )
        .await
    }

    pub async fn tool_history(
        conn: &DatabaseConnection,
        tool_id: &str,
        channel: &str,
    ) -> Result<VersionHistory, AppCommandError> {
        let path = format!("/agent-platforms/v1/tools/{tool_id}/versions");
        Self::history(
            conn,
            &path,
            catalog_query(channel, capability::current_target()),
        )
        .await
    }

    pub async fn resolve_agent(
        conn: &DatabaseConnection,
        request: ResolveAgentRequest<'_>,
    ) -> Result<AgentOffer, AppCommandError> {
        let request = ResolveAgentRequest {
            target: capability::current_agent_target(),
            ..request
        };
        let first: Result<AgentOffer, AppCommandError> =
            Self::post(conn, "/agent-platforms/v1/resolve", &request).await;
        let offer = match first {
            Ok(offer) => offer,
            Err(error) if retryable_agent_resolve_error(&error) => {
                tracing::warn!(
                    error_code = error.detail.as_deref().unwrap_or("unknown"),
                    registry_id = request.registry_id,
                    runtime = request.runtime,
                    target = request.target,
                    arch = request.arch,
                    reason = request.reason,
                    retry_attempt = 2,
                    retry_delay_ms = AGENT_RESOLVE_RETRY_DELAY_MS,
                    "[AgentPlatform] retrying agent resolve after transient catalog error"
                );
                tokio::time::sleep(Duration::from_millis(AGENT_RESOLVE_RETRY_DELAY_MS)).await;
                Self::post(conn, "/agent-platforms/v1/resolve", &request).await?
            }
            Err(error) => return Err(error),
        };
        capability::validate_agent_offer(&offer).map_err(rejected_offer)?;
        Ok(offer)
    }

    pub async fn resolve_tool(
        conn: &DatabaseConnection,
        request: ResolveToolRequest<'_>,
    ) -> Result<ToolOffer, AppCommandError> {
        let offer = Self::post(conn, "/agent-platforms/v1/tools/resolve", &request).await?;
        capability::validate_tool_offer(&offer).map_err(rejected_offer)?;
        Ok(offer)
    }

    pub async fn download_agent(
        conn: &DatabaseConnection,
        request: DownloadRequest<'_>,
    ) -> Result<DownloadTicket, AppCommandError> {
        let request = DownloadRequest {
            target: capability::current_agent_target(),
            ..request
        };
        Self::post(conn, "/agent-platforms/v1/download", &request).await
    }

    pub async fn download_tool(
        conn: &DatabaseConnection,
        request: DownloadRequest<'_>,
    ) -> Result<DownloadTicket, AppCommandError> {
        Self::post(conn, "/agent-platforms/v1/tools/download", &request).await
    }

    async fn history(
        conn: &DatabaseConnection,
        path: &str,
        query: [(&str, String); 5],
    ) -> Result<VersionHistory, AppCommandError> {
        let response = Self::request(conn, Method::GET, path)
            .await?
            .query(&query)
            .send()
            .await;
        decode_response(response.map_err(network_error)?).await
    }

    async fn post<T: serde::Serialize, R: DeserializeOwned>(
        conn: &DatabaseConnection,
        path: &str,
        body: &T,
    ) -> Result<R, AppCommandError> {
        let response = Self::request(conn, Method::POST, path)
            .await?
            .json(body)
            .send()
            .await;
        decode_response(response.map_err(network_error)?).await
    }

    async fn request(
        conn: &DatabaseConnection,
        method: Method,
        path: &str,
    ) -> Result<RequestBuilder, AppCommandError> {
        let token = iyw_account_access_token_core(conn)
            .await?
            .ok_or_else(|| AppCommandError::authentication_failed("Sign in to iyw-claw first"))?;
        let prefs = preferences::load(conn).await?;
        let installation_id = prefs.installation_id;
        let url = endpoint(path)?;
        tracing::debug!(
            "[AgentPlatform] {} {} installation_id={}",
            method,
            url,
            if installation_id.is_empty() {
                "<empty>"
            } else {
                &installation_id
            }
        );
        if installation_id.is_empty() {
            tracing::warn!("[AgentPlatform] installation_id is empty — server may reject request");
        }
        Ok(http_client()?
            .request(method, url)
            .header("token", token.expose())
            .header(INSTALLATION_HEADER, &installation_id))
    }
}

fn catalog_query(channel: &str, target: &str) -> [(&'static str, String); 5] {
    [
        ("clientVersion", env!("CARGO_PKG_VERSION").to_string()),
        ("runtime", capability::RUNTIME.to_string()),
        ("target", target.to_string()),
        ("arch", capability::current_arch().to_string()),
        ("channel", channel.to_string()),
    ]
}

async fn decode_response<T: DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, AppCommandError> {
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        tracing::warn!(
            "[AgentPlatform] HTTP {} — body: {}",
            status,
            text_preview(&body, 512)
        );
        return Err(http_status_error(status));
    }
    let bytes = response.bytes().await.map_err(network_error)?;
    let envelope = serde_json::from_slice::<Envelope>(&bytes).map_err(|error| {
        tracing::warn!(
            "[AgentPlatform] failed to parse response envelope: {} — body: {}",
            error,
            String::from_utf8_lossy(if bytes.len() > 512 {
                &bytes[..512]
            } else {
                &bytes
            })
        );
        AppCommandError::configuration_invalid("Invalid Agent platform response data")
            .with_detail(error.to_string())
    })?;
    if envelope.code != 1 {
        tracing::warn!(
            "[AgentPlatform] envelope code={} message={} data={}",
            envelope.code,
            envelope.message,
            envelope.data
        );
        return Err(envelope_error(envelope));
    }
    serde_json::from_value(envelope.data).map_err(|error| {
        AppCommandError::configuration_invalid("Invalid Agent platform response data")
            .with_detail(error.to_string())
    })
}

fn text_preview(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn network_error(error: reqwest::Error) -> AppCommandError {
    let detail = error.to_string();
    if error.is_connect() || error.is_timeout() {
        return AppCommandError::network("Agent platform request failed").with_detail(detail);
    }
    AppCommandError::configuration_invalid("Agent platform request failed").with_detail(detail)
}

fn http_status_error(status: reqwest::StatusCode) -> AppCommandError {
    let error = match status {
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
            AppCommandError::authentication_failed("Agent platform rejected authentication")
        }
        reqwest::StatusCode::REQUEST_TIMEOUT | reqwest::StatusCode::TOO_MANY_REQUESTS => {
            AppCommandError::network("Agent platform is temporarily unavailable")
        }
        _ if status.is_server_error() => {
            AppCommandError::network("Agent platform is temporarily unavailable")
        }
        _ if status.is_client_error() => {
            AppCommandError::invalid_input("Agent platform rejected the request")
        }
        _ => AppCommandError::configuration_invalid(
            "Agent platform returned an unexpected HTTP status",
        ),
    };
    error.with_detail(status.to_string())
}

fn rejected_offer(error: String) -> AppCommandError {
    AppCommandError::configuration_invalid("Agent platform offer was rejected").with_detail(error)
}
