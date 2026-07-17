use std::collections::{HashMap, HashSet};

use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;

use crate::models::{ContentBlock, MessageTurn, TurnRole};
use crate::parsers::truncate_str;

use super::tools::{
    grok_mcp_input_preview, str_field, tool_input_preview, unwrap_use_tool, update_text,
    update_tool_output, GROK_TOOL_OUTPUT_CAP,
};
use super::turns::{append_text, append_thinking, ensure_assistant, flush_assistant, GrokTurnMeta};
use super::updates::ParsedUpdates;

#[derive(Default)]
pub(super) struct UpdateAccumulator {
    parsed: ParsedUpdates,
    assistant: Option<MessageTurn>,
    tool_result_idx: HashMap<String, usize>,
    finalized_tools: HashSet<String>,
    turn_meta: GrokTurnMeta,
}

impl UpdateAccumulator {
    pub(super) fn consume(&mut self, value: &Value) {
        let now = self.record_timestamp(value);
        let Some(update) = value.pointer("/params/update") else {
            return;
        };
        let kind = update
            .get("sessionUpdate")
            .and_then(Value::as_str)
            .unwrap_or("");
        if kind == "user_message_chunk" {
            self.begin_user_turn();
        }
        self.turn_meta
            .observe(value.pointer("/params/_meta"), update.get("_meta"));
        match kind {
            "user_message_chunk" => self.handle_user(update, now),
            "agent_message_chunk" => self.handle_assistant_text(update, now, false),
            "agent_thought_chunk" => self.handle_assistant_text(update, now, true),
            "tool_call" => self.handle_tool_call(update, now),
            "tool_call_update" => self.handle_tool_update(update),
            "task_completed" => self.handle_task_completed(update),
            "turn_completed" => self.complete_turn(now),
            _ => {}
        }
    }

    pub(super) fn finish(mut self) -> ParsedUpdates {
        if let Some(turn) = self.assistant.as_mut() {
            self.turn_meta.apply(turn);
        }
        flush_assistant(
            &mut self.assistant,
            &mut self.parsed.turns,
            &mut self.tool_result_idx,
        );
        for (index, turn) in self.parsed.turns.iter_mut().enumerate() {
            turn.id = format!("grok-turn-{index}");
        }
        self.parsed
    }

    fn record_timestamp(&mut self, value: &Value) -> DateTime<Utc> {
        let timestamp = value
            .get("timestamp")
            .and_then(Value::as_i64)
            .and_then(|seconds| Utc.timestamp_opt(seconds, 0).single());
        if let Some(timestamp) = timestamp {
            self.parsed.first_ts.get_or_insert(timestamp);
            self.parsed.last_ts = Some(timestamp);
        }
        timestamp.unwrap_or_else(Utc::now)
    }

    fn begin_user_turn(&mut self) {
        if let Some(turn) = self.assistant.as_mut() {
            self.turn_meta.apply(turn);
        }
        flush_assistant(
            &mut self.assistant,
            &mut self.parsed.turns,
            &mut self.tool_result_idx,
        );
        self.turn_meta = GrokTurnMeta::default();
    }

    fn handle_user(&mut self, update: &Value, timestamp: DateTime<Utc>) {
        let text = update_text(update);
        self.parsed.content_events += 1;
        if self.parsed.first_user_text.is_none() && !text.trim().is_empty() {
            self.parsed.first_user_text = Some(text.clone());
        }
        if self.parsed.model.is_none() {
            self.parsed.model = update
                .pointer("/_meta/modelId")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        self.parsed.turns.push(MessageTurn {
            id: String::new(),
            role: TurnRole::User,
            blocks: vec![ContentBlock::Text { text }],
            timestamp,
            usage: None,
            duration_ms: None,
            model: None,
            completed_at: None,
        });
    }

    fn handle_assistant_text(&mut self, update: &Value, timestamp: DateTime<Utc>, thinking: bool) {
        self.parsed.content_events += 1;
        let turn = ensure_assistant(&mut self.assistant, timestamp);
        if thinking {
            append_thinking(turn, update_text(update));
        } else {
            append_text(turn, update_text(update));
        }
    }

    fn handle_tool_call(&mut self, update: &Value, timestamp: DateTime<Utc>) {
        self.parsed.content_events += 1;
        let id = str_field(update, "toolCallId");
        let raw_input = update.get("rawInput");
        let unwrapped = unwrap_use_tool(raw_input);
        let tool_name = resolve_tool_name(update, unwrapped.map(|(name, _)| name));
        let input_preview = match unwrapped {
            Some((_, input)) => grok_mcp_input_preview(input),
            None => tool_input_preview(raw_input),
        };
        let turn = ensure_assistant(&mut self.assistant, timestamp);
        turn.blocks.push(ContentBlock::ToolUse {
            tool_use_id: Some(id.clone()),
            tool_name,
            input_preview,
            meta: None,
        });
        turn.blocks.push(ContentBlock::ToolResult {
            tool_use_id: Some(id.clone()),
            output_preview: None,
            is_error: false,
            agent_stats: None,
            images: Vec::new(),
        });
        if !id.is_empty() {
            self.tool_result_idx.insert(id, turn.blocks.len() - 1);
        }
    }

    fn handle_tool_update(&mut self, update: &Value) {
        let id = str_field(update, "toolCallId");
        if self.finalized_tools.contains(&id) {
            return;
        }
        let failed = update.get("status").and_then(Value::as_str) == Some("failed");
        self.apply_tool_result(&id, update_tool_output(update), failed);
    }

    fn handle_task_completed(&mut self, update: &Value) {
        let snapshot = update.get("task_snapshot");
        let id = snapshot
            .map(|snapshot| str_field(snapshot, "task_id"))
            .unwrap_or_default();
        let output = snapshot
            .and_then(|snapshot| snapshot.get("output"))
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .map(|text| truncate_str(text, GROK_TOOL_OUTPUT_CAP));
        let failed = snapshot
            .and_then(|snapshot| snapshot.get("exit_code"))
            .and_then(Value::as_i64)
            .is_some_and(|code| code != 0);
        self.apply_tool_result(&id, output, failed);
        if !id.is_empty() {
            self.finalized_tools.insert(id);
        }
    }

    fn complete_turn(&mut self, timestamp: DateTime<Utc>) {
        if let Some(mut turn) = self.assistant.take() {
            self.turn_meta.apply(&mut turn);
            turn.completed_at = Some(timestamp);
            self.parsed.turns.push(turn);
        }
        self.turn_meta = GrokTurnMeta::default();
        self.tool_result_idx.clear();
    }

    fn apply_tool_result(&mut self, id: &str, output: Option<String>, failed: bool) {
        let Some(turn) = self.assistant.as_mut() else {
            return;
        };
        let Some(&index) = self.tool_result_idx.get(id) else {
            return;
        };
        if let Some(ContentBlock::ToolResult {
            output_preview,
            is_error,
            ..
        }) = turn.blocks.get_mut(index)
        {
            if let Some(text) = output {
                *output_preview = Some(text);
            }
            *is_error |= failed;
        }
    }
}

fn resolve_tool_name(update: &Value, unwrapped_name: Option<&str>) -> String {
    unwrapped_name
        .or_else(|| {
            update
                .get("_meta")
                .and_then(|meta| meta.get("x.ai/tool"))
                .and_then(|tool| tool.get("name"))
                .and_then(Value::as_str)
        })
        .or_else(|| update.get("title").and_then(Value::as_str))
        .unwrap_or("tool")
        .to_string()
}
