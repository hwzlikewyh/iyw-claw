use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};

use super::acp_mapping::Update;
use crate::UpstreamError;

#[derive(Default)]
pub(super) struct ThinkingProjection {
    turn: Option<String>,
    streams: HashMap<(String, &'static str, u64), String>,
    recovered: HashSet<String>,
}

impl ThinkingProjection {
    pub(super) fn map(
        &mut self,
        method: &str,
        params: &Value,
    ) -> Result<Option<Update>, UpstreamError> {
        let Some(turn) = params["turnId"].as_str() else {
            return Ok(None);
        };
        if self.turn.as_deref() != Some(turn) {
            self.turn = Some(turn.to_string());
            self.streams.clear();
            self.recovered.clear();
        }
        if params["_iywRecovered"] == true
            && params.pointer("/item/type").and_then(Value::as_str) == Some("reasoning")
        {
            if let Some(id) = params.pointer("/item/id").and_then(Value::as_str) {
                self.recovered.insert(id.into());
            }
        }
        if let Some((kind, index_key)) = stream_kind(method) {
            let id = string(params, "itemId")?;
            if self.recovered.contains(id) {
                return Ok(None);
            }
            let delta = string(params, "delta")?;
            let index = params[index_key].as_u64().unwrap_or_default();
            self.streams
                .entry((id.to_string(), kind, index))
                .or_default()
                .push_str(delta);
            return Ok(Some(chunk(delta.to_string())));
        }
        if method != "item/completed"
            || params.pointer("/item/type").and_then(Value::as_str) != Some("reasoning")
        {
            return Ok(None);
        }
        let item = &params["item"];
        let id = string(item, "id")?;
        let mut missing = String::new();
        for kind in ["summary", "content"] {
            for (index, full) in item[kind].as_array().into_iter().flatten().enumerate() {
                let full = full
                    .as_str()
                    .ok_or_else(|| invalid("reasoning snapshot is not text"))?;
                let previous = self
                    .streams
                    .entry((id.to_string(), kind, index as u64))
                    .or_default();
                if !full.starts_with(previous.as_str()) {
                    self.recovered.insert(id.into());
                    *previous = full.to_string();
                    continue;
                }
                missing.push_str(&full[previous.len()..]);
                *previous = full.to_string();
            }
        }
        Ok((!missing.is_empty()).then(|| chunk(missing)))
    }
}

pub(super) fn is_delta(method: &str) -> bool {
    stream_kind(method).is_some()
}

fn stream_kind(method: &str) -> Option<(&'static str, &'static str)> {
    match method {
        "item/reasoning/summaryTextDelta" | "item/reasoningSummaryText/delta" => {
            Some(("summary", "summaryIndex"))
        }
        "item/reasoning/textDelta" | "item/reasoningContent/delta" => {
            Some(("content", "contentIndex"))
        }
        _ => None,
    }
}

fn chunk(text: String) -> Update {
    Update {
        method: "agent_thought_chunk",
        params: json!({ "content": { "type": "text", "text": text } }),
    }
}
fn string<'a>(value: &'a Value, field: &str) -> Result<&'a str, UpstreamError> {
    value[field]
        .as_str()
        .ok_or_else(|| invalid("reasoning event is missing text or identity"))
}
fn invalid(message: &str) -> UpstreamError {
    UpstreamError::InvalidResponse(message.to_string())
}
