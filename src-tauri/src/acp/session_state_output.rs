use super::{parse_tool_call_output_text, ToolCallOutput, ToolCallState, ToolCallStatus};

const MAX_OUTPUT_BYTES: usize = 256 * 1024;

pub(super) fn is_native(meta: Option<&serde_json::Value>) -> bool {
    meta.and_then(|meta| meta.pointer("/iyw/rawOutputAppend"))
        .and_then(serde_json::Value::as_bool)
        .is_some()
}

pub(super) fn is_compaction_summary(meta: Option<&serde_json::Value>) -> bool {
    meta.and_then(|meta| meta.get(crate::parsers::compaction::SUMMARY_META_KEY))
        .and_then(serde_json::Value::as_bool)
        == Some(true)
}

/// 摘要是保留上下文的文本；流式分片和 JSON 外形的摘要都不能被重新解析。
pub(super) fn apply_summary(tool: &mut ToolCallState, text: Option<&str>, append: bool) {
    let Some(text) = text else {
        return;
    };
    append_text(tool, text, append);
}

pub(super) fn apply(tool: &mut ToolCallState, text: Option<&str>, append: bool) {
    let Some(text) = text else {
        return;
    };
    if !append
        && matches!(
            tool.status,
            ToolCallStatus::Completed | ToolCallStatus::Failed
        )
    {
        tool.output = Some(parse_tool_call_output_text(text));
        return;
    }
    append_text(tool, text, append);
}

fn append_text(tool: &mut ToolCallState, text: &str, append: bool) {
    // 原始文本原地追加，避免每个增量替换快照，也不提前解析未完成的 JSON。
    if !append || !matches!(tool.output, Some(ToolCallOutput::Text { .. })) {
        tool.output = Some(ToolCallOutput::Text {
            content: String::new(),
        });
    }
    if let Some(ToolCallOutput::Text { content }) = &mut tool.output {
        content.push_str(text);
        let mut start = content.len().saturating_sub(MAX_OUTPUT_BYTES);
        while !content.is_char_boundary(start) {
            start += 1;
        }
        if start > 0 {
            content.drain(..start);
        }
    }
}
