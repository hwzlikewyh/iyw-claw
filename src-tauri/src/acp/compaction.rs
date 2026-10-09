use serde_json::{json, Value};

/// 将新 ACP 压缩生命周期转换为本项目既有的工具增量，保持落盘/恢复路径一致。
pub(super) fn normalize(update: &mut Value) {
    let Some(kind) = update["sessionUpdate"].as_str() else {
        return;
    };
    if !matches!(kind, "compaction_update" | "compaction_summary_chunk") {
        return;
    }
    let Some(id) = update["compactionId"].as_str().filter(|id| !id.is_empty()) else {
        return;
    };
    let chunk = kind == "compaction_summary_chunk";
    if chunk {
        *update = json!({
            "sessionUpdate": "tool_call_update", "toolCallId": id,
            "rawOutput": summary_text(update.get("content")),
            "_meta": {"iyw.compactionSummary": true, "iyw": {"rawOutputAppend": true}},
        });
        return;
    }
    let meta = lifecycle_meta(update);
    let mut mapped = json!({
        "sessionUpdate": "tool_call_update", "toolCallId": id,
        "title": "Context compaction", "_meta": meta,
    });
    if let Some(status) = update.get("status") {
        mapped["status"] = status.clone();
    }
    if let Some(text) = summary_text(update.get("summary")) {
        mapped["rawOutput"] = json!(text);
    }
    *update = mapped;
}

fn lifecycle_meta(update: &Value) -> Value {
    let mut meta = update
        .get("_meta")
        .filter(|m| m.is_object())
        .cloned()
        .unwrap_or(json!({}));
    let marker = meta
        .get("contextCompaction")
        .or_else(|| meta.pointer("/jetbrains.air/contextCompaction"))
        .or_else(|| meta.pointer("/jetbrains/air/contextCompaction"))
        .filter(|marker| marker.is_object())
        .cloned()
        .unwrap_or(json!({"version": 1}));
    meta["contextCompaction"] = marker;
    meta[crate::parsers::compaction::SUMMARY_META_KEY] = json!(true);
    if let Some(error) = update.get("error").filter(|e| !e.is_null()) {
        meta["contextCompaction"]["error"] = error
            .as_str()
            .map(str::to_owned)
            .or_else(|| {
                error
                    .get("message")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .map(Value::String)
            .unwrap_or_else(|| json!("Context compaction failed"));
    }
    meta
}

/// 生命周期完成更新可能只含状态，补回已收到的触发方式和 token 计数。
pub(super) async fn preserve_metadata(
    state: &std::sync::Arc<tokio::sync::RwLock<super::session_state::SessionState>>,
    tool_call_id: &str,
    incoming: Option<Value>,
) -> Option<Value> {
    let mut incoming = incoming?;
    if incoming.get(crate::parsers::compaction::SUMMARY_META_KEY) != Some(&json!(true)) {
        return Some(incoming);
    }
    let state = state.read().await;
    let Some(existing) = state
        .active_tool_calls
        .get(tool_call_id)
        .and_then(|tool| tool.meta.as_ref())
    else {
        return Some(incoming);
    };
    let mut merged = existing.as_object().cloned().unwrap_or_default();
    let incoming_marker = incoming.get("contextCompaction").and_then(Value::as_object);
    if let Some(fields) = incoming_marker {
        let mut marker = existing
            .get("contextCompaction")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        marker.extend(fields.clone());
        incoming["contextCompaction"] = Value::Object(marker);
    }
    if let Some(fields) = incoming.as_object() {
        merged.extend(fields.clone());
    }
    Some(Value::Object(merged))
}

/// 重放可能缺少开始事件；第一片摘要仍要建立可恢复的分隔线。
pub(super) async fn chunk_metadata(
    state: &std::sync::Arc<tokio::sync::RwLock<super::session_state::SessionState>>,
    tool_call_id: &str,
) -> Option<Value> {
    let state = state.read().await;
    if state
        .active_tool_calls
        .get(tool_call_id)
        .and_then(|tool| tool.meta.as_ref())
        .is_some_and(|meta| {
            meta.get(crate::parsers::compaction::SUMMARY_META_KEY) == Some(&json!(true))
        })
    {
        return None;
    }
    Some(json!({"contextCompaction": {"version": 1}, "iyw.compactionSummary": true}))
}

fn summary_text(value: Option<&Value>) -> Option<String> {
    let value = value?;
    if let Some(text) = value.as_str() {
        return Some(text.into());
    }
    let blocks = value
        .as_array()
        .map(|a| a.as_slice())
        .unwrap_or(std::slice::from_ref(value));
    let text: String = blocks
        .iter()
        .filter(|block| block["type"] == "text")
        .filter_map(|block| block["text"].as_str())
        .collect();
    (!text.is_empty()).then_some(text)
}
