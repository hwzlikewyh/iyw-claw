use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::{blocking::Client, Url};

use crate::failure::{self, Failure};
use crate::model::{ApiResponse, EnvironmentPlan, ResolveRequest};

const PLAN_TIMEOUT: Duration = Duration::from_secs(30);

pub struct FusionClient {
    base_url: String,
    http: Client,
}

impl FusionClient {
    pub fn new() -> Result<Self> {
        let base_url = std::env::var("IYW_CLAW_FUSION_API_BASE_URL")
            .unwrap_or_else(|_| "https://gateway.iyw.cn/iyw-fusion-api".to_string());
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(20 * 60))
            .build()
            .context("create Fusion HTTP client")?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http,
        })
    }

    pub fn resolve(&self, request: &ResolveRequest) -> Result<EnvironmentPlan> {
        crate::retry::run("环境计划", || self.resolve_once(request))
    }

    fn resolve_once(&self, request: &ResolveRequest) -> Result<EnvironmentPlan> {
        let url = format!("{}/app-updates/v1/environment/resolve", self.base_url);
        let response = self
            .http
            .post(url)
            .timeout(PLAN_TIMEOUT)
            .json(request)
            .send()
            .map_err(failure::network)?;
        let status = response.status();
        if status.is_server_error() || status.as_u16() == 429 {
            return Err(Failure::network(format!("Fusion 暂时不可用（HTTP {status}）")).into());
        }
        let body = response
            .json::<ApiResponse<serde_json::Value>>()
            .map_err(failure::network)?;
        if !status.is_success() || body.code != 1 {
            let code = body
                .data
                .as_ref()
                .and_then(|value| value.get("errorCode"))
                .and_then(|value| value.as_str())
                .unwrap_or("UNKNOWN");
            return Err(Failure::permanent(
                "PLAN",
                format!(
                    "Fusion 环境计划失败（{code}，HTTP {status}）：{}",
                    body.message
                ),
            )
            .into());
        }
        let plan =
            serde_json::from_value(body.data.context("Fusion environment plan omitted data")?)
                .context("Fusion environment plan has an incompatible format")?;
        self.validate_download_urls(&plan)?;
        Ok(plan)
    }

    pub fn http(&self) -> &Client {
        &self.http
    }

    fn validate_download_urls(&self, plan: &EnvironmentPlan) -> Result<()> {
        let fusion = Url::parse(&self.base_url).context("parse Fusion base URL")?;
        for action in &plan.actions {
            if action.action == "keep" {
                continue;
            }
            let url = Url::parse(&action.artifact.url).context("parse TOS download URL")?;
            if url.scheme() != "https"
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.fragment().is_some()
                || url.host_str() == fusion.host_str()
            {
                bail!("environment artifact must use a direct HTTPS object-storage URL")
            }
        }
        Ok(())
    }
}
