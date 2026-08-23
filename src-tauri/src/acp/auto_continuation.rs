use std::sync::atomic::Ordering;

use crate::acp::session_state::{LiveContentBlock, LiveMessage, SessionState, ToolCallStatus};
use crate::acp::types::PlanEntryInfo;

pub(crate) const AUTO_CONTINUATION_PROMPT: &str =
    "继续完成当前用户请求。不要复述计划；执行尚未完成且已获授权的步骤。\n如果需要新的用户授权、选择或信息，明确说明阻塞并停止。";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AutoContinuationEvidence {
    pub reason_code: &'static str,
    pub evidence_kind: &'static str,
}

pub(crate) fn evaluate(
    state: &SessionState,
    stop_reason: &str,
    had_output: bool,
) -> Option<AutoContinuationEvidence> {
    if stop_reason != "end_turn" || !had_output || !is_direct_user_session(state) {
        return None;
    }
    if has_blocking_work(state) {
        return None;
    }
    let Some(live) = state.live_message.as_ref() else {
        return None;
    };
    let text = assistant_text_after_last_tool(live);
    if is_explicit_blocker_or_question(&text)
        || is_explicit_completion(&text)
        || mentions_sensitive_action(&text)
    {
        return None;
    }
    if has_pending_plan(live) {
        return Some(AutoContinuationEvidence {
            reason_code: "plan_pending",
            evidence_kind: "plan",
        });
    }
    is_action_promise(&text).then_some(AutoContinuationEvidence {
        reason_code: "action_promise_without_tool",
        evidence_kind: "commitment_text",
    })
}

fn is_direct_user_session(state: &SessionState) -> bool {
    let owner = state.owner_window_label.as_str();
    state.conversation_id.is_some()
        && !state.is_delegation_child
        && !owner.starts_with("chat_channel:")
        && owner != "automation"
        && owner != "web"
        && !owner.starts_with("automation:")
        && !owner.starts_with("delegation:")
        && owner != "delegation-probe"
}

fn has_blocking_work(state: &SessionState) -> bool {
    state.pending_permission.is_some()
        || state.pending_question.is_some()
        || state.pending_channel_confirmation.is_some()
        || !state.active_delegations.is_empty()
        || state.agent_inputs.iter().any(|item| !item.is_terminal())
        || state.active_terminal_count.load(Ordering::Acquire) > 0
        || state.has_active_background_work(chrono::Utc::now())
        || state.active_tool_calls.values().any(|tool| {
            matches!(
                tool.status,
                ToolCallStatus::Pending | ToolCallStatus::InProgress
            )
        })
}

fn assistant_text_after_last_tool(live: &LiveMessage) -> String {
    let start = live
        .content
        .iter()
        .rposition(|block| matches!(block, LiveContentBlock::ToolCallRef { .. }))
        .map(|index| index + 1)
        .unwrap_or(0);
    live.content[start..]
        .iter()
        .filter_map(|block| match block {
            LiveContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
        .trim()
        .to_string()
}

fn latest_plan(live: &LiveMessage) -> Option<Vec<PlanEntryInfo>> {
    live.content.iter().rev().find_map(|block| match block {
        LiveContentBlock::Plan { entries } => serde_json::from_value(entries.clone()).ok(),
        _ => None,
    })
}

fn has_pending_plan(live: &LiveMessage) -> bool {
    latest_plan(live).is_some_and(|entries| {
        entries.iter().any(|entry| {
            matches!(
                entry.status.trim().to_ascii_lowercase().as_str(),
                "pending" | "in_progress" | "in-progress" | "in progress"
            )
        })
    })
}

fn is_explicit_blocker_or_question(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    text.contains('?')
        || text.contains('？')
        || [
            "请确认",
            "需要你",
            "请提供",
            "无法",
            "不能",
            "缺少",
            "等待",
            "阻塞",
            "cannot",
            "can't",
            "need you",
            "please provide",
            "blocked",
            "waiting",
        ]
        .iter()
        .any(|marker| text.contains(marker) || lower.contains(marker))
}

fn is_explicit_completion(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "已完成",
        "完成了",
        "已经处理",
        "已经修复",
        "成功完成",
        "done",
        "completed",
        "finished",
        "successfully completed",
    ]
    .iter()
    .any(|marker| text.contains(marker) || lower.contains(marker))
}

fn mentions_sensitive_action(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "删除",
        "移除",
        "发布",
        "推送",
        "部署",
        "凭据",
        "密钥",
        "令牌",
        "权限",
        "授权",
        "支付",
        "购买",
        "重置",
        "格式化",
        "delete",
        "remove",
        "publish",
        "push",
        "deploy",
        "release",
        "credential",
        "secret",
        "token",
        "permission",
        "authorize",
        "payment",
        "purchase",
        "reset",
        "format",
        "uninstall",
    ]
    .iter()
    .any(|marker| text.contains(marker) || lower.contains(marker))
}

fn is_action_promise(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let lower = text.to_ascii_lowercase();
    let action = [
        "运行",
        "执行",
        "修改",
        "测试",
        "转写",
        "安装",
        "查找",
        "搜索",
        "重启",
        "run",
        "execute",
        "modify",
        "test",
        "transcribe",
        "install",
        "search",
    ];
    let has_action = action
        .iter()
        .any(|word| text.contains(word) || lower.contains(word));
    has_action
        && [
            "接下来",
            "下一步",
            "现在",
            "让我",
            "我会",
            "将",
            "用 gpu",
            "i'll",
            "i will",
            "next",
            "now i",
            "let me",
        ]
        .iter()
        .any(|marker| text.contains(marker) || lower.contains(marker))
}
