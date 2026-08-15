use std::path::Path;
use std::time::{Duration, Instant};

use serde::Deserialize;

use super::command_runner::AgentBrowserCli;
use super::error::{BrowserError, BrowserErrorCode};

pub(super) const BINDING_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_BINDING_BYTES: u64 = 16 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TabBinding {
    pub target_id: String,
    pinned: bool,
}

pub(super) async fn wait_for_binding(
    cli: &AgentBrowserCli,
    session: &str,
) -> Result<TabBinding, BrowserError> {
    let path = cli.target_path(session);
    let deadline = Instant::now() + BINDING_TIMEOUT;
    while Instant::now() < deadline {
        if let Ok(binding) = read_binding(&path).await {
            return Ok(binding);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(binding_error())
}

pub(super) async fn read_binding(path: &Path) -> Result<TabBinding, BrowserError> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|_| binding_error())?;
    if metadata.len() > MAX_BINDING_BYTES {
        return Err(binding_error());
    }
    let bytes = tokio::fs::read(path).await.map_err(|_| binding_error())?;
    let binding: TabBinding = serde_json::from_slice(&bytes).map_err(|_| binding_error())?;
    if !binding.pinned || binding.target_id.is_empty() || binding.target_id.len() > 256 {
        return Err(binding_error());
    }
    Ok(binding)
}

pub(super) fn binding_error() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserRuntimeUnavailable,
        "The pinned browser tab could not be initialized",
    )
    .retryable(true)
}
