use std::sync::Arc;
use std::time::Duration;

use sea_orm::DatabaseConnection;
use serde_json::Value;

use crate::acp::delegation::listener::DelegationListener;

use super::authority::SessionContext;
use super::iyw_image::errors::{
    request_id, response_payload, transport_failure as image_transport_error, upstream_failure,
    UpstreamFailure,
};

const GATEWAY_ORIGIN: &str = "https://gateway.iyw.cn";
const FUSION_PREFIX: &str = "/iyw-fusion-api/v1";
const HTTP_TIMEOUT: Duration = Duration::from_secs(super::iyw_image::FUSION_TIMEOUT_SECONDS);
const CATALOG_TIMEOUT: Duration = Duration::from_secs(15);
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

mod image;

#[derive(Clone)]
pub(super) struct IywGatewayService {
    conn: DatabaseConnection,
    listener: Arc<DelegationListener>,
    client: reqwest::Client,
    pub(super) upload_client: reqwest::Client,
    pub(super) remote: Arc<super::remote_mcp::RemoteGateway>,
    pub(super) image_timeout: Option<Duration>,
    pub(super) image_context: Option<Arc<super::iyw_image::context::ImageContext>>,
    pub(super) image_catalog: Arc<super::iyw_image_models::cache::CatalogCache>,
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
        let upload_client = reqwest::Client::builder()
            .timeout(UPLOAD_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .build()
            .map_err(|error| format!("failed to initialize IYW upload client: {error}"))?;
        Ok(Arc::new(Self {
            remote: super::remote_mcp::RemoteGateway::new(conn.clone()),
            conn,
            listener,
            client,
            upload_client,
            image_timeout: None,
            image_context: None,
            image_catalog: Arc::default(),
        }))
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
        if self.image_context.is_some() {
            return self.image_identity().await.map(|(token, _)| token);
        }
        let token = crate::commands::iyw_account::iyw_account_access_token_core(&self.conn)
            .await
            .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))?
            .map(|token| token.expose().to_string())
            .ok_or_else(|| {
                rmcp::ErrorData::invalid_request("Sign in to iyw-claw before using IYW tools", None)
            })?;
        Ok(token)
    }

    pub(super) async fn product_context(&self) -> Result<(String, String), rmcp::ErrorData> {
        crate::commands::iyw_account::iyw_account_product_context_core(&self.conn)
            .await
            .map_err(|error| rmcp::ErrorData::invalid_request(error.to_string(), None))
    }

    pub(super) async fn post_gateway(
        &self,
        prefix: &str,
        path: &str,
        body: Value,
    ) -> Result<Value, rmcp::ErrorData> {
        self.image_io(
            "gateway",
            self.post_enveloped(&format!("{prefix}/{path}"), body),
        )
        .await
    }

    pub(super) async fn post_fusion(
        &self,
        path: &str,
        body: Value,
    ) -> Result<Value, rmcp::ErrorData> {
        self.image_submission(self.image_io("submit", self.send_fusion(path, body)))
            .await
    }

    async fn send_fusion(&self, path: &str, body: Value) -> Result<Value, rmcp::ErrorData> {
        crate::commands::iyw_account::iyw_account_ensure_can_consume_core(&self.conn)
            .await
            .map_err(|error| rmcp::ErrorData::invalid_request(error.to_string(), None))?;
        let token = self.token().await?;
        let url = crate::acp::model_gateway::endpoint(path)
            .unwrap_or_else(|| self.url(FUSION_PREFIX, path));
        let response = self
            .client
            .post(url)
            .timeout(self.image_timeout.unwrap_or(HTTP_TIMEOUT))
            .header("token", &token)
            .json(&body)
            .send()
            .await
            .map_err(|error| image_transport_error("Fusion request", &error))?;
        let status = response.status();
        let request_id = request_id(&response);
        let mut payload = response_payload(response, "Fusion response").await?;
        if !status.is_success() {
            return Err(upstream_failure(UpstreamFailure {
                source: "fusion",
                operation: path,
                status: status.as_u16(),
                payload: &payload,
                request_id,
                token: &token,
            }));
        }
        if let Some(object) = payload.as_object_mut() {
            object.insert("_iyw_request_id".into(), serde_json::json!(request_id));
        }
        Ok(payload)
    }

    pub(super) async fn get_fusion(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<Value, rmcp::ErrorData> {
        self.image_io("catalog", self.fetch_fusion(path, query))
            .await
    }

    async fn fetch_fusion(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<Value, rmcp::ErrorData> {
        let token = self.token().await?;
        let response = self
            .client
            .get(self.url(FUSION_PREFIX, path))
            .timeout(CATALOG_TIMEOUT)
            .query(query)
            .header("token", &token)
            .send()
            .await
            .map_err(|error| image_transport_error("Fusion catalog request", &error))?;
        let status = response.status();
        let request_id = request_id(&response);
        let payload = response_payload(response, "Fusion catalog response").await?;
        if !status.is_success() {
            return Err(upstream_failure(UpstreamFailure {
                source: "catalog",
                operation: path,
                status: status.as_u16(),
                payload: &payload,
                request_id,
                token: &token,
            }));
        }
        Ok(payload)
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
            .header("token", &token)
            .json(&body)
            .send()
            .await
            .map_err(|error| image_transport_error("gateway request", &error))?;
        let status = response.status();
        let request_id = request_id(&response);
        let payload = response_payload(response, "gateway response").await?;
        let accepted =
            status.is_success() && payload.get("code").and_then(Value::as_i64) == Some(1);
        if !accepted {
            return Err(upstream_failure(UpstreamFailure {
                source: "gateway",
                operation: url_path,
                status: status.as_u16(),
                payload: &payload,
                request_id,
                token: &token,
            }));
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
