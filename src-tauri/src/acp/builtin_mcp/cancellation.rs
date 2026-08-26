use std::time::Duration;

use rmcp::ErrorData;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::acp::delegation::companion::{CompanionBridge, PreparedDirectCall, SpawnResult};

const COMMITTING_CALL_SETTLE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy)]
pub(super) enum CallCancellationPolicy {
    Cancel,
    CompleteWithUnknownEffect,
}

impl CallCancellationPolicy {
    pub(super) fn for_call(tool_name: &str, arguments: &Value) -> Self {
        if mutation_may_outlive_request(tool_name, arguments) {
            Self::CompleteWithUnknownEffect
        } else {
            Self::Cancel
        }
    }

    pub(super) fn error_after_call(self, message: &'static str) -> ErrorData {
        match self {
            Self::Cancel => ErrorData::invalid_request(message, None),
            Self::CompleteWithUnknownEffect => effect_unknown(message),
        }
    }
}

pub(super) fn post_ack_error(
    delivery_ack_committed: bool,
    fallback: ErrorData,
    message: &'static str,
) -> ErrorData {
    if delivery_ack_committed {
        return CallCancellationPolicy::CompleteWithUnknownEffect.error_after_call(message);
    }
    fallback
}

pub(super) async fn run_call_with_cancellation(
    bridge: &CompanionBridge,
    request_id: Value,
    tool_name: String,
    arguments: Value,
    request_cancel: CancellationToken,
    authority_cancel: CancellationToken,
    policy: CallCancellationPolicy,
) -> Result<SpawnResult, ErrorData> {
    if request_cancel.is_cancelled() {
        return Err(ErrorData::invalid_request("MCP request cancelled", None));
    }
    if authority_cancel.is_cancelled() {
        return Err(ErrorData::invalid_request("MCP authority revoked", None));
    }
    match policy {
        CallCancellationPolicy::Cancel => {
            run_cancelable(
                bridge,
                request_id,
                tool_name,
                arguments,
                request_cancel,
                authority_cancel,
            )
            .await
        }
        CallCancellationPolicy::CompleteWithUnknownEffect => {
            run_committing(
                bridge,
                request_id,
                tool_name,
                arguments,
                request_cancel,
                authority_cancel,
            )
            .await
        }
    }
}

async fn run_cancelable(
    bridge: &CompanionBridge,
    request_id: Value,
    tool_name: String,
    arguments: Value,
    request_cancel: CancellationToken,
    authority_cancel: CancellationToken,
) -> Result<SpawnResult, ErrorData> {
    let call = bridge.direct_call(request_id.clone(), tool_name, arguments);
    tokio::pin!(call);
    tokio::select! {
        biased;
        _ = request_cancel.cancelled() => {
            let _ = bridge.cancel(request_id, Some("MCP request cancelled".into())).await;
            Err(ErrorData::invalid_request("MCP request cancelled", None))
        }
        _ = authority_cancel.cancelled() => {
            let _ = bridge.cancel(request_id, Some("MCP authority revoked".into())).await;
            Err(ErrorData::invalid_request("MCP authority revoked", None))
        }
        result = &mut call => Ok(result),
    }
}

async fn run_committing(
    bridge: &CompanionBridge,
    request_id: Value,
    tool_name: String,
    arguments: Value,
    request_cancel: CancellationToken,
    authority_cancel: CancellationToken,
) -> Result<SpawnResult, ErrorData> {
    let prepared = bridge
        .prepare_direct_call(request_id.clone(), tool_name, arguments)
        .await;
    let call_cancel = CancellationToken::new();
    let _cancel_on_drop = CancelOnDrop(call_cancel.clone());
    let mut call = tokio::spawn(run_committing_call(
        bridge.clone(),
        request_id,
        prepared,
        call_cancel.clone(),
    ));
    tokio::select! {
        biased;
        _ = request_cancel.cancelled() => {
            call_cancel.cancel();
            settle_committing_call(&mut call).await;
            Err(effect_unknown("MCP request cancelled after dispatch"))
        },
        _ = authority_cancel.cancelled() => {
            call_cancel.cancel();
            settle_committing_call(&mut call).await;
            Err(effect_unknown("MCP authority revoked after dispatch"))
        },
        result = &mut call => match result {
            Ok(result) if result.response.is_none() => {
                Err(effect_unknown("MCP mutation result unavailable after dispatch"))
            }
            Ok(result) => Ok(result),
            Err(error) => Err(effect_unknown(format!(
                "MCP mutation task failed after dispatch: {error}"
            ))),
        },
    }
}

async fn run_committing_call(
    bridge: CompanionBridge,
    request_id: Value,
    prepared: PreparedDirectCall,
    cancellation: CancellationToken,
) -> SpawnResult {
    let call = prepared.run();
    tokio::pin!(call);
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => {
            let _ = bridge
                .cancel(request_id, Some("MCP committing call cancelled".into()))
                .await;
            settle_direct_call(&mut call).await
        }
        result = &mut call => result,
    }
}

async fn settle_direct_call<F>(call: &mut std::pin::Pin<&mut F>) -> SpawnResult
where
    F: std::future::Future<Output = SpawnResult>,
{
    if let Ok(result) = tokio::time::timeout(COMMITTING_CALL_SETTLE_TIMEOUT, call).await {
        return result;
    }
    tracing::warn!(
        target: "builtin_mcp",
        timeout_ms = COMMITTING_CALL_SETTLE_TIMEOUT.as_millis(),
        "mutation call did not settle after cancellation; dropping local waiter"
    );
    SpawnResult {
        response: None,
        after_relay: None,
    }
}

async fn settle_committing_call(call: &mut tokio::task::JoinHandle<SpawnResult>) {
    if tokio::time::timeout(
        COMMITTING_CALL_SETTLE_TIMEOUT + Duration::from_secs(1),
        &mut *call,
    )
    .await
    .is_err()
    {
        tracing::warn!(
            target: "builtin_mcp",
            timeout_ms = COMMITTING_CALL_SETTLE_TIMEOUT.as_millis(),
            "mutation call did not settle after cancellation; aborting local waiter"
        );
        call.abort();
        let _ = call.await;
    }
}

struct CancelOnDrop(CancellationToken);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

fn effect_unknown(message: impl Into<String>) -> ErrorData {
    ErrorData::invalid_request(
        message.into(),
        Some(json!({ "effectMayHaveOccurred": true })),
    )
}

fn mutation_may_outlive_request(tool_name: &str, arguments: &Value) -> bool {
    matches!(
        tool_name,
        "delegate_to_agent"
            | "cancel_delegation"
            | "present_task_files"
            | "transcribe_audio"
            | "transcribe_audio_flash"
            | "show_image"
            | "analyze_image"
            | "append_user_memory"
            | "propose_user_memory"
            | "create_scheduled_task"
            | "update_scheduled_task"
            | "delete_scheduled_task"
            | "save_message_channel"
            | "delete_message_channel"
            | "operate_message_channel"
            | "send_channel_messages"
    ) || channel_operation_mutates(tool_name, arguments)
        || browser_operation_mutates(tool_name)
}

fn browser_operation_mutates(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "browser_open"
            | "browser_click"
            | "browser_fill"
            | "browser_press"
            | "browser_scroll"
            | "browser_screenshot"
            | "browser_close_tab"
            | "browser_request_user_action"
            | "browser_present"
            | "browser_close_window"
    )
}

fn channel_operation_mutates(tool_name: &str, arguments: &Value) -> bool {
    match tool_name {
        "manage_channel_credential" => {
            arguments.get("operation").and_then(Value::as_str) != Some("status")
        }
        "manage_channel_settings" => {
            arguments.get("operation").and_then(Value::as_str) != Some("get")
        }
        _ => false,
    }
}
