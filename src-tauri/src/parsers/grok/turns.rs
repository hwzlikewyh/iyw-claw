use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::models::{ContentBlock, MessageTurn, TurnRole, TurnUsage};

#[derive(Default)]
pub(super) struct GrokTurnMeta {
    total_tokens: Option<u64>,
    start_ms: Option<i64>,
    end_ms: Option<i64>,
    model: Option<String>,
}

impl GrokTurnMeta {
    pub(super) fn observe(&mut self, params_meta: Option<&Value>, update_meta: Option<&Value>) {
        if let Some(meta) = params_meta {
            if let Some(tokens) = meta.get("totalTokens").and_then(Value::as_u64) {
                self.total_tokens = Some(self.total_tokens.map_or(tokens, |old| old.max(tokens)));
            }
            if let Some(start) = meta.get("turnStartMs").and_then(Value::as_i64) {
                self.start_ms = Some(self.start_ms.map_or(start, |old| old.min(start)));
            }
            if let Some(end) = meta.get("agentTimestampMs").and_then(Value::as_i64) {
                self.end_ms = Some(self.end_ms.map_or(end, |old| old.max(end)));
            }
        }
        if self.model.is_none() {
            self.model = update_meta
                .and_then(|meta| meta.get("modelId"))
                .and_then(Value::as_str)
                .filter(|model| !model.is_empty())
                .map(str::to_string);
        }
    }

    pub(super) fn apply(&self, turn: &mut MessageTurn) {
        if turn.model.is_none() {
            turn.model.clone_from(&self.model);
        }
        if turn.usage.is_none() {
            if let Some(tokens) = self.total_tokens.filter(|tokens| *tokens > 0) {
                turn.usage = Some(TurnUsage {
                    input_tokens: tokens,
                    output_tokens: 0,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                });
            }
        }
        if turn.duration_ms.is_none() {
            if let (Some(start), Some(end)) = (self.start_ms, self.end_ms) {
                if end > start {
                    turn.duration_ms = Some((end - start) as u64);
                }
            }
        }
    }
}

pub(super) fn ensure_assistant(
    assistant: &mut Option<MessageTurn>,
    timestamp: DateTime<Utc>,
) -> &mut MessageTurn {
    assistant.get_or_insert_with(|| MessageTurn {
        id: String::new(),
        role: TurnRole::Assistant,
        blocks: Vec::new(),
        timestamp,
        usage: None,
        duration_ms: None,
        model: None,
        completed_at: None,
    })
}

pub(super) fn flush_assistant(
    assistant: &mut Option<MessageTurn>,
    turns: &mut Vec<MessageTurn>,
    tool_result_idx: &mut HashMap<String, usize>,
) {
    if let Some(turn) = assistant.take() {
        turns.push(turn);
    }
    tool_result_idx.clear();
}

pub(super) fn append_text(turn: &mut MessageTurn, text: String) {
    if text.is_empty() {
        return;
    }
    if let Some(ContentBlock::Text { text: last }) = turn.blocks.last_mut() {
        last.push_str(&text);
    } else {
        turn.blocks.push(ContentBlock::Text { text });
    }
}

pub(super) fn append_thinking(turn: &mut MessageTurn, text: String) {
    if text.is_empty() {
        return;
    }
    if let Some(ContentBlock::Thinking { text: last }) = turn.blocks.last_mut() {
        last.push('\n');
        last.push_str(&text);
    } else {
        turn.blocks.push(ContentBlock::Thinking { text });
    }
}
