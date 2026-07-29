use std::sync::LazyLock;
use std::time::Duration;

use reqwest::{redirect::Policy, Method, RequestBuilder, Url};
use sea_orm::DatabaseConnection;
use serde::de::DeserializeOwned;
use serde::Deserialize;

use crate::acp::version_center::capability;
use crate::acp::version_center::types::{
    AgentOffer, CatalogSnapshot, DownloadRequest, DownloadTicket, ResolveAgentRequest,
    ResolveToolRequest, ToolOffer, VersionHistory,
};
use crate::app_error::AppCommandError;
use crate::commands::iyw_account::iyw_account_access_token_core;
use crate::update::preferences;

const BASE_URL_ENV: &str = "IYW_CLAW_AGENT_PLATFORM_BASE_URL";
const FUSION_BASE_URL_ENV: &str = "IYW_CLAW_FUSION_API_BASE_URL";
const INSTALLATION_HEADER: &str = "X-IYW-Installation-ID";

#[cfg(debug_assertions)]
const DEFAULT_BASE_URL: &str = "http://127.0.0.1:6001";
#[cfg(all(not(debug_assertions), feature = "test-gateway"))]
const DEFAULT_BASE_URL: &str = "http://192.168.1.86:3201/ai-application";
#[cfg(all(not(debug_assertions), not(feature = "test-gateway")))]
const DEFAULT_BASE_URL: &str = "https://gateway.iyw.cn/iyw-fusion-api";

static HTTP_CLIENT: LazyLock<Result<reqwest::Client, String>> = LazyLock::new(|| {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .redirect(Policy::none())
        .build()
        .map_err(|error| error.to_string())
});

#[derive(Debug)]
pub enum CatalogFetch {
    NotModified,
    Updated {
        snapshot: CatalogSnapshot,
        etag: Option<String>,
    },
}

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
        let preferences = preferences::load(conn).await?;
        let query = [
            ("clientVersion", env!("CARGO_PKG_VERSION")),
            ("runtime", capability::RUNTIME),
            ("target", capability::current_target()),
            ("arch", capability::current_arch()),
            ("channel", preferences.channel.as_str()),
        ];
        let mut request = Self::request(conn, Method::GET, "/agent-platforms/v1/catalog")
            .await?
            .query(&query);
        if let Some(etag) = etag.filter(|value| !value.trim().is_empty()) {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        let response = request.send().await.map_err(network_error)?;
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(CatalogFetch::NotModified);
        }
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(ToString::to_string);
        let snapshot = decode_response(response).await?;
        Ok(CatalogFetch::Updated { snapshot, etag })
    }

    pub async fn agent_history(
        conn: &DatabaseConnection,
        registry_id: &str,
        channel: &str,
    ) -> Result<VersionHistory, AppCommandError> {
        let path = format!("/agent-platforms/v1/agents/{registry_id}/versions");
        Self::history(conn, &path, channel).await
    }

    pub async fn tool_history(
        conn: &DatabaseConnection,
        tool_id: &str,
        channel: &str,
    ) -> Result<VersionHistory, AppCommandError> {
        let path = format!("/agent-platforms/v1/tools/{tool_id}/versions");
        Self::history(conn, &path, channel).await
    }

    pub async fn resolve_agent(
        conn: &DatabaseConnection,
        request: ResolveAgentRequest<'_>,
    ) -> Result<AgentOffer, AppCommandError> {
        let offer = Self::post(conn, "/agent-platforms/v1/resolve", &request).await?;
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
        channel: &str,
    ) -> Result<VersionHistory, AppCommandError> {
        let query = catalog_query(channel);
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
        let installation_id = preferences::load(conn).await?.installation_id;
        Ok(http_client()?
            .request(method, endpoint(path)?)
            .header("token", token.expose())
            .header(INSTALLATION_HEADER, installation_id))
    }
}

fn catalog_query(channel: &str) -> [(&'static str, String); 5] {
    [
        ("clientVersion", env!("CARGO_PKG_VERSION").to_string()),
        ("runtime", capability::RUNTIME.to_string()),
        ("target", capability::current_target().to_string()),
        ("arch", capability::current_arch().to_string()),
        ("channel", channel.to_string()),
    ]
}

fn http_client() -> Result<reqwest::Client, AppCommandError> {
    HTTP_CLIENT.as_ref().cloned().map_err(|error| {
        AppCommandError::configuration_invalid("Failed to initialize Agent platform client")
            .with_detail(error)
    })
}

fn endpoint(path: &str) -> Result<Url, AppCommandError> {
    let base = configured_base_url();
    let parsed = Url::parse(&format!("{base}/")).map_err(|error| {
        AppCommandError::configuration_invalid("Invalid Agent platform base URL")
            .with_detail(error.to_string())
    })?;
    let insecure_dev_allowed = cfg!(debug_assertions) || cfg!(feature = "test-gateway");
    let valid = parsed.host_str().is_some()
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.query().is_none()
        && parsed.fragment().is_none()
        && (parsed.scheme() == "https" || (insecure_dev_allowed && parsed.scheme() == "http"));
    if !valid {
        return Err(AppCommandError::configuration_invalid(
            "Agent platform base URL must be an HTTPS origin",
        ));
    }
    parsed.join(path.trim_start_matches('/')).map_err(|error| {
        AppCommandError::configuration_invalid("Invalid Agent platform endpoint")
            .with_detail(error.to_string())
    })
}

fn configured_base_url() -> String {
    for key in [BASE_URL_ENV, FUSION_BASE_URL_ENV] {
        if let Ok(value) = std::env::var(key) {
            let value = value.trim().trim_end_matches('/');
            if !value.is_empty() {
                return value.to_string();
            }
        }
    }
    DEFAULT_BASE_URL.to_string()
}

async fn decode_response<T: DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, AppCommandError> {
    if !response.status().is_success() {
        return Err(
            AppCommandError::network("Agent platform gateway rejected the request")
                .with_detail(response.status().to_string()),
        );
    }
    let envelope = response.json::<Envelope>().await.map_err(|error| {
        AppCommandError::configuration_invalid("Invalid Agent platform response")
            .with_detail(error.to_string())
    })?;
    if envelope.code != 1 {
        return Err(AppCommandError::invalid_input(envelope.message));
    }
    serde_json::from_value(envelope.data).map_err(|error| {
        AppCommandError::configuration_invalid("Invalid Agent platform response data")
            .with_detail(error.to_string())
    })
}

fn network_error(error: reqwest::Error) -> AppCommandError {
    AppCommandError::network("Agent platform request failed").with_detail(error.to_string())
}

fn rejected_offer(error: String) -> AppCommandError {
    AppCommandError::configuration_invalid("Agent platform offer was rejected").with_detail(error)
}
