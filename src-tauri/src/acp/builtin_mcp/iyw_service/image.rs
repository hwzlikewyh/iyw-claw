use std::{future::Future, sync::Arc, time::Instant};

use futures_util::future::BoxFuture;
use rmcp::ErrorData;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use super::{
    super::{authority::SessionContext, iyw_image},
    IywGatewayService,
};

impl IywGatewayService {
    pub(in crate::acp::builtin_mcp) fn generate_image<'a>(
        &'a self,
        authority: &'a SessionContext,
        arguments: Value,
        request_cancel: CancellationToken,
    ) -> BoxFuture<'a, Result<rmcp::model::CallToolResult, ErrorData>> {
        // 在 MCP 边界装箱，避免图片执行状态放大整个工具处理器的栈帧。
        Box::pin(async move {
            tracing::info!(
                connection_id = authority.connection_id(),
                stage = "authorize",
                "[iyw-image] request started"
            );
            let mut scoped = self.clone();
            scoped.image_context = Some(Arc::new(iyw_image::context::ImageContext::new(
                self.image_identity().await?.1,
                request_cancel,
                authority.cancellation().clone(),
            )));
            Box::pin(iyw_image::generate(&scoped, authority, arguments)).await
        })
    }

    pub(in crate::acp::builtin_mcp) async fn image_identity(
        &self,
    ) -> Result<(String, String), ErrorData> {
        let credentials =
            crate::commands::iyw_account::iyw_account_memory_credentials_core(&self.conn)
                .await
                .map_err(|error| ErrorData::invalid_request(error.to_string(), None))?
                .ok_or_else(|| {
                    ErrorData::invalid_request("Sign in to iyw-claw before using IYW tools", None)
                })?;
        let (token, scope) = (credentials.0.expose().to_string(), credentials.1);
        if self
            .image_context
            .as_ref()
            .is_some_and(|context| context.scope != scope)
        {
            return Err(ErrorData::invalid_request(
                "Account or workspace changed during image generation; stop this request",
                Some(
                    serde_json::json!({"code": "image_account_changed", "execution_status": "unknown", "retryable": false}),
                ),
            ));
        }
        Ok((token, scope))
    }

    pub(in crate::acp::builtin_mcp) async fn upload_bytes(
        &self,
        bytes: Vec<u8>,
        mime_type: &str,
        extension: &str,
    ) -> Result<String, ErrorData> {
        let key = format!("{mime_type}:{:x}", Sha256::digest(&bytes));
        let operation =
            super::super::iyw_upload::upload_image_bytes(self, bytes, (mime_type, extension));
        match &self.image_context {
            Some(context) => context.upload(key, operation).await,
            None => operation.await,
        }
    }

    pub(in crate::acp::builtin_mcp) fn image_io<'a, T: 'a>(
        &'a self,
        stage: &'a str,
        operation: impl Future<Output = Result<T, ErrorData>> + Send + 'a,
    ) -> BoxFuture<'a, Result<T, ErrorData>> {
        // 直接返回已装箱的执行状态，不再给 I/O 叠加内嵌的异步包装。
        match &self.image_context {
            Some(context) => context.run(stage, operation),
            None => Box::pin(operation),
        }
    }

    pub(in crate::acp::builtin_mcp) async fn post_image_gateway(
        &self,
        prefix: &str,
        path: &str,
        body: Value,
    ) -> Result<Value, ErrorData> {
        self.image_submission(self.post_gateway(prefix, path, body))
            .await
    }

    pub(super) async fn image_submission<T>(
        &self,
        operation: impl Future<Output = Result<T, ErrorData>>,
    ) -> Result<T, ErrorData> {
        let started = Instant::now();
        let _permit = match &self.image_context {
            Some(context) => Some(
                self.image_io("queue", async {
                    context.executions.acquire().await.map_err(|_| {
                        ErrorData::internal_error("Image execution queue closed", None)
                    })
                })
                .await?,
            ),
            None => None,
        };
        tracing::debug!(
            queue_ms = started.elapsed().as_millis(),
            "[iyw-image] submission slot acquired"
        );
        operation.await
    }
}
