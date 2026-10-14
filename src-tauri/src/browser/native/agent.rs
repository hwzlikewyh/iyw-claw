use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::super::error::{BrowserError, BrowserErrorCode};
use super::{NativeBrowser, NativeExecution};

const WAIT_INTERVAL: Duration = Duration::from_millis(200);
const AGENT_BUNDLE: &str = include_str!("agent.bundle.js");
const COMMANDS: &str = include_str!("commands.js");
static REF_SEQUENCE: AtomicU64 = AtomicU64::new(1);

impl NativeBrowser {
    pub(in crate::browser) async fn wait_loaded(
        &self,
        id: &str,
        url: &str,
        cancellation: CancellationToken,
    ) -> Result<Value, BrowserError> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let state = self
                .request(
                    super::request::NativeRequest::new(id, "state", json!({})).within(
                        deadline.saturating_duration_since(Instant::now()),
                        cancellation.clone(),
                    ),
                )
                .await?;
            if state["loading"] != true && (url == "about:blank" || state["url"] != "about:blank") {
                return Ok(state);
            }
            if Instant::now() >= deadline {
                return Err(timeout_error("The browser page did not finish loading"));
            }
            tokio::select! {
                _ = cancellation.cancelled() => return Err(cancelled_error()),
                _ = tokio::time::sleep(WAIT_INTERVAL) => {}
            }
        }
    }

    pub(in crate::browser) async fn execute_cli(
        &self,
        execution: NativeExecution<'_>,
    ) -> Result<Value, BrowserError> {
        let command = execution.args.first().map(String::as_str).unwrap_or("");
        #[cfg(target_os = "windows")]
        if matches!(command, "click" | "dblclick" | "hover") {
            let mut locate = vec!["pointer".to_string()];
            locate.extend_from_slice(execution.args);
            let point = self.script(&execution, &locate).await?;
            let value = self.request(execution.request("pointer", point)).await?;
            return Ok(json!({"success":true,"data":value}));
        }
        let data = match command {
            "wait" => self.wait(&execution).await?,
            "screenshot" => self.screenshot(&execution).await?,
            "back" | "forward" | "reload" => {
                self.request(execution.request(command, json!({}))).await?
            }
            "cookies" => {
                self.request(execution.request("cookies", json!({"args":execution.args})))
                    .await?
            }
            "set"
                if execution
                    .args
                    .get(1)
                    .is_some_and(|value| value == "viewport") =>
            {
                return Err(super::native_error(
                    "Native viewport follows its browser window".into(),
                ))
            }
            _ => self.script(&execution, execution.args).await?,
        };
        Ok(json!({"success":true,"data":data}))
    }

    async fn screenshot(&self, execution: &NativeExecution<'_>) -> Result<Value, BrowserError> {
        if execution
            .args
            .iter()
            .any(|value| matches!(value.as_str(), "--annotate" | "--full"))
        {
            return Err(super::native_error(
                "This screenshot option is unavailable in the lightweight browser".into(),
            ));
        }
        let captured = self
            .request(execution.request("capture", json!({})))
            .await?;
        let _guard = self.screenshot_lock.lock().await;
        save_screenshot(captured).await
    }

    async fn script(
        &self,
        execution: &NativeExecution<'_>,
        args: &[String],
    ) -> Result<Value, BrowserError> {
        let state = self.request(execution.request("state", json!({}))).await?;
        let epoch = format!("{}.{}", execution.id, state["epoch"].as_u64().unwrap_or(0));
        let prefix = REF_SEQUENCE.fetch_add(1, Ordering::Relaxed).to_string();
        let expression = format!("(() => {{ if (!globalThis.__codegAgent) {{ {AGENT_BUNDLE} }} if (!globalThis.__iywNativeBrowser) {{ {COMMANDS} }} return globalThis.__iywNativeBrowser.execute({},{},{}); }})()", json!(args), json!(epoch), json!(prefix));
        self.request(execution.request("eval", json!({"expression":expression})))
            .await
    }

    async fn wait(&self, execution: &NativeExecution<'_>) -> Result<Value, BrowserError> {
        let target = execution.args.get(1).map(String::as_str).unwrap_or("1000");
        if target.starts_with('-') {
            return Err(super::native_error("This wait option is unavailable in the lightweight browser; use a selector or milliseconds".into()));
        }
        if let Ok(milliseconds) = target.parse::<u64>() {
            return timed_wait(milliseconds, execution.timeout, &execution.cancellation).await;
        }
        let deadline = Instant::now() + execution.timeout;
        let query = vec!["exists".to_string(), target.to_string()];
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(timeout_error("The browser wait condition was not met"));
            }
            let remaining_call = NativeExecution {
                id: execution.id,
                args: &query,
                timeout: remaining,
                cancellation: execution.cancellation.clone(),
            };
            if self.script(&remaining_call, &query).await?["exists"] == true {
                return Ok(json!({"matched":true}));
            }
            tokio::select! {
                _ = execution.cancellation.cancelled() => return Err(cancelled_error()),
                _ = tokio::time::sleep(WAIT_INTERVAL.min(remaining)) => {}
            }
        }
    }
}

async fn save_screenshot(value: Value) -> Result<Value, BrowserError> {
    use base64::Engine;
    const MAX_SCREENSHOT_BYTES: usize = 25 * 1024 * 1024;
    let data = value["data"]
        .as_str()
        .ok_or_else(|| super::native_error("Screenshot data is unavailable".into()))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| super::native_error("Screenshot data is invalid".into()))?;
    if bytes.is_empty() || bytes.len() > MAX_SCREENSHOT_BYTES {
        return Err(super::native_error(
            "Screenshot exceeds the supported size".into(),
        ));
    }
    let directory = crate::paths::iyw_claw_user_dir().join("browser/screenshots/webview");
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|error| super::native_error(error.to_string()))?;
    let path = directory.join(format!("{}.png", uuid::Uuid::new_v4()));
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|error| super::native_error(error.to_string()))?;
    super::super::screenshot_quota::enforce_screenshot_quota(&directory).await?;
    Ok(json!({"path":path.to_string_lossy(),"mimeType":"image/png"}))
}

async fn timed_wait(
    milliseconds: u64,
    timeout: Duration,
    cancellation: &CancellationToken,
) -> Result<Value, BrowserError> {
    let duration = Duration::from_millis(milliseconds);
    tokio::select! {
        _ = cancellation.cancelled() => Err(cancelled_error()),
        _ = tokio::time::sleep(duration.min(timeout)) => {
            if duration > timeout { Err(timeout_error("The browser wait exceeded its timeout")) }
            else { Ok(json!({"waited":milliseconds})) }
        }
    }
}

fn timeout_error(message: &str) -> BrowserError {
    BrowserError::new(BrowserErrorCode::BrowserOperationTimeout, message)
}
fn cancelled_error() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserCancelled,
        "The browser operation was cancelled",
    )
}
