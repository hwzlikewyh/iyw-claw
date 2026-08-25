use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Child;
use tokio_util::sync::CancellationToken;

use super::error::{BrowserError, BrowserErrorCode};

const MAX_COMMAND_OUTPUT: usize = 4 * 1024 * 1024;
const OUTPUT_DRAIN_IDLE_TIMEOUT: Duration = Duration::from_millis(200);

pub(super) struct CollectedOutput {
    pub(super) success: bool,
    pub(super) stdout: Vec<u8>,
    pub(super) stderr: Vec<u8>,
}

pub(super) async fn collect_output(mut child: Child) -> Result<CollectedOutput, BrowserError> {
    let stdout = child.stdout.take().ok_or_else(unavailable_error)?;
    let stderr = child.stderr.take().ok_or_else(unavailable_error)?;
    let child_exited = CancellationToken::new();
    let wait_token = child_exited.clone();
    let wait = async move {
        let status = child.wait().await;
        wait_token.cancel();
        status
    };
    let (status, stdout, stderr) = tokio::join!(
        wait,
        read_bounded(stdout, child_exited.clone()),
        read_bounded(stderr, child_exited)
    );
    Ok(CollectedOutput {
        success: status.map_err(|_| unavailable_error())?.success(),
        stdout: stdout?,
        stderr: stderr?,
    })
}

async fn read_bounded(
    mut reader: impl AsyncRead + Unpin,
    child_exited: CancellationToken,
) -> Result<Vec<u8>, BrowserError> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        tokio::select! {
            read = reader.read(&mut buffer) => {
                let read = read.map_err(|_| unavailable_error())?;
                if read == 0 {
                    return Ok(output);
                }
                extend_bounded(&mut output, &buffer[..read])?;
            }
            _ = child_exited.cancelled() => {
                drain_after_exit(&mut reader, &mut output, &mut buffer).await?;
                return Ok(output);
            }
        }
    }
}

async fn drain_after_exit(
    reader: &mut (impl AsyncRead + Unpin),
    output: &mut Vec<u8>,
    buffer: &mut [u8],
) -> Result<(), BrowserError> {
    loop {
        match tokio::time::timeout(OUTPUT_DRAIN_IDLE_TIMEOUT, reader.read(buffer)).await {
            Ok(Ok(0)) | Err(_) => return Ok(()),
            Ok(Ok(read)) => extend_bounded(output, &buffer[..read])?,
            Ok(Err(_)) => return Err(unavailable_error()),
        }
    }
}

fn extend_bounded(output: &mut Vec<u8>, chunk: &[u8]) -> Result<(), BrowserError> {
    if output.len().saturating_add(chunk.len()) > MAX_COMMAND_OUTPUT {
        return Err(BrowserError::new(
            BrowserErrorCode::BrowserInternal,
            "The browser controller returned too much data",
        ));
    }
    output.extend_from_slice(chunk);
    Ok(())
}

pub(super) fn parse_output(
    success: bool,
    stdout: &[u8],
    stderr: &[u8],
    session: &str,
    operation: &str,
) -> Result<Value, BrowserError> {
    if stdout.len() > MAX_COMMAND_OUTPUT || stderr.len() > MAX_COMMAND_OUTPUT {
        return Err(BrowserError::new(
            BrowserErrorCode::BrowserInternal,
            "The browser controller returned too much data",
        ));
    }
    let value: Value = serde_json::from_slice(stdout).map_err(|_| unavailable_error())?;
    if success && value.get("success").and_then(Value::as_bool) != Some(false) {
        return Ok(value);
    }
    let code = value
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let message = value
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let error = map_cli_error(code, message);
    tracing::warn!(
        target: "iyw_claw_browser",
        session,
        operation,
        controller_error_code = known_controller_error_code(code).unwrap_or("unknown"),
        error_code = ?error.code,
        retryable = error.retryable,
        "browser controller rejected operation"
    );
    Err(error)
}

fn map_cli_error(code: &str, message: &str) -> BrowserError {
    let lower = message.to_ascii_lowercase();
    let (mapped, summary, retryable) = match code {
        "tab_gone" => (
            BrowserErrorCode::BrowserTabGone,
            "The pinned browser tab is gone",
            true,
        ),
        "dialog_pending" => (
            BrowserErrorCode::BrowserDialogPending,
            "A browser dialog is blocking the operation",
            true,
        ),
        "stale_ref" => stale_reference_error(),
        "invalid_selector" | "selector_not_found" | "selector_ambiguous" => selector_error(),
        "operation_timeout" | "timeout" | "timed_out" => timeout_error(),
        _ if code.is_empty() && lower.starts_with("unknown ref:") => stale_reference_error(),
        _ if code.is_empty() && is_locator_error(&lower) => selector_error(),
        _ if code.is_empty()
            && (lower.contains("is covered by")
                || lower.contains("another element is covering")) =>
        {
            (
                BrowserErrorCode::BrowserControlChanged,
                "Another element is covering the browser target",
                true,
            )
        }
        _ if code.is_empty() && (lower.contains("timed out") || lower.contains("timeout")) => {
            timeout_error()
        }
        _ => (
            BrowserErrorCode::BrowserRuntimeUnavailable,
            "The browser controller rejected the operation",
            true,
        ),
    };
    let message = if let Some(detail) = known_controller_error_code(code) {
        format!("{summary} (controller code: {detail})")
    } else {
        summary.to_string()
    };
    BrowserError::new(mapped, message).retryable(retryable)
}

fn stale_reference_error() -> (BrowserErrorCode, &'static str, bool) {
    (
        BrowserErrorCode::BrowserSnapshotStale,
        "The browser snapshot reference is stale",
        true,
    )
}

fn selector_error() -> (BrowserErrorCode, &'static str, bool) {
    (
        BrowserErrorCode::BrowserInvalidArgument,
        "The browser target could not be resolved uniquely",
        false,
    )
}

fn timeout_error() -> (BrowserErrorCode, &'static str, bool) {
    (
        BrowserErrorCode::BrowserOperationTimeout,
        "The browser controller operation timed out",
        true,
    )
}

fn is_locator_error(message: &str) -> bool {
    message.starts_with("element not found")
        || message.starts_with("no element found")
        || message.contains("invalid selector")
        || message.contains("queryselector")
        || message.contains("strict mode violation")
        || message.starts_with("element matched multiple")
}

fn known_controller_error_code(code: &str) -> Option<&str> {
    match code {
        "tab_gone" | "dialog_pending" | "stale_ref" | "invalid_selector" | "selector_not_found"
        | "selector_ambiguous" | "operation_timeout" | "timeout" | "timed_out" => Some(code),
        _ => None,
    }
}

pub(super) fn unavailable_error() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserRuntimeUnavailable,
        "The browser controller is unavailable",
    )
    .retryable(true)
}

pub(super) fn cancelled_error() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserCancelled,
        "The browser operation was cancelled",
    )
}
