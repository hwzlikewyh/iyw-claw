use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::models::{MessageTurn, TurnRole, TurnUsage};
use crate::parsers::title_from_user_text;

use super::blocks::{
    add_usage, assistant_block, collect_text_parts, tool_result_block, usage_from_step,
};
use super::fields::{clean_string, event_millis, is_duplicate_stream_event, non_empty, with_data};

/// Values accumulated while scanning a single Harness event log.
#[derive(Default)]
pub(super) struct SessionParse {
    pub(super) turns: Vec<MessageTurn>,
    pub(super) cwd: Option<String>,
    pub(super) created_at: Option<DateTime<Utc>>,
    pub(super) delegation_depth: u64,
    pub(super) title: Option<String>,
    pub(super) first_user_text: Option<String>,
    pub(super) model: Option<String>,
    pub(super) context_window: Option<u64>,
    pub(super) last_step_input_side: Option<u64>,
    pub(super) first_ts: Option<DateTime<Utc>>,
    pub(super) last_ts: Option<DateTime<Utc>>,
    pub(super) message_count: u32,
    pub(super) content_events: u32,
}

pub(super) struct EventState {
    session: SessionParse,
    open_assistant: Option<usize>,
    pending_usage: Option<TurnUsage>,
    turn_started_at: Option<DateTime<Utc>>,
}

impl EventState {
    pub(super) fn parse(text: &str) -> SessionParse {
        let mut state = Self {
            session: SessionParse::default(),
            open_assistant: None,
            pending_usage: None,
            turn_started_at: None,
        };
        for line in text.lines() {
            state.apply_line(line);
        }
        state.close_assistant(None);
        state.session
    }

    fn apply_line(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return;
        };
        self.apply_event(&value);
    }

    fn apply_event(&mut self, value: &Value) {
        let event_type = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if is_duplicate_stream_event(event_type) {
            return;
        }
        let timestamp = event_millis(value);
        self.record_timestamp(timestamp.clone());
        let data = value.get("data");
        match event_type {
            "session" => self.read_session_header(value),
            "turn/start" => self.start_turn(timestamp),
            "turn/end" => self.end_turn(timestamp),
            "user/message" => with_data(data, |data| self.add_user_message(data, timestamp)),
            "assistant/message" => {
                with_data(data, |data| self.add_assistant_message(data, timestamp))
            }
            "tool/result" => with_data(data, |data| self.add_tool_result(data, timestamp)),
            "request/header" => with_data(data, |data| self.read_request_header(data)),
            "request/context" => with_data(data, |data| self.read_request_context(data)),
            "session/title" => with_data(data, |data| self.read_title(data)),
            _ => {}
        }
    }

    fn record_timestamp(&mut self, timestamp: Option<DateTime<Utc>>) {
        let Some(timestamp) = timestamp else {
            return;
        };
        if self.session.first_ts.is_none() {
            self.session.first_ts = Some(timestamp.clone());
        }
        self.session.last_ts = Some(timestamp);
    }

    fn read_session_header(&mut self, value: &Value) {
        self.session.cwd = clean_string(value.get("cwd"));
        self.session.created_at = value
            .get("createdAt")
            .and_then(Value::as_i64)
            .and_then(DateTime::from_timestamp_millis);
        self.session.delegation_depth = value
            .get("delegationDepth")
            .and_then(Value::as_u64)
            .unwrap_or(0);
    }

    fn start_turn(&mut self, timestamp: Option<DateTime<Utc>>) {
        self.close_assistant(None);
        self.turn_started_at = timestamp;
    }

    fn end_turn(&mut self, timestamp: Option<DateTime<Utc>>) {
        self.close_assistant(timestamp);
        self.turn_started_at = None;
    }

    fn add_user_message(&mut self, data: &Value, timestamp: Option<DateTime<Utc>>) {
        if data.pointer("/source/kind").and_then(Value::as_str) != Some("user") {
            return;
        }
        let text = collect_text_parts(data.get("content"));
        if text.trim().is_empty() {
            return;
        }
        if self.session.first_user_text.is_none() {
            self.session.first_user_text = Some(title_from_user_text(text.trim()));
        }
        self.session.content_events += 1;
        self.session.message_count += 1;
        let timestamp = self.turn_timestamp(timestamp);
        self.session.turns.push(MessageTurn {
            id: format!("turn-{}", self.session.turns.len()),
            role: TurnRole::User,
            blocks: vec![crate::models::ContentBlock::Text { text }],
            timestamp: timestamp.clone(),
            usage: None,
            duration_ms: None,
            model: None,
            completed_at: Some(timestamp),
        });
    }

    fn read_request_header(&mut self, data: &Value) {
        if let Some(model) = data
            .pointer("/header/config/model")
            .and_then(Value::as_str)
            .and_then(non_empty)
        {
            self.session.model = Some(model.to_string());
        }
    }

    fn read_request_context(&mut self, data: &Value) {
        if let Some(window) = data
            .get("contextWindow")
            .and_then(Value::as_u64)
            .filter(|window| *window > 0)
        {
            self.session.context_window = Some(window);
        }
    }

    fn read_title(&mut self, data: &Value) {
        if let Some(title) = data
            .get("title")
            .and_then(Value::as_str)
            .and_then(non_empty)
        {
            self.session.title = Some(title.to_string());
        }
    }

    fn add_assistant_message(&mut self, data: &Value, timestamp: Option<DateTime<Utc>>) {
        self.record_step_usage(data.get("usage"));
        let source_model = data
            .pointer("/message/source/model")
            .and_then(Value::as_str)
            .and_then(non_empty)
            .map(String::from);
        if self.session.model.is_none() {
            self.session.model.clone_from(&source_model);
        }
        let Some(blocks) = data.pointer("/message/content").and_then(Value::as_array) else {
            return;
        };
        self.append_assistant_blocks(blocks, timestamp, source_model);
    }

    fn record_step_usage(&mut self, value: Option<&Value>) {
        let Some(usage) = usage_from_step(value) else {
            return;
        };
        self.session.last_step_input_side = Some(
            usage
                .input_tokens
                .saturating_add(usage.cache_read_input_tokens),
        );
        self.pending_usage = Some(match self.pending_usage.take() {
            Some(previous) => add_usage(previous, usage),
            None => usage,
        });
    }

    fn append_assistant_blocks(
        &mut self,
        blocks: &[Value],
        timestamp: Option<DateTime<Utc>>,
        source_model: Option<String>,
    ) {
        let timestamp = self.turn_timestamp(timestamp);
        let model = self.session.model.clone().or(source_model);
        let mut counted_text = false;
        for block in blocks {
            let Some((rendered, is_text)) = assistant_block(block) else {
                continue;
            };
            if is_text && !counted_text {
                self.session.message_count += 1;
                counted_text = true;
            }
            self.session.content_events += 1;
            self.ensure_assistant(timestamp.clone(), model.clone())
                .blocks
                .push(rendered);
        }
    }

    fn add_tool_result(&mut self, data: &Value, timestamp: Option<DateTime<Utc>>) {
        self.session.content_events += 1;
        let timestamp = self.turn_timestamp(timestamp);
        let model = self.session.model.clone();
        self.ensure_assistant(timestamp, model)
            .blocks
            .push(tool_result_block(data));
    }

    fn turn_timestamp(&self, timestamp: Option<DateTime<Utc>>) -> DateTime<Utc> {
        timestamp
            .or_else(|| self.session.last_ts.clone())
            .unwrap_or_else(Utc::now)
    }

    fn ensure_assistant(
        &mut self,
        timestamp: DateTime<Utc>,
        model: Option<String>,
    ) -> &mut MessageTurn {
        let index = match self.open_assistant {
            Some(index) => index,
            None => {
                self.session.turns.push(MessageTurn {
                    id: format!("turn-{}", self.session.turns.len()),
                    role: TurnRole::Assistant,
                    blocks: Vec::new(),
                    timestamp,
                    usage: None,
                    duration_ms: None,
                    model,
                    completed_at: None,
                });
                let index = self.session.turns.len() - 1;
                self.open_assistant = Some(index);
                index
            }
        };
        &mut self.session.turns[index]
    }

    fn close_assistant(&mut self, ended_at: Option<DateTime<Utc>>) {
        let Some(index) = self.open_assistant.take() else {
            self.pending_usage = None;
            return;
        };
        let Some(turn) = self.session.turns.get_mut(index) else {
            self.pending_usage = None;
            return;
        };
        if let Some(usage) = self.pending_usage.take() {
            turn.usage = Some(
                turn.usage
                    .take()
                    .map_or(usage.clone(), |old| add_usage(old, usage)),
            );
        }
        if let Some(end) = ended_at {
            turn.completed_at = Some(end.clone());
            if let Some(start) = self.turn_started_at.clone() {
                let duration_ms = end.signed_duration_since(start).num_milliseconds();
                if duration_ms > 0 {
                    turn.duration_ms = Some(duration_ms as u64);
                }
            }
        }
    }
}
