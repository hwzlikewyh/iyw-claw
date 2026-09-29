use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

const MAX_RUNTIME_ID_BYTES: usize = 160;
const MAX_RETRY_DETAIL_CHARS: usize = 900;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeObservation {
    TerminalPoll { item_id: String, process_id: String },
    Retry(RetryProgress),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RetryProgress {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_attempts: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl RuntimeObservation {
    pub fn from_meta(meta: Option<&Map<String, Value>>) -> Option<Self> {
        let value = meta?.get("iyw")?.get("activity")?;

        // 先尝试完整解析
        if let Ok(observation) = serde_json::from_value::<Self>(value.clone()) {
            if let Self::Retry(progress) = &observation {
                if let Some(detail) = value.get("detail").and_then(Value::as_str) {
                    let detail = super::stderr_tail::sanitize_diagnostic(detail)
                        .chars()
                        .take(MAX_RETRY_DETAIL_CHARS)
                        .collect::<String>();
                    tracing::warn!(
                        attempt = ?progress.attempt,
                        max = ?progress.max_attempts,
                        reason = ?progress.reason,
                        detail,
                        "[ACP] Agent request is retrying after an error"
                    );
                }
            }
            if let Self::TerminalPoll {
                item_id,
                process_id,
            } = &observation
            {
                if !valid_id(item_id) || !valid_id(process_id) {
                    return None;
                }
            }
            return Some(observation);
        }

        // 如果是 retry 但解析失败，回退到最小状态
        if value.get("kind").and_then(Value::as_str) == Some("retry") {
            tracing::warn!("[ACP] Retry observation with malformed progress fields, using default");
            return Some(Self::Retry(RetryProgress::default()));
        }

        None
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_RUNTIME_ID_BYTES && !value.chars().any(char::is_control)
}
