use sea_orm::EntityTrait;
use tauri::Manager;

use super::desktop::Task;
use crate::acp::manager::ConnectionManager;
use crate::acp::types::PromptInputBlock;
use crate::acp::update_tasks::UpdateTask;
use crate::acp::{AgentInputPayload, AgentInputStatus, InputDelivery};
use crate::db::entities::conversation::{self, ConversationStatus};
use crate::db::service::agent_input_outbox_service;
use crate::db::AppDatabase;

const CONTINUE_TASK: &str = "应用刚因用户要求更新而暂停并重启。请在当前会话中继续更新前尚未完成的任务，沿用之前的目标、约束和授权。先核对会话记录、工具结果和实际产物；不要从头重复执行，已完成的步骤不要重做。中断时结果未知的外部操作先核实，不要盲目重发。仍需用户确认的操作继续等待确认。";

pub(super) async fn resume(task: &Task, entry: &UpdateTask) -> Result<(), String> {
    if !entry.armed {
        return Ok(());
    }
    let db = task.app.state::<AppDatabase>();
    if !eligible(&db, entry).await? {
        return Ok(());
    }
    let manager = task.app.state::<ConnectionManager>();
    let connection = connect(task, entry).await?;
    verify_connection(&manager, &connection, entry).await?;
    if entry.resume_turn {
        submit_continuation(task, entry, &connection).await?;
    }
    manager
        .resume_agent_input_connection(&db, &connection, entry.conversation_id)
        .await
        .map_err(|error| error.to_string())?;
    if entry.resume_turn {
        wait_consumed(&db, &entry.input_id).await?;
    }
    tracing::info!(conversation_id = entry.conversation_id, input_id = %entry.input_id,
        "[app-update] task continuation accepted in original session");
    Ok(())
}

async fn wait_consumed(db: &AppDatabase, id: &str) -> Result<(), String> {
    const DELIVERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
    const DELIVERY_POLL: std::time::Duration = std::time::Duration::from_millis(250);
    let deadline = tokio::time::Instant::now() + DELIVERY_TIMEOUT;
    loop {
        let item = agent_input_outbox_service::get(&db.conn, id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or("Task continuation input is missing")?;
        match item.status {
            AgentInputStatus::Consumed | AgentInputStatus::Deleted => return Ok(()),
            AgentInputStatus::Failed => {
                return Err(item
                    .last_error
                    .unwrap_or_else(|| "Task continuation failed".into()))
            }
            _ => {}
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(
                "Task continuation is still queued; its recovery record was retained".into(),
            );
        }
        tokio::time::sleep(DELIVERY_POLL).await;
    }
}

async fn eligible(db: &AppDatabase, entry: &UpdateTask) -> Result<bool, String> {
    let Some(row) = conversation::Entity::find_by_id(entry.conversation_id)
        .one(&db.conn)
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(false);
    };
    if row.deleted_at.is_some()
        || row.external_id.as_deref() != Some(&entry.session_id)
        || row.status == ConversationStatus::Completed
        || (row.status == ConversationStatus::PendingReview
            && row.last_completed_turn_generation >= entry.generation)
    {
        return Ok(false);
    }
    if let Some(input) = agent_input_outbox_service::get(&db.conn, &entry.input_id)
        .await
        .map_err(|error| error.to_string())?
    {
        if input.status == AgentInputStatus::Consumed || input.status == AgentInputStatus::Deleted {
            return Ok(false);
        }
        if matches!(
            input.status,
            AgentInputStatus::Dispatching | AgentInputStatus::Failed
        ) {
            return Err(
                "Task continuation delivery is uncertain; inspect the original conversation".into(),
            );
        }
    }
    Ok(true)
}

async fn connect(task: &Task, entry: &UpdateTask) -> Result<String, String> {
    let manager = task.app.state::<ConnectionManager>();
    let db = task.app.state::<AppDatabase>();
    let data_dir = crate::system_skills::data_dir_from_env();
    let environment = crate::commands::acp::build_session_runtime_env(
        &db,
        entry.agent_type,
        Some(&entry.session_id),
        &data_dir,
    )
    .await
    .map_err(|error| error.to_string())?;
    crate::commands::acp::verify_agent_installed(entry.agent_type, &environment)
        .map_err(|error| error.to_string())?;
    crate::acp::account_credentials::sync_agent_credentials_for_acp(&db.conn, entry.agent_type)
        .await
        .map_err(|error| error.to_string())?;
    let trace =
        crate::acp::startup_trace::StartupTrace::new(entry.agent_type, true, "update_recovery");
    manager
        .spawn_agent_traced(
            entry.agent_type,
            entry.working_dir.clone(),
            Some(entry.session_id.clone()),
            Some(entry.conversation_id),
            environment,
            "main".into(),
            task.emitter(),
            entry.mode.clone(),
            entry.config.clone(),
            false,
            None,
            None,
            trace,
        )
        .await
        .map_err(|error| error.to_string())
}

async fn submit_continuation(
    task: &Task,
    entry: &UpdateTask,
    connection: &str,
) -> Result<(), String> {
    let db = task.app.state::<AppDatabase>();
    let manager = task.app.state::<ConnectionManager>();
    let item = manager
        .submit_agent_input(
            &db,
            connection,
            entry.conversation_id,
            entry.input_id.clone(),
            AgentInputPayload {
                blocks: vec![PromptInputBlock::Text {
                    text: CONTINUE_TASK.into(),
                }],
                display_text: "更新后继续未完成任务".into(),
                mode_id: entry.mode.clone(),
                delivery: InputDelivery::Queue,
            },
        )
        .await
        .map_err(|error| error.to_string())?;
    if matches!(
        item.status,
        AgentInputStatus::Failed | AgentInputStatus::Deleted
    ) {
        return Err(
            "Task continuation was not accepted; inspect the conversation before retrying".into(),
        );
    }
    Ok(())
}

async fn verify_connection(
    manager: &ConnectionManager,
    connection: &str,
    entry: &UpdateTask,
) -> Result<(), String> {
    let state = manager
        .get_state(connection)
        .await
        .ok_or("Restored task connection is missing")?;
    let state = state.read().await;
    if state.external_id.as_deref() != Some(entry.session_id.as_str()) || state.recovery_failed {
        return Err("Original task session could not be restored".into());
    }
    if state.turn_in_flight
        || state.turn_completion_pending
        || state.pending_permission.is_some()
        || state.pending_question.is_some()
    {
        return Err(
            "Original task is still active or waiting for confirmation; recovery was deferred"
                .into(),
        );
    }
    Ok(())
}
