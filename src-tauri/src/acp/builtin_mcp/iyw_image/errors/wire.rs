use serde_json::{json, Value};

pub(in crate::acp::builtin_mcp) fn transport_failure(
    stage: &'static str,
    error: &reqwest::Error,
) -> rmcp::ErrorData {
    let lookup = stage.starts_with("Fusion catalog");
    let execution_status = if lookup { "not_started" } else { "unknown" };
    tracing::warn!(
        stage,
        timeout = error.is_timeout(),
        connect = error.is_connect(),
        status = error.status().map(|status| status.as_u16()),
        "[iyw-image] image transport failed"
    );
    rmcp::ErrorData::internal_error(
        format!(
            "IYW image {stage} failed; {}",
            if lookup {
                "no image generation was started"
            } else {
                "resolve the original submission before retrying"
            }
        ),
        Some(
            json!({"stage": stage, "timeout": error.is_timeout(), "connect": error.is_connect(),
            "status": error.status().map(|status| status.as_u16()), "execution_status": execution_status, "retryable": false}),
        ),
    )
}

pub(in crate::acp::builtin_mcp) async fn response_payload(
    response: reqwest::Response,
    stage: &'static str,
) -> Result<Value, rmcp::ErrorData> {
    let status = response.status().as_u16();
    let request_id = super::request_id(&response);
    response.json::<Value>().await.map_err(|error| {
        let mut failure = transport_failure(stage, &error);
        if let Some(data) = &mut failure.data {
            data["status"] = json!(status);
            data["request_id"] = json!(request_id);
        }
        failure
    })
}
