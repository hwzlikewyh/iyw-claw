use super::sources::LedgerSource;
use crate::acp::runtime_task_receipts::TaskReceipt;
use crate::models::{ContentBlock, DbConversationDetail, MessageTurn, TurnRole};
use std::collections::{BTreeMap, HashMap, HashSet};

pub(super) fn collect(detail: &DbConversationDetail, source: &mut LedgerSource) {
    let mut group = Vec::new();
    let mut seen: HashSet<_> = source
        .requests
        .iter()
        .map(|row| row.request_id.clone())
        .collect();
    for turn in &detail.turns {
        if matches!(turn.role, TurnRole::User) {
            append(&group, source, &mut seen);
            group.clear();
        } else {
            group.push(turn);
        }
    }
    append(&group, source, &mut seen);
}

fn append(group: &[&MessageTurn], source: &mut LedgerSource, seen: &mut HashSet<String>) {
    let Some(last) = group
        .iter()
        .rev()
        .find(|turn| matches!(turn.role, TurnRole::Assistant))
    else {
        return;
    };
    let display = last.fork_message_id.as_deref().unwrap_or(&last.id);
    let native = source
        .messages
        .get(display)
        .cloned()
        .unwrap_or_else(|| format!("tool-history:{display}"));
    let Some((thread, turn)) = native.split_once(':') else {
        return;
    };
    let requests = group_requests(group);
    if requests.is_empty() {
        return;
    }
    source.messages.insert(display.to_string(), native.clone());
    for (request_id, purpose) in requests {
        if !seen.insert(request_id.clone()) {
            continue;
        }
        source.own.insert(request_id.clone());
        inherit_scopes(source, &native, &request_id);
        source.requests.push(TaskReceipt {
            thread_id: thread.into(),
            turn_id: turn.into(),
            request_id,
            message_id: display.into(),
            purpose: purpose.into(),
            parent_tool_use_id: None,
            task_id: None,
            root_turn_id: None,
            tool_ids: Vec::new(),
            goal_identity_known: false,
        });
    }
}

fn inherit_scopes(source: &mut LedgerSource, native: &str, request_id: &str) {
    let related: HashSet<_> = source
        .requests
        .iter()
        .filter(|receipt| format!("{}:{}", receipt.thread_id, receipt.turn_id) == native)
        .map(|receipt| receipt.request_id.as_str())
        .collect();
    for ids in source.scopes.values_mut() {
        if ids.iter().any(|id| related.contains(id.as_str())) {
            ids.insert(request_id.to_string());
        }
    }
}

fn group_requests(group: &[&MessageTurn]) -> BTreeMap<String, &'static str> {
    let mut tools = HashMap::new();
    let mut requests = BTreeMap::new();
    for block in group.iter().flat_map(|turn| &turn.blocks) {
        match block {
            ContentBlock::ToolUse {
                tool_use_id: Some(id),
                tool_name,
                ..
            } => {
                tools.insert(id.as_str(), tool_name.ends_with("generate_iyw_image"));
            }
            ContentBlock::ToolResult {
                tool_use_id,
                output_preview: Some(output),
                ..
            } => {
                let image = tool_use_id
                    .as_deref()
                    .and_then(|id| tools.get(id))
                    .copied()
                    .unwrap_or(false);
                requests.extend(super::tool_results::requests(output, image));
            }
            _ => {}
        }
    }
    requests
}
