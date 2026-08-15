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
    Err(map_cli_error(code))
}

fn map_cli_error(code: &str) -> BrowserError {
    let mapped = match code {
        "tab_gone" => BrowserErrorCode::BrowserTabGone,
        "dialog_pending" => BrowserErrorCode::BrowserDialogPending,
        _ => BrowserErrorCode::BrowserRuntimeUnavailable,
    };
    BrowserError::new(mapped, "The browser controller rejected the operation").retryable(true)
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
