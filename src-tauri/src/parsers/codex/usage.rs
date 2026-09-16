use serde_json::Value;

use crate::models::{MessageRole, TurnUsage, UnifiedMessage};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct UsageSnapshot {
    input: u64,
    output: u64,
    cached: u64,
    written: u64,
}

impl UsageSnapshot {
    fn parse(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let count = |key| object.get(key).and_then(Value::as_u64).unwrap_or(0);
        Some(Self {
            input: count("input_tokens"),
            output: count("output_tokens"),
            cached: count("cached_input_tokens"),
            written: count("cache_write_input_tokens"),
        })
    }

    fn difference(self, previous: Self) -> Self {
        Self {
            input: self.input.saturating_sub(previous.input),
            output: self.output.saturating_sub(previous.output),
            cached: self.cached.saturating_sub(previous.cached),
            written: self.written.saturating_sub(previous.written),
        }
    }

    fn normalized(self) -> Option<TurnUsage> {
        if self == Self::default() {
            return None;
        }
        // Codex 的输入含缓存读写，输出已含推理，展示分类必须互斥。
        let cached = self.cached.min(self.input);
        let written = self.written.min(self.input.saturating_sub(cached));
        Some(TurnUsage {
            input_tokens: self.input.saturating_sub(cached).saturating_sub(written),
            output_tokens: self.output,
            cache_creation_input_tokens: written,
            cache_read_input_tokens: cached,
        })
    }
}

#[derive(Default)]
pub(super) struct UsageTracker {
    previous: Option<UsageSnapshot>,
}

impl UsageTracker {
    pub(super) fn observe(&mut self, info: &Value) -> Option<TurnUsage> {
        let last = info.get("last_token_usage").and_then(UsageSnapshot::parse);
        let Some(total) = info.get("total_token_usage").and_then(UsageSnapshot::parse) else {
            return last.and_then(UsageSnapshot::normalized);
        };
        let previous = self.previous.replace(total);
        let delta = match previous {
            Some(previous) if previous == total => return None,
            Some(previous) if total.input >= previous.input && total.output >= previous.output => {
                total.difference(previous)
            }
            _ => last.unwrap_or(total),
        };
        delta.normalized()
    }
}

pub(super) fn extract_usage(value: &Value) -> Option<TurnUsage> {
    UsageSnapshot::parse(value)?.normalized()
}

pub(super) fn total_tokens(value: &Value) -> Option<u64> {
    if let Some(total) = value.get("total_tokens").and_then(Value::as_u64) {
        return Some(total);
    }
    let usage = UsageSnapshot::parse(value)?;
    let total = usage.input.saturating_add(usage.output);
    (total > 0).then_some(total)
}

pub(super) fn add_usage(target: &mut Option<TurnUsage>, extra: TurnUsage) {
    let Some(current) = target.as_mut() else {
        *target = Some(extra);
        return;
    };
    current.input_tokens = current.input_tokens.saturating_add(extra.input_tokens);
    current.output_tokens = current.output_tokens.saturating_add(extra.output_tokens);
    current.cache_read_input_tokens = current
        .cache_read_input_tokens
        .saturating_add(extra.cache_read_input_tokens);
    current.cache_creation_input_tokens = current
        .cache_creation_input_tokens
        .saturating_add(extra.cache_creation_input_tokens);
}

#[derive(Default)]
pub(super) struct TaskUsageTracker {
    start: Option<usize>,
    turn_id: Option<String>,
}

impl TaskUsageTracker {
    pub(super) fn begin(&mut self, payload: &Value, messages: &mut [UnifiedMessage]) {
        let turn_id = payload.get("turn_id").and_then(Value::as_str);
        if self.start.is_some() && turn_id.is_some() && self.turn_id.as_deref() == turn_id {
            return;
        }
        self.finish(messages);
        self.start = Some(messages.len());
        self.turn_id = turn_id.map(str::to_owned);
    }

    pub(super) fn finish(&mut self, messages: &mut [UnifiedMessage]) {
        let Some(start) = self.start.take() else {
            return;
        };
        self.turn_id = None;
        let Some(task) = messages.get_mut(start..) else {
            return;
        };
        let Some(last_assistant) = task
            .iter()
            .rposition(|message| matches!(message.role, MessageRole::Assistant))
        else {
            return;
        };
        // 执行中的追加消息和正文分页不能截断一次原生任务的消费统计。
        let mut total = None;
        for message in task.iter_mut() {
            if let Some(usage) = message.usage.take() {
                add_usage(&mut total, usage);
            }
        }
        task[last_assistant].usage = total;
    }
}
