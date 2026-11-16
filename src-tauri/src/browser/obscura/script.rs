use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{json, Value};

use super::super::{BrowserError, BrowserErrorCode};
use super::worker::{unavailable, Operation, Rpc, Worker};
use super::Tab;

const AGENT: &str = include_str!("../native/agent.bundle.js");
const COMMANDS: &str = include_str!("../native/commands.js");
const MAX_SCREENSHOT_BYTES: usize = 25 * 1024 * 1024;
static REF_SEQUENCE: AtomicU64 = AtomicU64::new(1);
const MAX_TITLE_CHARS: usize = 512;

pub(super) struct Script<'a> {
    worker: &'a Worker,
    tab: &'a Tab,
    operation: &'a Operation<'a>,
}

impl<'a> Script<'a> {
    pub fn new(worker: &'a Worker, tab: &'a Tab, operation: &'a Operation<'a>) -> Self {
        Self {
            worker,
            tab,
            operation,
        }
    }

    pub async fn command(&self, args: &[String]) -> Result<Value, BrowserError> {
        let tab = self.tab;
        let epoch = format!("{}.{}", tab.id, tab.epoch);
        let prefix = REF_SEQUENCE.fetch_add(1, Ordering::Relaxed).to_string();
        let expression = format!("(() => {{ if (!globalThis.__codegAgent) {{ {AGENT} }} if (!globalThis.__iywNativeBrowser) {{ {COMMANDS} }} return globalThis.__iywNativeBrowser.execute({},{},{}); }})()", json!(args), json!(epoch), json!(prefix));
        self.evaluate(&expression).await
    }

    pub async fn evaluate(&self, expression: &str) -> Result<Value, BrowserError> {
        let Self {
            worker,
            tab,
            operation,
        } = self;
        let frames = worker
            .call(
                Rpc {
                    method: "Page.getFrameTree",
                    params: json!({}),
                    session: Some(tab.session.clone()),
                },
                operation,
            )
            .await?;
        let frame = frames["frameTree"]["frame"]["id"]
            .as_str()
            .ok_or_else(|| unavailable("Obscura page has no main frame"))?;
        let world = worker
            .call(
                Rpc {
                    method: "Page.createIsolatedWorld",
                    params: json!({"frameId":frame,"worldName":"iyw-obscura"}),
                    session: Some(tab.session.clone()),
                },
                operation,
            )
            .await?;
        let context = world["executionContextId"]
            .as_u64()
            .ok_or_else(|| unavailable("Obscura page has no isolated execution context"))?;
        let timeout = operation
            .deadline
            .saturating_duration_since(std::time::Instant::now())
            .as_millis() as u64;
        let result = worker.call(Rpc { method: "Runtime.evaluate", params: json!({
        "expression":expression,"contextId":context,"returnByValue":true,"awaitPromise":true,"timeout":timeout.max(1)
    }), session: Some(tab.session.clone()) }, operation).await?;
        Ok(result["result"]["value"].clone())
    }

    pub async fn screenshot(&self) -> Result<Value, BrowserError> {
        let Self {
            worker,
            tab,
            operation,
        } = self;
        use base64::Engine;
        let value = worker
            .call(
                Rpc {
                    method: "Page.captureScreenshot",
                    params: json!({"format":"png"}),
                    session: Some(tab.session.clone()),
                },
                operation,
            )
            .await?;
        let data = value["data"]
            .as_str()
            .ok_or_else(|| screenshot_error("Obscura screenshot is unavailable"))?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|_| screenshot_error("Obscura screenshot is invalid"))?;
        if bytes.is_empty() || bytes.len() > MAX_SCREENSHOT_BYTES {
            return Err(screenshot_error(
                "Obscura screenshot exceeds the supported size",
            ));
        }
        let directory = crate::paths::iyw_claw_user_dir().join("browser/screenshots/obscura");
        tokio::fs::create_dir_all(&directory)
            .await
            .map_err(|error| screenshot_error(&error.to_string()))?;
        let path = directory.join(format!("{}.png", uuid::Uuid::new_v4()));
        tokio::fs::write(&path, bytes)
            .await
            .map_err(|error| screenshot_error(&error.to_string()))?;
        super::super::screenshot_quota::enforce_screenshot_quota(&directory).await?;
        Ok(json!({"path":path.to_string_lossy(),"mimeType":"image/png"}))
    }
}

pub(super) async fn metadata(
    worker: &Worker,
    tab: &mut Tab,
    operation: &Operation<'_>,
) -> Result<(), BrowserError> {
    let value = Script::new(worker, tab, operation)
        .evaluate("({url:location.href,title:document.title})")
        .await?;
    tab.url = value["url"].as_str().unwrap_or(&tab.url).to_string();
    tab.title = value["title"]
        .as_str()
        .unwrap_or_default()
        .chars()
        .take(MAX_TITLE_CHARS)
        .collect();
    Ok(())
}

fn screenshot_error(message: &str) -> BrowserError {
    BrowserError::new(BrowserErrorCode::BrowserFrameDecodeFailed, message)
}
