//! 远山、星河复用 ACP 分叉扩展；云舟历史分叉调用同一进程的 HTTP API。

use sacp::schema::{ForkSessionRequest, ForkSessionResponse};
use sacp::{Agent, ConnectionTo, UntypedMessage};

use crate::acp::error::AcpError;

/// Send a `session/fork` request over an existing ACP connection.
///
/// 返回原始分叉响应，由连接层负责适配器所需的恢复与会话绑定。
pub async fn fork_session(
    cx: &ConnectionTo<Agent>,
    req: ForkSessionRequest,
    opencode: Option<&crate::acp::opencode_fork::OpenCodeForkClient>,
) -> Result<ForkSessionResponse, AcpError> {
    let message_id = req
        .meta
        .as_ref()
        .and_then(|meta| meta.get("jetbrains"))
        .and_then(|meta| meta.pointer("/air/fork/messageId"))
        .and_then(serde_json::Value::as_str);
    if let (Some(client), Some(message_id)) = (opencode, message_id) {
        return client
            .fork(req.session_id.0.as_ref(), &req.cwd, message_id)
            .await
            .map(ForkSessionResponse::new);
    }
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
