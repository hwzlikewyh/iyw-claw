use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};

use super::acp_mapping::Update;
use crate::UpstreamError;

#[derive(Default)]
pub(super) struct MessageProjection {
    turn: Option<String>,
    text: HashMap<String, String>,
    recovered: HashSet<String>,
}

impl MessageProjection {
    pub(super) fn map(
        &mut self,
        method: &str,
        params: &Value,
    ) -> Result<Option<Update>, UpstreamError> {
        if method == "turn/completed" {
            return self.final_message(params);
        }
        let Some(turn) = params.get("turnId").and_then(Value::as_str) else {
            return Ok(None);
        };
        if self.turn.as_deref() != Some(turn) {
            self.turn = Some(turn.to_string());
            self.text.clear();
            self.recovered.clear();
        }
        if params["_iywRecovered"] == true
            && params.pointer("/item/type").and_then(Value::as_str) == Some("agentMessage")
        {
            if let Some(id) = params.pointer("/item/id").and_then(Value::as_str) {
                self.recovered.insert(id.to_string());
            }
        }
        if method == "item/agentMessage/delta" {
            let id = required(params, "itemId")?;
            // 快照之后排队中的旧 delta 没有偏移信息，只以最终完整 item 补齐，避免重复追加。
            if self.recovered.contains(id) {
                return Ok(None);
            }
            let delta = required(params, "delta")?;
            self.text.entry(id.into()).or_default().push_str(delta);
            return Ok(Some(chunk(delta)));
        }
        if method != "item/completed"
            || params.pointer("/item/type").and_then(Value::as_str) != Some("agentMessage")
        {
            return Ok(None);
        }
        let item = &params["item"];
        let id = required(item, "id")?;
        let full = required(item, "text")?;
        let previous = self.text.entry(id.into()).or_default();
        if !full.starts_with(previous.as_str()) {
            // 中途丢失的片段无法用追加修正，由回合结束的权威内容快照整体校正。
            self.recovered.insert(id.into());
            return Ok(None);
        }
        let suffix = &full[previous.len()..];
        let update = (!suffix.is_empty()).then(|| chunk(suffix));
        *previous = full.to_string();
        Ok(update)
    }

    fn final_message(&mut self, params: &Value) -> Result<Option<Update>, UpstreamError> {
        // 上游必达的回合结束通知携带最终回答，普通 item/completed 可能因队列满丢失。
        let Some(item) = params
            .pointer("/turn/items")
            .and_then(Value::as_array)
            .and_then(|items| {
                items
                    .iter()
                    .rev()
                    .find(|item| item["type"] == "agentMessage")
            })
        else {
            return Ok(None);
        };
        self.map(
            "item/completed",
            &json!({
                "turnId": params.pointer("/turn/id"), "item": item,
            }),
        )
    }
}

fn chunk(text: &str) -> Update {
    Update {
        method: "agent_message_chunk",
        params: json!({ "content": { "type": "text", "text": text } }),
    }
}

fn required<'a>(value: &'a Value, field: &str) -> Result<&'a str, UpstreamError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| UpstreamError::InvalidResponse(format!("message has no {field}")))
}
