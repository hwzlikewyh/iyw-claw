use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::{blocking::Client, redirect::Policy, Url};
use serde::{de::DeserializeOwned, Serialize};

use crate::failure::{self, Failure};
use crate::model::{
    ApiResponse, DownloadRequest, EnvironmentAction, EnvironmentArtifact, EnvironmentPlan,
    ResolveRequest,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const API_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(20 * 60);

pub struct FusionClient {
    base_url: String,
    http: Client,
}

impl FusionClient {
    pub fn new() -> Result<Self> {
        let base_url = std::env::var("IYW_CLAW_FUSION_API_BASE_URL")
            .unwrap_or_else(|_| "https://gateway.iyw.cn/iyw-fusion-api".to_string());
        let url = Url::parse(&base_url).context("invalid Fusion URL")?;
        if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
            bail!("Fusion API requires HTTPS without embedded credentials")
        }
        let http = Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(DOWNLOAD_TIMEOUT)
            .redirect(Policy::none())
            .build()
            .context("create environment HTTP client")?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http,
        })
    }

    pub fn resolve(&self, request: &ResolveRequest) -> Result<EnvironmentPlan> {
        crate::retry::run("环境计划", || {
            let plan: EnvironmentPlan = self.post("resolve", request)?;
            for action in &plan.actions {
                if action.action != "keep" {
                    validate_download_url(&action.artifact.url)?;
                }
            }
            Ok(plan)
        })
    }

    pub fn refresh(
        &self,
        request: &ResolveRequest,
        action: &EnvironmentAction,
    ) -> Result<EnvironmentArtifact> {
        let input = DownloadRequest {
            environment: request,
            component_id: &action.component_id,
            version_id: &action.artifact.version_id,
            artifact_id: &action.artifact.artifact_id,
        };
        let artifact: EnvironmentArtifact = self.post("download", &input)?;
        let expected = &action.artifact;
        if artifact.artifact_id != expected.artifact_id
            || artifact.version_id != expected.version_id
            || artifact.sha256 != expected.sha256
            || artifact.size_bytes != expected.size_bytes
        {
            return Err(
                Failure::permanent("PLAN", "刷新下载地址时制品身份发生变化，请重新安装").into(),
            );
        }
        validate_download_url(&artifact.url)?;
        Ok(artifact)
    }

    pub fn http(&self) -> &Client {
        &self.http
    }

    fn post<T: DeserializeOwned>(&self, route: &str, body: &impl Serialize) -> Result<T> {
        let response = self
            .http
            .post(format!(
                "{}/app-updates/v1/environment/{route}",
                self.base_url
            ))
            .timeout(API_TIMEOUT)
            .json(body)
            .send()
            .map_err(failure::network)?;
        let status = response.status();
        if status.is_server_error() || status.as_u16() == 429 {
            return Err(Failure::network(format!("Fusion 暂时不可用（HTTP {status}）")).into());
        }
        let envelope: ApiResponse<serde_json::Value> = response.json().map_err(failure::network)?;
        if !status.is_success() || envelope.code != 1 {
            let code = envelope
                .data
                .as_ref()
                .and_then(|v| v.get("errorCode"))
                .and_then(|v| v.as_str())
                .unwrap_or("UNKNOWN");
            let message = format!(
                "Fusion 拒绝环境请求（{code}，HTTP {status}）：{}",
                envelope.message
            );
            let retryable = matches!(
                code,
                "AGENT_STORAGE_UNAVAILABLE" | "AGENT_DOWNLOAD_UNAVAILABLE" | "AGENT_RATE_LIMITED"
            );
            return Err(Failure {
                code: if retryable { "NETWORK" } else { "PLAN" },
                message,
                retryable,
            }
            .into());
        }
        serde_json::from_value(envelope.data.context("Fusion response omitted data")?)
            .context("Fusion environment response has an incompatible format")
    }
}

pub fn validate_download_url(value: &str) -> Result<()> {
    let url = Url::parse(value).context("invalid object storage URL")?;
    let trusted = url.host_str().is_some_and(|host| {
        host == "vol-ai.iywtu.com" || (host.contains(".tos-") && host.ends_with(".volces.com"))
    });
    if !trusted
        || url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.port().is_some()
    {
        return Err(Failure::permanent("PLAN", "下载地址不是允许的 TOS HTTPS 地址").into());
    }
    Ok(())
}
