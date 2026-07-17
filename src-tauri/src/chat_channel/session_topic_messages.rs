use super::i18n::{self, Lang};
use super::types::RichMessage;
use crate::models::agent::AgentType;

pub(super) fn active_session(lang: Lang, prefix: &str) -> String {
    match lang {
        Lang::ZhCn | Lang::ZhTw => {
            format!("当前 topic 已有活跃会话。请继续发送 follow-up，或先使用 {prefix}cancel。")
        }
        _ => format!(
            "This topic already has an active session. Send a follow-up or use {prefix}cancel first."
        ),
    }
}

pub(super) fn no_session(lang: Lang, prefix: &str) -> String {
    match lang {
        Lang::ZhCn | Lang::ZhTw => format!(
            "当前 topic 尚未绑定会话。使用 {prefix}task <描述> 开始，或 {prefix}resume <id> 恢复。"
        ),
        _ => format!(
            "This topic is not bound to a session. Use {prefix}task <description> or {prefix}resume <id>."
        ),
    }
}

pub(super) fn create_failed(lang: Lang, detail: &str) -> String {
    match lang {
        Lang::ZhCn | Lang::ZhTw => format!(
            "创建 Telegram topic 失败：{detail}\n请确认当前 chat 是 forum supergroup，且 bot 拥有管理 topics 权限。"
        ),
        _ => format!(
            "Failed to create Telegram topic: {detail}\nMake sure this chat is a forum supergroup and the bot can manage topics."
        ),
    }
}

pub(super) fn resume_failed(lang: Lang, conversation_id: i32, detail: &str) -> String {
    match lang {
        Lang::ZhCn | Lang::ZhTw => {
            format!("当前 topic 已绑定会话 #{conversation_id}，但恢复 Agent 失败：{detail}")
        }
        _ => format!(
            "This topic is bound to conversation #{conversation_id}, but failed to resume the Agent: {detail}"
        ),
    }
}

pub(super) fn general_task_created(
    lang: Lang,
    agent_type: AgentType,
    conversation_id: i32,
    folder_name: &str,
) -> RichMessage {
    let body = match lang {
        Lang::ZhCn | Lang::ZhTw => format!(
            "已创建新 topic 并启动任务：[{}] #{} @ {}",
            agent_type, conversation_id, folder_name
        ),
        _ => format!(
            "Created a new topic and started task: [{}] #{} @ {}",
            agent_type, conversation_id, folder_name
        ),
    };
    RichMessage::info(body).with_title(i18n::task_started_title(lang))
}

pub(super) fn topic_title(task_description: &str) -> String {
    let title = if task_description.chars().count() <= 80 {
        task_description.to_string()
    } else {
        format!(
            "{}...",
            task_description.chars().take(77).collect::<String>()
        )
    };
    format!("iyw-claw: {title}").chars().take(128).collect()
}
