//! Explicit compile probe for OpenAI's in-process App Server client.
//!
//! This package is intentionally outside the default harness package. Cargo
//! therefore does not resolve or fetch the large Codex workspace during normal
//! `iyw-codex-harness` checks.

use codex_app_server_client::{InProcessAppServerClient, InProcessClientStartArgs};
use codex_app_server_protocol::{
    ClientNotification, ClientRequest, JSONRPCErrorError, RequestId,
    Result as JsonRpcResult,
};

/// Opaque probe type used to ensure the locked client crate remains linkable.
pub struct UpstreamClientProbe {
    client: InProcessAppServerClient,
}

impl UpstreamClientProbe {
    /// Keep the upstream client private until the ACP bridge contract is ready.
    pub(crate) fn from_client(client: InProcessAppServerClient) -> Self {
        Self { client }
    }

    pub(crate) fn client(&self) -> &InProcessAppServerClient {
        &self.client
    }
}

/// Keep the startup argument shape visible to the explicit compile boundary.
pub fn preserve_start_args(args: InProcessClientStartArgs) -> InProcessClientStartArgs {
    args
}

/// Compile-check the complete in-process client surface used by the future
/// ACP bridge without starting a Codex runtime in the probe itself.
pub async fn exercise_client_surface(
    mut client: InProcessAppServerClient,
    request: ClientRequest,
    typed_request: ClientRequest,
    notification: ClientNotification,
    request_id: RequestId,
    result: JsonRpcResult,
    error: JSONRPCErrorError,
) {
    let _ = client.request(request).await;
    let _ = client
        .request_typed::<serde_json::Value>(typed_request)
        .await;
    let _ = client.notify(notification).await;
    let _ = client.next_event().await;
    let _ = client
        .resolve_server_request(request_id.clone(), result.clone())
        .await;
    let _ = client.reject_server_request(request_id, error).await;
    let _ = client.shutdown().await;
}
