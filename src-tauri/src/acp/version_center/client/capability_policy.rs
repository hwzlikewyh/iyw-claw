use reqwest::Method;
use sea_orm::DatabaseConnection;

use super::{AgentPlatformClient, Envelope};
use crate::acp::capability_policy::{
    CapabilityPolicyError, CapabilityPolicyFetcher, CapabilityPolicySnapshot, PolicyFetch,
};

const CAPABILITY_POLICY_PATH: &str = "/agent-platforms/v1/capability-policy";

#[derive(Clone)]
pub struct CapabilityPolicyHttpFetcher {
    conn: DatabaseConnection,
}

impl CapabilityPolicyHttpFetcher {
    pub fn new(conn: DatabaseConnection) -> Self {
        Self { conn }
    }
}

#[async_trait::async_trait]
impl CapabilityPolicyFetcher for CapabilityPolicyHttpFetcher {
    async fn fetch(&self, etag: Option<&str>) -> Result<PolicyFetch, CapabilityPolicyError> {
        let mut request =
            AgentPlatformClient::request(&self.conn, Method::GET, CAPABILITY_POLICY_PATH)
                .await
                .map_err(CapabilityPolicyError::transport)?;
        if let Some(etag) = etag.filter(|value| !value.trim().is_empty()) {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        let response = request
            .send()
            .await
            .map_err(CapabilityPolicyError::transport)?;
        decode_policy_response(response).await
    }
}

async fn decode_policy_response(
    response: reqwest::Response,
) -> Result<PolicyFetch, CapabilityPolicyError> {
    let status = response.status();
    if status == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(PolicyFetch::NotModified);
    }
    if !status.is_success() {
        return Err(CapabilityPolicyError::transport(format!(
            "Agent platform returned HTTP {status}"
        )));
    }
    let etag = response
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string);
    let bytes = response
        .bytes()
        .await
        .map_err(CapabilityPolicyError::transport)?;
    let envelope =
        serde_json::from_slice::<Envelope>(&bytes).map_err(CapabilityPolicyError::transport)?;
    if envelope.code != 1 {
        return Err(CapabilityPolicyError::transport(format!(
            "Agent platform rejected capability policy: code={}",
            envelope.code
        )));
    }
    let snapshot = serde_json::from_value::<CapabilityPolicySnapshot>(envelope.data)
        .map_err(CapabilityPolicyError::transport)?;
    Ok(PolicyFetch::Updated { snapshot, etag })
}
