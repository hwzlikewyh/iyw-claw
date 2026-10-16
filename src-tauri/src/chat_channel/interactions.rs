mod action;
mod question;

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::task_context::{self, TaskContext};
use super::types::*;
use crate::acp::manager::ConnectionManager;
use crate::acp::types::AcpEvent;

pub(super) const INTERACTION_TTL: Duration = Duration::from_secs(600);
const MAX_ACTIONS: usize = 4096;

#[derive(Clone)]
enum Action {
    Task(&'static str),
    Permission(String, String),
    Confirmation(String, bool),
    Answer(String, String, String),
}

struct Pending {
    task: TaskContext,
    action: Action,
    created: Instant,
}

fn pending() -> &'static Mutex<HashMap<String, Pending>> {
    static PENDING: OnceLock<Mutex<HashMap<String, Pending>>> = OnceLock::new();
    PENDING.get_or_init(Mutex::default)
}

fn button(task: &TaskContext, label: &str, action: Action) -> MessageButton {
    let id = format!("iywt:{}", uuid::Uuid::new_v4().simple());
    let mut pending = pending().lock().unwrap_or_else(|e| e.into_inner());
    pending.retain(|_, item| item.created.elapsed() < INTERACTION_TTL);
    if pending.len() >= MAX_ACTIONS {
        if let Some(oldest) = pending
            .iter()
            .min_by_key(|(_, item)| item.created)
            .map(|(id, _)| id.clone())
        {
            pending.remove(&oldest);
        }
    }
    pending.insert(
        id.clone(),
        Pending {
            task: task.clone(),
            action,
            created: Instant::now(),
        },
    );
    MessageButton {
        id,
        label: label.chars().take(20).collect(),
        style: ButtonStyle::Default,
    }
}

pub(crate) fn task_card(task: &TaskContext, title: &str, body: &str) -> InteractiveMessage {
    InteractiveMessage {
        base: RichMessage::info(body).with_title(title),
        buttons: vec![
            button(task, "查看进度", Action::Task("status")),
            button(task, "停止汇报", Action::Task("quiet")),
            button(task, "恢复汇报", Action::Task("resume")),
        ],
        callback_context: serde_json::json!({"task_id":task.id}),
    }
}

pub(crate) async fn handle(
    command: &IncomingCommand,
    manager: &ConnectionManager,
) -> Option<RichMessage> {
    let key = command
        .callback_data
        .as_deref()
        .or_else(|| command.command_text.trim().strip_prefix("操作 "));
    let Some(key) = key.filter(|key| key.starts_with("iywt:")) else {
        return question::answer_text(command, manager).await;
    };
    let item = {
        let mut pending = pending().lock().unwrap_or_else(|e| e.into_inner());
        let Some(item) = pending.get(key) else {
            return Some(RichMessage::info("此操作已处理或已过期。"));
        };
        if item.task.origin.route.sender_id != command.sender_id
            || !item
                .task
                .origin
                .route
                .target
                .matches_thread(&command.target)
        {
            return Some(RichMessage::error("此操作不属于当前用户或聊天。"));
        }
        pending.remove(key).expect("interaction checked under lock")
    };
    if item.created.elapsed() >= INTERACTION_TTL || task_context::tracked(&item.task.id).is_none() {
        return Some(RichMessage::info("任务或问题已变更，请使用最新消息。"));
    }
    let result = action::execute(item, manager).await;
    Some(match result {
        Ok(text) => RichMessage::info(text),
        Err(error) => RichMessage::error(error),
    })
}

pub(crate) async fn present(
    channels: &super::manager::ChatChannelManager,
    connection_id: &str,
    event: (u64, &AcpEvent),
) {
    let Some(task) = task_context::current(connection_id) else {
        return;
    };
    if event.0 <= task.origin.accepted_event_seq {
        return;
    }
    let card = event_card(&task, event.1);
    if let Some(card) = card {
        let card = with_text_actions(&task, card);
        if let Err(error) = channels
            .send_interactive_to_target(&task.origin.route.target, &card)
            .await
        {
            tracing::warn!(
                channel_id = task.origin.route.channel_id,
                category = error.category(),
                "[channel-interaction] prompt delivery failed"
            );
        }
    }
}

pub(crate) fn with_text_actions(
    task: &TaskContext,
    mut card: InteractiveMessage,
) -> InteractiveMessage {
    if !matches!(
        task.origin.channel_type,
        ChannelType::Lark | ChannelType::WecomAiBot
    ) {
        for button in &card.buttons {
            card.base
                .body
                .push_str(&format!("\n{}：回复「操作 {}」", button.label, button.id));
        }
    }
    card
}

async fn permission(
    manager: &ConnectionManager,
    connection_id: &str,
    choice: (&str, &str),
) -> Result<String, String> {
    let (request, option) = choice;
    let state = manager.get_state(connection_id).await.ok_or("会话已结束")?;
    let active = state
        .read()
        .await
        .pending_permission
        .as_ref()
        .is_some_and(|item| item.request_id == request);
    if !active {
        return Err("权限请求已结束".into());
    }
    manager
        .respond_permission(connection_id, &request, &option)
        .await
        .map_err(|e| e.to_string())?;
    Ok("已提交权限选择。".into())
}

fn event_card(task: &TaskContext, event: &AcpEvent) -> Option<InteractiveMessage> {
    match event {
        AcpEvent::QuestionRequest {
            question_id,
            questions,
        } => question::card(&task, question_id, questions),
        AcpEvent::PermissionRequest {
            request_id,
            options,
            ..
        } => Some(InteractiveMessage {
            base: RichMessage::info("此任务需要操作授权，请选择。也可以使用 /approve 或 /deny。")
                .with_title("需要确认"),
            buttons: options
                .iter()
                .take(6)
                .map(|option| {
                    button(
                        &task,
                        &option.name,
                        Action::Permission(request_id.clone(), option.option_id.clone()),
                    )
                })
                .collect(),
            callback_context: serde_json::json!({"task_id":task.id}),
        }),
        AcpEvent::ChannelConfirmationRequested { confirmation } => Some(InteractiveMessage {
            base: RichMessage::info(format!(
                "{}：{}。此操作涉及本地关联记录，请确认。",
                confirmation.action, confirmation.channel_name
            ))
            .with_title("确认渠道操作"),
            buttons: vec![
                button(
                    &task,
                    "确认",
                    Action::Confirmation(confirmation.confirmation_id.clone(), true),
                ),
                button(
                    &task,
                    "取消",
                    Action::Confirmation(confirmation.confirmation_id.clone(), false),
                ),
            ],
            callback_context: serde_json::json!({"task_id":task.id}),
        }),
        _ => None,
    }
}
