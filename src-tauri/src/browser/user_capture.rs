use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::error::{BrowserError, BrowserErrorCode};
use super::manager::BrowserSessionManager;
use super::types::{BrowserElementSummary, BrowserScreenshot};

const SCREENSHOT_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_SCREENSHOT_BYTES: usize = 25 * 1024 * 1024;
const MAX_INSPECT_COORDINATE: f64 = 4_096.0;

impl BrowserSessionManager {
    pub async fn capture_browser_tab_screenshot(
        &self,
        tab_id: &str,
    ) -> Result<BrowserScreenshot, BrowserError> {
        let lease = self.acquire_user_control(tab_id).await?;
        let result = self.capture_browser_tab_screenshot_owned(tab_id).await;
        lease.finish().await;
        result
    }

    pub async fn inspect_browser_tab_element(
        &self,
        tab_id: &str,
        x: f64,
        y: f64,
    ) -> Result<BrowserElementSummary, BrowserError> {
        if !valid_coordinate(x) || !valid_coordinate(y) {
            return Err(BrowserError::new(
                BrowserErrorCode::BrowserInvalidArgument,
                "The browser selection point is invalid",
            ));
        }
        let lease = self.acquire_user_control(tab_id).await?;
        let result = self.inspect_browser_tab_element_owned(tab_id, x, y).await;
        lease.finish().await;
        result
    }

    async fn capture_browser_tab_screenshot_owned(
        &self,
        tab_id: &str,
    ) -> Result<BrowserScreenshot, BrowserError> {
        let action = self.tabs.action_target(tab_id).await?;
        let path = action
            .cli
            .screenshot_path()
            .join(format!("share-{}.png", Uuid::new_v4()));
        let output = path.to_string_lossy().into_owned();
        let captured = action
            .cli
            .run_pinned(
                &action.session,
                &action.cdp_url,
                &["screenshot", &output],
                SCREENSHOT_TIMEOUT,
                CancellationToken::new(),
            )
            .await;
        if let Err(error) = captured {
            let _ = tokio::fs::remove_file(&path).await;
            return Err(error);
        }
        let bytes = tokio::fs::read(&path).await;
        let _ = tokio::fs::remove_file(&path).await;
        let bytes = bytes.map_err(|_| screenshot_unavailable())?;
        if bytes.is_empty() || bytes.len() > MAX_SCREENSHOT_BYTES {
            return Err(screenshot_unavailable());
        }
        Ok(BrowserScreenshot {
            data: STANDARD.encode(bytes),
            mime_type: "image/png".to_string(),
        })
    }

    async fn inspect_browser_tab_element_owned(
        &self,
        tab_id: &str,
        x: f64,
        y: f64,
    ) -> Result<BrowserElementSummary, BrowserError> {
        let action = self.tabs.action_target(tab_id).await?;
        let script = inspect_script(x, y);
        let response = action
            .cli
            .run_pinned(
                &action.session,
                &action.cdp_url,
                &["eval", &script],
                SCREENSHOT_TIMEOUT,
                CancellationToken::new(),
            )
            .await?;
        parse_element_summary(&response).ok_or_else(element_unavailable)
    }
}

fn valid_coordinate(value: f64) -> bool {
    value.is_finite() && (0.0..=MAX_INSPECT_COORDINATE).contains(&value)
}

fn inspect_script(x: f64, y: f64) -> String {
    format!(
        r#"(() => {{ const el = document.elementFromPoint({x}, {y}); if (!el) return null; const text = (el.innerText || el.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 256); const name = [el.getAttribute('aria-label'), el.getAttribute('title'), el.getAttribute('name'), el.getAttribute('alt'), el.getAttribute('placeholder'), text].find(Boolean) || null; const selector = el.id ? '#' + CSS.escape(el.id) : el.tagName.toLowerCase() + (el.getAttribute('name') ? '[name="' + CSS.escape(el.getAttribute('name')) + '"]' : ''); return {{ tag: el.tagName.toLowerCase(), role: el.getAttribute('role'), name, text: text || null, selector }}; }})()"#
    )
}

fn parse_element_summary(value: &serde_json::Value) -> Option<BrowserElementSummary> {
    let value = unwrap_element_value(value)?;
    let tag = value.get("tag")?.as_str()?.to_string();
    let selector = value.get("selector")?.as_str()?.to_string();
    (!tag.is_empty() && !selector.is_empty()).then(|| BrowserElementSummary {
        tag,
        role: value
            .get("role")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        name: value
            .get("name")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        text: value
            .get("text")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        selector,
    })
}

fn unwrap_element_value(value: &serde_json::Value) -> Option<&serde_json::Value> {
    let mut current = value;
    for _ in 0..4 {
        if current.get("tag").is_some() && current.get("selector").is_some() {
            return Some(current);
        }
        current = current
            .get("data")
            .or_else(|| current.get("value"))
            .or_else(|| current.get("result"))
            .or_else(|| current.get("output"))?;
    }
    None
}

fn screenshot_unavailable() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserInternal,
        "The browser screenshot could not be prepared",
    )
}

fn element_unavailable() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserInternal,
        "The selected browser element is unavailable",
    )
}
