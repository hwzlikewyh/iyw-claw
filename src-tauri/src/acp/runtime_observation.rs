use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

const MAX_RUNTIME_ID_BYTES: usize = 160;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeObservation {
    TerminalPoll { item_id: String, process_id: String },
    Retry,
}

impl RuntimeObservation {
    pub fn from_meta(meta: Option<&Map<String, Value>>) -> Option<Self> {
        let value = meta?.get("iyw")?.get("activity")?;
        let observation: Self = serde_json::from_value(value.clone()).ok()?;
        if let Self::TerminalPoll {
            item_id,
            process_id,
        } = &observation
        {
            if !valid_id(item_id) || !valid_id(process_id) {
                return None;
            }
        }
        Some(observation)
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_RUNTIME_ID_BYTES && !value.chars().any(char::is_control)
}
