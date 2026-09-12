//! ACP `session/fork` support via raw JSON-RPC messages.
//!
//! The `sacp` crate does not yet provide typed request/response types for
//! `session/fork`, so we use `UntypedMessage` (the same pattern used for
//! `session/set_config_option` in connection.rs).

use sacp::schema::{ForkSessionRequest, ForkSessionResponse};
use sacp::{Agent, ConnectionTo, UntypedMessage};

use crate::acp::error::AcpError;

/// Send a `session/fork` request over an existing ACP connection.
///
/// 返回原始分叉响应，由连接层负责适配器所需的恢复与会话绑定。
pub async fn fork_session(
    cx: &ConnectionTo<Agent>,
    req: ForkSessionRequest,
) -> Result<ForkSessionResponse, AcpError> {
    let untyped_req = UntypedMessage::new("session/fork", &req)
        .map_err(|e| AcpError::protocol(format!("Failed to build fork request: {e}")))?;

    let raw_response: serde_json::Value = cx
        .send_request_to(Agent, untyped_req)
        .block_task()
        .await
        .map_err(|e| AcpError::protocol(format!("session/fork failed: {e}")))?;

    let response: ForkSessionResponse = serde_json::from_value(raw_response)
        .map_err(|e| AcpError::protocol(format!("Failed to parse fork response: {e}")))?;

    Ok(response)
}
