use std::time::Instant;

use sea_orm::DatabaseConnection;

use super::ReplyRoute;
use crate::acp::manager::ConnectionManager;
use crate::acp::question::PendingQuestionState;
use crate::acp::types::{ConnectionStatus, PromptInputBlock};
use crate::acp::AgentInputPayload;
use crate::chat_channel::interactions::question::parse_answer;
use crate::chat_channel::manager::ChatChannelManager;
use crate::chat_channel::types::{IncomingCommand, RichMessage};
use crate::db::AppDatabase;

type Runtime<'a> = (
    &'a DatabaseConnection,
    &'a ChatChannelManager,
    &'a ConnectionManager,
);

pub(crate) async fn forward(
    command: &IncomingCommand,
    target_id: &str,
    runtime: Runtime<'_>,
) -> Option<RichMessage> {
    let text = command.command_text.trim();
    if text.is_empty()
        || command.callback_data.is_some()
        || text.starts_with("操作 iywt:")
        || !command.attachments.is_empty()
        || (text.starts_with('/') && text != "/skip")
    {
        return None;
    }
    let routes = runtime.1.reply_routes();
    let (matches, error) = current_routes(command, target_id, runtime).await;
    let route = match matches.as_slice() {
        [] => return error.map(RichMessage::error),
        [route] => route,
        _ => {
            return Some(RichMessage::info(
                "有多个原任务正在等你回答，请在对应的原对话中回复。",
            ))
        }
    };
    let before_delivery = Instant::now();
    let result = deliver(command, route, runtime).await;
    tracing::info!(
        channel_id = command.channel_id,
        conversation_id = route.source.conversation_id,
        trace_id = command.message_trace_id,
        accepted = result.is_ok(),
        "[channel-reply] original conversation reply processed"
    );
    Some(match result {
        Ok(message) => {
            routes.consume(&route.source, before_delivery);
            RichMessage::info(message)
        }
        Err(error) => RichMessage::error(error),
    })
}

async fn current_routes(
    command: &IncomingCommand,
    target_id: &str,
    runtime: Runtime<'_>,
) -> (Vec<ReplyRoute>, Option<String>) {
    let mut current = Vec::new();
    let mut error = None;
    for route in runtime.1.reply_routes().matching(command, target_id) {
        match pending_question(&route, runtime).await {
            Ok(_) => current.push(route),
            Err(reason) => error = Some(reason),
        }
    }
    (current, error)
}

async fn deliver(
    command: &IncomingCommand,
    route: &ReplyRoute,
    runtime: Runtime<'_>,
) -> Result<&'static str, String> {
    let pending = pending_question(route, runtime).await?;
    if let Some(pending) = pending {
        if pending.questions.iter().any(|question| question.secret) {
            return Err("请在原对话填写敏感字段。".into());
        }
        let answer = parse_answer(&pending.questions, command.command_text.trim())?;
        if runtime
            .2
            .try_answer_question(&route.source.connection_id, &pending.question_id, answer)
            .await
            .map_err(|error| format!("回答未通过校验，请重新填写：{error}"))?
        {
            return Ok("回答已传回原对话，继续处理原任务。");
        }
    }
    if command.command_text.trim() == "/skip" {
        return Ok("原问题已经结束，无需跳过。");
    }
    queue_reply(command, route, runtime).await?;
    Ok("回答已传回原对话，将继续处理原任务。")
}

async fn pending_question(
    route: &ReplyRoute,
    runtime: Runtime<'_>,
) -> Result<Option<PendingQuestionState>, String> {
    let Some(state) = runtime.2.get_state(&route.source.connection_id).await else {
        runtime.1.reply_routes().finish(&route.source);
        tracing::info!(
            conversation_id = route.source.conversation_id,
            reason = "connection_closed",
            "[channel-reply] discarded stale reply route"
        );
        return Err("原任务连接已关闭，请打开原对话继续。".into());
    };
    let state = state.read().await;
    if state.conversation_id != Some(route.source.conversation_id)
        || state.turn_generation != route.source.turn_generation
        || state.is_delegation_child
        || matches!(
            state.status,
            ConnectionStatus::Disconnected | ConnectionStatus::Error
        )
    {
        runtime.1.reply_routes().finish(&route.source);
        tracing::info!(
            conversation_id = route.source.conversation_id,
            reason = "source_changed",
            "[channel-reply] discarded stale reply route"
        );
        return Err("原任务已切换或结束连接，请在原对话继续。".into());
    }
    Ok(state.pending_question.clone())
}

async fn queue_reply(
    command: &IncomingCommand,
    route: &ReplyRoute,
    runtime: Runtime<'_>,
) -> Result<(), String> {
    let text = format!(
        "用户通过微信回复此前的联系消息：\n{}",
        command.command_text.trim()
    );
    let payload = AgentInputPayload {
        blocks: vec![PromptInputBlock::Text { text: text.clone() }],
        display_text: text,
        mode_id: None,
    };
    runtime
        .2
        .submit_agent_input(
            &AppDatabase {
                conn: runtime.0.clone(),
            },
            &route.source.connection_id,
            route.source.conversation_id,
            format!("channel-reply:{}:{}", route.id, command.message_trace_id),
            payload,
        )
        .await
        .map(|_| ())
        .map_err(|error| format!("回答未能传回原对话，请重试：{error}"))
}
