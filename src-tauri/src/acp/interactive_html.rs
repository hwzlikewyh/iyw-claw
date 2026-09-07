//! 会话内 HTML 页面及有界的用户反馈。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::oneshot;

pub const MAX_HTML_BYTES: usize = 256 * 1024;
pub const MAX_RESULT_BYTES: usize = 64 * 1024;
pub const MAX_TITLE_CHARS: usize = 120;
pub const MAX_OPEN_PAGES: usize = 8;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InteractiveHtmlRequest {
    pub title: String,
    pub html: String,
    #[serde(default)]
    pub wait_for_response: bool,
}

impl InteractiveHtmlRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.title.trim().is_empty() || self.title.chars().count() > MAX_TITLE_CHARS {
            return Err(format!(
                "title must contain 1..={MAX_TITLE_CHARS} characters"
            ));
        }
        if self.html.trim().is_empty() || self.html.len() > MAX_HTML_BYTES {
            return Err(format!(
                "html must contain 1..={MAX_HTML_BYTES} UTF-8 bytes"
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractiveHtmlState {
    pub interaction_id: String,
    pub title: String,
    pub html: String,
    pub wait_for_response: bool,
    #[serde(skip)]
    pub cancellation: tokio_util::sync::CancellationToken,
}

pub struct RegisteredHtml {
    pub interaction_id: String,
    pub answer_rx: Option<oneshot::Receiver<Value>>,
    pub cancellation: tokio_util::sync::CancellationToken,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HtmlResponse {
    pub interaction_id: String,
    pub action: HtmlResponseAction,
    // Value preserves JSON null, arrays and primitives without imposing a form schema.
    #[serde(default)]
    pub data: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HtmlResponseAction {
    Submit,
    Text,
    Close,
}

impl HtmlResponse {
    pub fn validate(&self) -> Result<(), String> {
        let bytes = serde_json::to_vec(&self.data).map_err(|error| error.to_string())?;
        if bytes.len() > MAX_RESULT_BYTES {
            return Err(format!("response exceeds {MAX_RESULT_BYTES} UTF-8 bytes"));
        }
        if matches!(self.action, HtmlResponseAction::Text)
            && !self
                .data
                .as_str()
                .is_some_and(|text| !text.trim().is_empty())
        {
            return Err("text response must be a non-empty string".to_string());
        }
        Ok(())
    }

    pub fn outcome(&self) -> Value {
        let status = if matches!(self.action, HtmlResponseAction::Close) {
            "cancelled"
        } else {
            "submitted"
        };
        json!({
            "interaction_id": self.interaction_id,
            "status": status,
            "source": if matches!(self.action, HtmlResponseAction::Text) { "text" } else { "page" },
            "data": self.data,
        })
    }
}
