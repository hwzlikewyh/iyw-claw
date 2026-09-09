use sacp::schema::{InitializeRequest, InitializeResponse};
use sacp::{Agent, ConnectionTo, UntypedMessage};

pub(super) async fn send(
    connection: &ConnectionTo<Agent>,
    request: InitializeRequest,
) -> Result<(InitializeResponse, bool), sacp::Error> {
    // 当前 schema 未启用实验 close 类型，直接读取实际协商结果，避免按版本猜能力。
    let request = UntypedMessage::new("initialize", request)?;
    let raw = connection
        .send_request_to(Agent, request)
        .block_task()
        .await?;
    let supports_close = raw
        .pointer("/agentCapabilities/sessionCapabilities/close")
        .is_some_and(serde_json::Value::is_object);
    let response = serde_json::from_value(raw).map_err(sacp::util::internal_error)?;
    Ok((response, supports_close))
}
