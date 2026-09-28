use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::models::{MessageRole, UnifiedMessage};

#[derive(Default)]
pub(super) struct TurnTimingTracker {
    start_index: Option<usize>,
    started_at: Option<DateTime<Utc>>,
    turn_id: Option<String>,
    native_task: bool,
}

impl TurnTimingTracker {
    pub(super) fn begin(&mut self, messages: &mut [UnifiedMessage], record: &Value) {
        let turn_id = record.pointer("/payload/turn_id").and_then(Value::as_str);
        if self.native_task && turn_id.is_some() && self.turn_id.as_deref() == turn_id {
            return;
        }
        self.finish(messages, None);
        self.start_index = Some(messages.len());
        self.started_at =
            super::parse_codex_timestamp(record).or_else(|| unix_timestamp(record, "started_at"));
        self.turn_id = turn_id.map(str::to_owned);
        self.native_task = true;
    }

    pub(super) fn context(&mut self, messages: &mut [UnifiedMessage], record: &Value) {
        let turn_id = record.pointer("/payload/turn_id").and_then(Value::as_str);
        // 原生任务中可能有多次上下文记录，不能把每次记录都累计成整轮耗时。
        if self.native_task
            && (turn_id.is_none() || self.turn_id.is_none() || self.turn_id.as_deref() == turn_id)
        {
            return;
        }
        self.finish(messages, None);
        self.start_index = Some(messages.len());
        self.started_at = super::parse_codex_timestamp(record);
        self.turn_id = turn_id.map(str::to_owned);
    }

    pub(super) fn finish(&mut self, messages: &mut [UnifiedMessage], record: Option<&Value>) {
        let event_turn_id = record
            .and_then(|record| record.pointer("/payload/turn_id"))
            .and_then(Value::as_str);
        if self.turn_id.is_some()
            && event_turn_id.is_some()
            && self.turn_id.as_deref() != event_turn_id
        {
            return;
        }
        let Some(start_index) = self.start_index.take() else {
            return;
        };
        let started_at = self.started_at.take();
        self.turn_id = None;
        self.native_task = false;
        let Some(task) = messages.get_mut(start_index..) else {
            return;
        };
        let end = record
            .and_then(|record| {
                unix_timestamp(record, "completed_at")
                    .or_else(|| super::parse_codex_timestamp(record))
            })
            .or_else(|| last_activity(task));
        let duration = record.and_then(recorded_duration).or_else(|| {
            let start = started_at.or_else(|| unix_timestamp(record?, "started_at"))?;
            u64::try_from((end? - start).num_milliseconds()).ok()
        });
        let Some(last) = task
            .iter_mut()
            .rev()
            .find(|message| matches!(message.role, MessageRole::Assistant))
        else {
            return;
        };
        last.duration_ms = duration.or(last.duration_ms);
        last.completed_at = end.or(last.completed_at);
        tracing::debug!(duration_ms = ?duration, terminal_event = record.is_some(),
            "[codex-history] finalized turn timing");
    }
}

fn recorded_duration(record: &Value) -> Option<u64> {
    record
        .pointer("/payload/duration_ms")
        .and_then(Value::as_u64)
        .or_else(|| {
            let start = unix_timestamp(record, "started_at")?;
            let end = unix_timestamp(record, "completed_at")?;
            u64::try_from((end - start).num_milliseconds()).ok()
        })
}

fn unix_timestamp(record: &Value, field: &str) -> Option<DateTime<Utc>> {
    record
        .get("payload")?
        .get(field)?
        .as_i64()
        .and_then(|seconds| DateTime::from_timestamp(seconds, 0))
}

fn last_activity(messages: &[UnifiedMessage]) -> Option<DateTime<Utc>> {
    // 缺少结束事件的旧记录只回退到本轮内容，不能计入下一次提问前的空闲。
    messages
        .iter()
        .filter(|message| matches!(message.role, MessageRole::Assistant | MessageRole::Tool))
        .map(|message| message.completed_at.unwrap_or(message.timestamp))
        .max()
}
