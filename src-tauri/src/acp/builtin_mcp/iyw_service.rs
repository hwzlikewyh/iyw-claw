use std::sync::Arc;
use std::time::Duration;

use reqwest::Url;
use sea_orm::DatabaseConnection;
use serde_json::{json, Value};

use crate::acp::delegation::listener::DelegationListener;

use super::authority::SessionContext;

const GATEWAY_ORIGIN: &str = "https://gateway.iyw.cn";
const FUSION_PREFIX: &str = "/iyw-fusion-api/v1";
const HTTP_TIMEOUT: Duration = Duration::from_secs(super::iyw_image::FUSION_TIMEOUT_SECONDS);

#[derive(Clone)]
pub(super) struct IywGatewayService {
    conn: DatabaseConnection,
    listener: Arc<DelegationListener>,
    client: reqwest::Client,
    pub(super) remote: Arc<super::remote_mcp::RemoteGateway>,
    pub(super) image_timeout: Option<Duration>,
}

impl IywGatewayService {
    pub(super) fn new(
        conn: DatabaseConnection,
        listener: Arc<DelegationListener>,
    ) -> Result<Arc<Self>, String> {
        let client = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .user_agent("iyw-claw")
            .build()
            .map_err(|error| format!("failed to initialize IYW gateway client: {error}"))?;
        Ok(Arc::new(Self {
            remote: super::remote_mcp::RemoteGateway::new(conn.clone()),
            conn,
            listener,
            client,
            image_timeout: None,
        }))
    }

    pub(super) async fn generate_image(
        &self,
        authority: &SessionContext,
        arguments: Value,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        super::iyw_image::generate(self, authority, arguments).await
    }

    pub(super) async fn list_image_models(
        &self,
        arguments: Value,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        super::iyw_image_models::list(self, arguments).await
    }

    pub(super) async fn search_knowledge(
        &self,
        arguments: Value,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        super::iyw_knowledge::search(self, arguments).await
    }

    pub(super) async fn token(&self) -> Result<String, rmcp::ErrorData> {
        crate::commands::iyw_account::iyw_account_access_token_core(&self.conn)
            .await
            .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))?
            .map(|token| token.expose().to_string())
            .ok_or_else(|| {
                rmcp::ErrorData::invalid_request("Sign in to iyw-claw before using IYW tools", None)
            })
    }

    pub(super) async fn post_gateway(
        &self,
        prefix: &str,
        path: &str,
        body: Value,
    ) -> Result<Value, rmcp::ErrorData> {
        self.post_enveloped(&format!("{prefix}/{path}"), body).await
    }

    pub(super) async fn post_fusion(
        &self,
        path: &str,
        body: Value,
    ) -> Result<Value, rmcp::ErrorData> {
        let token = self.token().await?;
        let response = self
            .client
            .post(self.url(FUSION_PREFIX, path))
            .timeout(self.image_timeout.unwrap_or(HTTP_TIMEOUT))
            .header("token", token)
            .json(&body)
            .send()
            .await
            .map_err(|error| image_transport_error("Fusion request", &error))?;
        let status = response.status();
        let payload = response
            .json::<Value>()
            .await
            .map_err(|error| image_transport_error("Fusion response", &error))?;
        if !status.is_success() {
            tracing::warn!(
                operation = path,
                http_status = status.as_u16(),
                "[iyw-image] upstream Fusion generation failed"
            );
            return Err(rmcp::ErrorData::internal_error(
                "IYW Fusion image request failed upstream; this is not local parameter validation. Check any uncertain submission state before retrying or choosing another suitable service",
                Some(json!({"status": status.as_u16()})),
            ));
        }
        Ok(payload)
    }

    pub(super) async fn post_fusion_multipart(
        &self,
        path: &str,
        form: reqwest::multipart::Form,
    ) -> Result<Value, rmcp::ErrorData> {
        let token = self.token().await?;
        let response = self
            .client
            .post(self.url(FUSION_PREFIX, path))
            .timeout(self.image_timeout.unwrap_or(HTTP_TIMEOUT))
            .header("token", token)
            .multipart(form)
            .send()
            .await
            .map_err(|error| image_transport_error("Fusion edit request", &error))?;
        let status = response.status();
        let payload = response
            .json::<Value>()
            .await
            .map_err(|error| image_transport_error("Fusion edit response", &error))?;
        if !status.is_success() {
            tracing::warn!(
                operation = path,
                http_status = status.as_u16(),
                "[iyw-image] upstream Fusion edit failed"
            );
            return Err(rmcp::ErrorData::internal_error(
                "IYW Fusion image edit request failed upstream; this does not prove all image editing routes are unavailable. Check any uncertain submission state before retrying or choosing another suitable service",
                Some(json!({"status": status.as_u16()})),
            ));
        }
        Ok(payload)
    }

    pub(super) async fn download_image(&self, source: &str) -> Result<Vec<u8>, rmcp::ErrorData> {
        let url = Url::parse(source)
            .map_err(|_| rmcp::ErrorData::invalid_params("image URL is invalid", None))?;
        if url.scheme() != "https" || url.host_str().is_none() || !url.username().is_empty() {
            return Err(rmcp::ErrorData::invalid_params(
                "image URL must be credential-free HTTPS",
                None,
            ));
        }
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|error| image_transport_error("download", &error))?;
        if !response.status().is_success() {
            return Err(rmcp::ErrorData::invalid_params(
                "image URL could not be downloaded",
                Some(json!({"status": response.status().as_u16()})),
            ));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|error| image_transport_error("download body", &error))?;
        if bytes.len() > crate::acp::delegation::image_loader::MAX_IMAGE_BYTES {
            return Err(rmcp::ErrorData::invalid_params(
                "image exceeds the 20 MiB limit",
                None,
            ));
        }
        Ok(bytes.to_vec())
    }

    pub(super) async fn get_fusion(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<Value, rmcp::ErrorData> {
        let token = self.token().await?;
        let response = self
            .client
            .get(self.url(FUSION_PREFIX, path))
            .query(query)
            .header("token", token)
            .send()
            .await
            .map_err(|error| image_transport_error("Fusion catalog request", &error))?;
        let status = response.status();
        let payload = response
            .json::<Value>()
            .await
            .map_err(|error| image_transport_error("Fusion catalog response", &error))?;
        if !status.is_success() {
            tracing::warn!(
                operation = path,
                http_status = status.as_u16(),
                "[iyw-image] upstream Fusion catalog lookup failed"
            );
            return Err(rmcp::ErrorData::internal_error(
                "IYW Fusion model request failed upstream; this lookup supplies no model and does not establish that a suitable platform operation is unavailable",
                Some(json!({"status": status.as_u16()})),
            ));
        }
        Ok(payload)
    }

    pub(super) async fn upload_bytes(
        &self,
        bytes: Vec<u8>,
        mime_type: &str,
        extension: &str,
    ) -> Result<String, rmcp::ErrorData> {
        super::iyw_upload::upload_image_bytes(self, bytes, (mime_type, extension)).await
    }

    pub(super) async fn deliver(
        &self,
        authority: &SessionContext,
        urls: &[String],
        display: bool,
        register_artifact: bool,
    ) -> Value {
        super::iyw_delivery::deliver(self, authority, urls, display, register_artifact).await
    }

    pub(super) fn listener(&self) -> &Arc<DelegationListener> {
        &self.listener
    }

    async fn post_enveloped(&self, url_path: &str, body: Value) -> Result<Value, rmcp::ErrorData> {
        let token = self.token().await?;
        let response = self
            .client
            .post(self.url("", url_path))
            .timeout(self.image_timeout.unwrap_or(HTTP_TIMEOUT))
            .header("token", token)
            .json(&body)
            .send()
            .await
            .map_err(|error| image_transport_error("gateway request", &error))?;
        let status = response.status();
        let payload = response
            .json::<Value>()
            .await
            .map_err(|error| image_transport_error("gateway response", &error))?;
        let accepted =
            status.is_success() && payload.get("code").and_then(Value::as_i64) == Some(1);
        if !accepted {
            tracing::warn!(
                operation = url_path,
                http_status = status.as_u16(),
                business_code = payload.get("code").and_then(|value| value.as_i64()),
                "[iyw-image] upstream gateway response rejected the operation"
            );
            return Err(rmcp::ErrorData::internal_error(
                "IYW gateway request was rejected upstream; this is not local parameter validation or proof that other image routes are unsupported. Do not replay unchanged arguments. Resolve any uncertain task state, then evaluate a suitable platform operation or model edit for the original goal",
                Some(json!({"status": status.as_u16(), "code": payload.get("code")})),
            ));
        }
        Ok(payload.get("data").cloned().unwrap_or(payload))
    }

    fn url(&self, prefix: &str, path: &str) -> String {
        let prefix = prefix.trim_matches('/');
        let path = path.trim_matches('/');
        if prefix.is_empty() {
            format!("{GATEWAY_ORIGIN}/{path}")
        } else {
            format!("{GATEWAY_ORIGIN}/{prefix}/{path}")
        }
    }
}

fn image_transport_error(stage: &'static str, error: &reqwest::Error) -> rmcp::ErrorData {
    tracing::warn!(
        stage,
        timeout = error.is_timeout(),
        connect = error.is_connect(),
        status = error.status().map(|status| status.as_u16()),
        "[iyw-image] image transport failed"
    );
    rmcp::ErrorData::internal_error(format!("IYW image {stage} failed"), None)
}
