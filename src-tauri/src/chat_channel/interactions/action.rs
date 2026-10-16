use super::{question, Action, Pending};
use crate::acp::manager::ConnectionManager;
use crate::chat_channel::task_context;

pub(super) async fn execute(item: Pending, manager: &ConnectionManager) -> Result<String, String> {
    let connection_id = &item.task.connection_id;
    match item.action {
        Action::Task("status") => Ok(question::progress_text(
            crate::chat_channel::task_progress::view(&item.task).await,
        )),
        Action::Task(action) => {
            task_context::configure(&item.task.id, None, Some(action == "resume"))?;
            Ok("汇报设置已更新，任务继续执行。".into())
        }
        Action::Permission(request, option) => {
            super::permission(manager, connection_id, (&request, &option)).await
        }
        Action::Confirmation(id, confirmed) => {
            let state = manager.get_state(connection_id).await.ok_or("会话已结束")?;
            let active = state
                .read()
                .await
                .pending_channel_confirmation
                .as_ref()
                .is_some_and(|item| {
                    item.confirmation_id == id && item.expires_at > chrono::Utc::now()
                });
            if !active {
                return Err("确认请求已结束".into());
            }
            manager
                .respond_channel_confirmation(connection_id, &id, confirmed)
                .await;
            Ok("已提交确认。".into())
        }
        Action::Answer(id, field, label) => {
            answer(manager, connection_id, (&id, field, label)).await
        }
    }
}

async fn answer(
    manager: &ConnectionManager,
    connection_id: &str,
    choice: (&str, String, String),
) -> Result<String, String> {
    let (id, field, label) = choice;
    let state = manager.get_state(connection_id).await.ok_or("会话已结束")?;
    if !state
        .read()
        .await
        .pending_question
        .as_ref()
        .is_some_and(|question| question.question_id == id)
    {
        return Err("问题已结束，请使用最新消息。".into());
    }
    manager
        .answer_question(
            connection_id,
            &id,
            crate::acp::question::QuestionAnswer {
                answers: vec![crate::acp::question::QuestionAnswerItem {
                    question_id: field,
                    labels: vec![label],
                }],
                declined: false,
            },
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok("已提交回答。".into())
}
