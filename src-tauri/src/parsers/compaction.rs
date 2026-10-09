use std::collections::HashSet;

use chrono::Utc;
use serde_json::{json, Value};

use crate::models::{ContentBlock, MessageRole, UnifiedMessage};

pub(crate) const SUMMARY_META_KEY: &str = "iyw.compactionSummary";

pub(crate) fn divider(record: &Value, id: String, marker: Value) -> UnifiedMessage {
    let timestamp = super::claude::parse_timestamp(record).unwrap_or_else(Utc::now);
    UnifiedMessage {
        id: id.clone(),
        role: MessageRole::Assistant,
        timestamp,
        usage: None,
        duration_ms: None,
        model: None,
        completed_at: Some(timestamp),
        content: vec![
            ContentBlock::ToolUse {
                tool_use_id: Some(id.clone()),
                tool_name: "context_compaction".into(),
                input_preview: None,
                meta: Some(json!({"contextCompaction": marker})),
            },
            ContentBlock::ToolResult {
                tool_use_id: Some(id),
                output_preview: None,
                is_error: false,
                agent_stats: None,
                images: vec![],
            },
        ],
    }
}

pub(crate) fn attach(message: &mut UnifiedMessage, summary: &str) {
    if summary.trim().is_empty() {
        return;
    }
    for block in &mut message.content {
        match block {
            ContentBlock::ToolUse {
                meta: Some(meta), ..
            } => meta[SUMMARY_META_KEY] = json!(true),
            ContentBlock::ToolResult { output_preview, .. } => {
                *output_preview = Some(summary.into())
            }
            _ => {}
        }
    }
}

/// 完整历史与追加历史共用同一个边界/摘要归并器，避免摘要成为用户输入。
#[derive(Default)]
pub(crate) struct ClaudeCompactions {
    seen: HashSet<String>,
    pending: Option<usize>,
}

impl ClaudeCompactions {
    pub(crate) fn observe(&mut self, record: &Value, messages: &mut Vec<UnifiedMessage>) -> bool {
        if record["type"] == "system" && record["subtype"] == "compact_boundary" {
            self.boundary(record, messages);
            return true;
        }
        if record["isCompactSummary"] == true {
            self.summary(record, messages);
            return true;
        }
        if matches!(record["type"].as_str(), Some("user" | "assistant")) {
            self.pending = None;
        }
        false
    }

    fn boundary(&mut self, record: &Value, messages: &mut Vec<UnifiedMessage>) {
        let key = record["uuid"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("compact-{}", messages.len()));
        self.pending = None;
        if !self.seen.insert(key.clone()) {
            return;
        }
        let mut marker = json!({"version": 1});
        for field in ["trigger", "preTokens", "postTokens", "durationMs"] {
            if let Some(value) = record.pointer(&format!("/compactMetadata/{field}")) {
                marker[field] = value.clone();
            }
        }
        self.pending = Some(messages.len());
        messages.push(divider(record, key, marker));
    }

    fn summary(&mut self, record: &Value, messages: &mut Vec<UnifiedMessage>) {
        if let Some(id) = record["uuid"].as_str() {
            if !self.seen.insert(id.into()) {
                return;
            }
        }
        let content = &record["message"]["content"];
        let text = content.as_str().map(str::to_owned).unwrap_or_else(|| {
            content
                .as_array()
                .into_iter()
                .flatten()
                .filter(|part| part["type"] == "text")
                .filter_map(|part| part["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        });
        let at = self.pending.take().unwrap_or_else(|| {
            let at = messages.len();
            let id = record["uuid"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("claude-compact-summary-{at}"));
            messages.push(divider(record, id, json!({"version": 1})));
            at
        });
        if let Some(message) = messages.get_mut(at) {
            attach(message, continuation_summary(&text));
        }
    }
}

fn continuation_summary(text: &str) -> &str {
    const PREFIX: &str = "This session is being continued from a previous conversation";
    if !text.starts_with(PREFIX) {
        return text;
    }
    let mut body = text.split_once('\n').map_or(text, |(_, body)| body).trim();
    body = body.strip_prefix("Summary:").unwrap_or(body).trim_start();
    while let Some((head, tail)) = body.rsplit_once('\n') {
        if ![
            "If you need specific details from before compaction",
            "Continue the conversation from where it left off",
            "Please continue the conversation from where",
        ]
        .iter()
        .any(|prefix| tail.trim().starts_with(prefix))
        {
            break;
        }
        body = head.trim_end();
    }
    if body.trim().is_empty() {
        text
    } else {
        body
    }
}

#[derive(Default)]
pub(crate) struct CodexCompactions {
    pending: Option<usize>,
    seen: HashSet<String>,
}

impl CodexCompactions {
    pub(crate) fn observe(&mut self, record: &Value, messages: &mut Vec<UnifiedMessage>) -> bool {
        if record["type"] == "compacted" {
            self.pending = Some(messages.len());
            codex_divider(record, messages);
            return true;
        }
        let item = &record["payload"]["item"];
        if record["type"] != "event_msg"
            || record["payload"]["type"] != "item_completed"
            || !matches!(
                item["type"].as_str(),
                Some("ContextCompaction" | "contextCompaction")
            )
        {
            return false;
        }
        let Some(id) = item["id"].as_str() else {
            return true;
        };
        if !self.seen.insert(id.into()) {
            return true;
        }
        let at = self.pending.take().unwrap_or_else(|| {
            let at = messages.len();
            messages.push(divider(record, id.into(), json!({"version": 1})));
            at
        });
        if let Some(summary) = item["summary"].as_str() {
            attach(&mut messages[at], summary);
        }
        true
    }
}

pub(crate) fn codex_divider(record: &Value, messages: &mut Vec<UnifiedMessage>) {
    let id = format!(
        "codex-compact-{}-{}",
        record["timestamp"].as_str().unwrap_or(""),
        messages.len()
    );
    let mut message = divider(record, id, json!({"version": 1}));
    // 远程压缩可能仅保存加密状态；空 message 不伪造可读摘要。
    if let Some(text) = record.pointer("/payload/message").and_then(Value::as_str) {
        attach(&mut message, text);
    }
    messages.push(message);
}
