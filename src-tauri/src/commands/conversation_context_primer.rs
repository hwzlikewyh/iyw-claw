use std::collections::HashSet;

use serde::Serialize;

use crate::models::message::{ContentBlock, MessageTurn, TurnRole};

const MAX_USER_TURNS: usize = 20;
const MAX_PRIMER_CHARS: usize = 24_000;
const MAX_USER_CHARS: usize = 1_300;
const MAX_ASSISTANT_CHARS: usize = 700;
const MAX_PLAN_CHARS: usize = 300;
const MAX_PLAN_PREVIEW_CHARS: usize = 4_000;
const MAX_TOOLS: usize = 8;
const MAX_TOOL_NAME_CHARS: usize = 80;
const OMITTED_BLOCK: &str = "[large file, patch, log, or encoded block omitted]";
const HEADER: &str = "# Visible transcript recap\n\nContinue this work using only the visible transcript recap below. The original agent session and its hidden context were not restored. Inspect the current workspace state before acting, and do not repeat prior actions merely because they appear in this recap.\n";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationContextPrimer {
    pub text: String,
    pub included_user_turns: usize,
    pub total_user_turns: usize,
    pub truncated: bool,
}

struct RenderedExchange {
    text: String,
    truncated: bool,
}

pub fn build_context_primer(turns: &[MessageTurn]) -> ConversationContextPrimer {
    let user_indices: Vec<usize> = turns
        .iter()
        .enumerate()
        .filter_map(|(index, turn)| matches!(turn.role, TurnRole::User).then_some(index))
        .collect();
    let total_user_turns = user_indices.len();
    let (selected, mut content_truncated) = select_exchanges(turns, &user_indices);
    let included_user_turns = selected.len();
    content_truncated |= included_user_turns < total_user_turns.min(MAX_USER_TURNS);
    let mut text = HEADER.to_string();
    if selected.is_empty() {
        text.push_str("\n[No visible user turns were available.]\n");
    } else {
        text.push('\n');
        text.push_str(&selected.join("\n\n"));
        text.push('\n');
    }

    ConversationContextPrimer {
        text,
        included_user_turns,
        total_user_turns,
        truncated: content_truncated,
    }
}

fn select_exchanges(turns: &[MessageTurn], user_indices: &[usize]) -> (Vec<String>, bool) {
    let recent = user_indices.len().saturating_sub(MAX_USER_TURNS);
    let mut selected = Vec::new();
    let mut used = char_count(HEADER);
    let mut truncated = recent > 0;
    for position in (recent..user_indices.len()).rev() {
        let start = user_indices[position];
        let end = user_indices
            .get(position + 1)
            .copied()
            .unwrap_or(turns.len());
        let exchange = render_exchange(&turns[start], &turns[start + 1..end]);
        let section_chars = char_count(&exchange.text) + 2;
        if used + section_chars > MAX_PRIMER_CHARS {
            truncated = true;
            break;
        }
        used += section_chars;
        truncated |= exchange.truncated;
        selected.push(exchange.text);
    }
    selected.reverse();
    (selected, truncated)
}

fn render_exchange(user: &MessageTurn, following: &[MessageTurn]) -> RenderedExchange {
    let (user_text, mut truncated) = render_user_text(user);
    let (assistant_text, assistant_truncated) = render_assistant_tail(following);
    truncated |= assistant_truncated;
    let (plan, tools, tool_errors) = render_tool_summary(following);
    let mut parts = vec![format!("## User\n{user_text}")];
    if let Some(text) = assistant_text {
        parts.push(format!("## Assistant conclusion\n{text}"));
    }
    if let Some(plan) = plan {
        parts.push(format!("Plan status: {plan}"));
    }
    if !tools.is_empty() {
        parts.push(format!("Tools used: {}", tools.join(", ")));
    }
    if tool_errors > 0 {
        parts.push(format!("Tool errors: {tool_errors} (details omitted)"));
    }
    RenderedExchange {
        text: parts.join("\n\n"),
        truncated,
    }
}

fn render_user_text(turn: &MessageTurn) -> (String, bool) {
    let text = turn
        .blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    let (sanitized, omitted) = omit_bulky_blocks(&text);
    let (text, shortened) = take_head(&sanitized, MAX_USER_CHARS);
    let text = if text.trim().is_empty() {
        "[non-text user content omitted]".to_string()
    } else {
        text
    };
    (text, omitted || shortened)
}

fn render_assistant_tail(following: &[MessageTurn]) -> (Option<String>, bool) {
    let text = following
        .iter()
        .filter(|turn| matches!(turn.role, TurnRole::Assistant))
        .flat_map(|turn| turn.blocks.iter())
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    let (sanitized, omitted) = omit_bulky_blocks(&text);
    if sanitized.trim().is_empty() {
        return (None, omitted);
    }
    let (tail, shortened) = take_tail(&sanitized, MAX_ASSISTANT_CHARS);
    (Some(tail), omitted || shortened)
}

fn render_tool_summary(following: &[MessageTurn]) -> (Option<String>, Vec<String>, usize) {
    let mut tools = Vec::new();
    let mut seen = HashSet::new();
    let mut plan = None;
    let mut errors = 0;
    for block in following.iter().flat_map(|turn| turn.blocks.iter()) {
        match block {
            ContentBlock::ToolUse {
                tool_name,
                input_preview,
                ..
            } => {
                if tools.len() < MAX_TOOLS && seen.insert(tool_name.as_str()) {
                    tools.push(
                        tool_name
                            .chars()
                            .take(MAX_TOOL_NAME_CHARS)
                            .collect::<String>(),
                    );
                }
                if plan.is_none() && tool_name.contains("plan") {
                    plan = input_preview.as_deref().and_then(compact_plan);
                }
            }
            ContentBlock::ToolResult { is_error: true, .. } => errors += 1,
            _ => {}
        }
    }
    (plan, tools, errors)
}

fn compact_plan(preview: &str) -> Option<String> {
    if char_count(preview) > MAX_PLAN_PREVIEW_CHARS {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(preview).ok()?;
    let items = value.get("plan")?.as_array()?;
    let summary = items
        .iter()
        .take(5)
        .filter_map(|item| {
            let step = item.get("step")?.as_str()?.trim();
            let status = item.get("status").and_then(serde_json::Value::as_str)?;
            (!step.is_empty()).then(|| format!("[{status}] {step}"))
        })
        .collect::<Vec<_>>()
        .join("; ");
    let (summary, _) = take_head(&summary, MAX_PLAN_CHARS);
    (!summary.is_empty()).then_some(summary)
}

fn omit_bulky_blocks(text: &str) -> (String, bool) {
    let mut output = Vec::new();
    let mut omitted = false;
    let mut fenced = false;
    let mut patch = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            fenced = !fenced;
            omitted = true;
            push_omission(&mut output);
            continue;
        }
        if trimmed.starts_with("*** Begin Patch") || trimmed.starts_with("diff --git ") {
            patch = true;
            omitted = true;
            push_omission(&mut output);
            continue;
        }
        if trimmed.starts_with("*** End Patch") {
            patch = false;
            continue;
        }
        if fenced || patch || looks_encoded(trimmed) {
            omitted = true;
            push_omission(&mut output);
            continue;
        }
        output.push(line);
    }
    (output.join("\n").trim().to_string(), omitted)
}

fn push_omission(output: &mut Vec<&str>) {
    if output.last().copied() != Some(OMITTED_BLOCK) {
        output.push(OMITTED_BLOCK);
    }
}

fn looks_encoded(line: &str) -> bool {
    line.len() > 256
        && line
            .bytes()
            .filter(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'='))
            .count()
            * 10
            > line.len() * 9
}

fn take_head(text: &str, limit: usize) -> (String, bool) {
    if char_count(text) <= limit {
        return (text.to_string(), false);
    }
    let marker = "\n[earlier content omitted]";
    let keep = limit.saturating_sub(char_count(marker));
    (
        format!("{}{}", text.chars().take(keep).collect::<String>(), marker),
        true,
    )
}

fn take_tail(text: &str, limit: usize) -> (String, bool) {
    if char_count(text) <= limit {
        return (text.to_string(), false);
    }
    let marker = "[earlier assistant text omitted]\n";
    let keep = limit.saturating_sub(char_count(marker));
    let tail = text.chars().rev().take(keep).collect::<String>();
    (
        format!("{marker}{}", tail.chars().rev().collect::<String>()),
        true,
    )
}

fn char_count(text: &str) -> usize {
    text.chars().count()
}
