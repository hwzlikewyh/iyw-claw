//! 更新暂停使用原连接和回合身份，不把更新中断记成用户取消。
use std::collections::BTreeMap;
use std::time::Duration;

use sea_orm::{DatabaseConnection, EntityTrait};
use serde::{Deserialize, Serialize};

use super::connection::ConnectionCommand;
use super::manager::ConnectionManager;
use super::operation_guard::active_operation_reason;
use super::session_state::SessionState;
use super::types::SessionConfigKindInfo;
use crate::models::AgentType;

const PAUSE_TIMEOUT: Duration = Duration::from_secs(30);
const PAUSE_POLL: Duration = Duration::from_millis(100);

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct UpdateTask {
    pub input_id: String,
    pub connection_id: String,
    pub conversation_id: i32,
    pub session_id: String,
    pub agent_type: AgentType,
    pub working_dir: Option<String>,
    pub mode: Option<String>,
    pub config: BTreeMap<String, String>,
    pub generation: i64,
    pub resume_turn: bool,
    pub armed: bool,
}

pub(crate) async fn capture(
    manager: &ConnectionManager,
    db: &DatabaseConnection,
) -> Result<Vec<UpdateTask>, String> {
    let states = manager
        .connections
        .lock()
        .await
        .values()
        .map(|connection| connection.state.clone())
        .collect::<Vec<_>>();
    let mut tasks = Vec::new();
    for state in states {
        let snapshot = state.read().await;
        if active_operation_reason(&snapshot, chrono::Utc::now()).is_some() {
            let task = capture_one(&snapshot)?;
            drop(snapshot);
            use crate::db::entities::conversation::{self, ConversationStatus};
            let row = conversation::Entity::find_by_id(task.conversation_id)
                .one(db)
                .await
                .map_err(|error| error.to_string())?
                .ok_or("update_blocked:unrecoverable_task")?;
            if row.deleted_at.is_none()
                && !matches!(
                    row.status,
                    ConversationStatus::Cancelled | ConversationStatus::Completed
                )
            {
                tasks.push(task);
            }
        }
    }
    Ok(tasks)
}

fn capture_one(state: &SessionState) -> Result<UpdateTask, String> {
    validate_recoverable(state)?;
    let config = state
        .config_options
        .iter()
        .flatten()
        .map(|option| {
            let SessionConfigKindInfo::Select(select) = &option.kind;
            (option.id.clone(), select.current_value.clone())
        })
        .collect();
    Ok(UpdateTask {
        input_id: uuid::Uuid::new_v4().to_string(),
        connection_id: state.connection_id.clone(),
        conversation_id: state
            .conversation_id
            .ok_or("update_blocked:unrecoverable_task")?,
        session_id: state
            .external_id
            .clone()
            .ok_or("update_blocked:unrecoverable_task")?,
        agent_type: state.agent_type,
        working_dir: state
            .working_dir
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned()),
        mode: state.current_mode.clone(),
        config,
        generation: state.turn_generation,
        resume_turn: state.turn_in_flight || state.turn_completion_pending,
        armed: false,
    })
}

fn validate_recoverable(state: &SessionState) -> Result<(), String> {
    if state.pending_permission.is_some()
        || state.pending_question.is_some()
        || state.pending_channel_confirmation.is_some()
    {
        return Err("update_blocked:interaction_pending".into());
    }
    // 宿主委托、渠道及独立后台进程有自己的交付身份，不能作为普通会话重建。
    if !state.recoverable_session
        || state.recovery_failed
        || !state.launch_finalized
        || state.is_delegation_child
        || !state.active_delegations.is_empty()
        || state.owner_window_label.starts_with("chat_channel:")
        || state.native_background_turn.is_some()
        || state.has_active_background_work(chrono::Utc::now())
    {
        return Err("update_blocked:unrecoverable_task".into());
    }
    Ok(())
}

pub(crate) async fn pause(manager: &ConnectionManager, task: &UpdateTask) -> Result<bool, String> {
    let (state, sender, prompt_lock) = {
        let connections = manager.connections.lock().await;
        let Some(connection) = connections.get(&task.connection_id) else {
            return Ok(false);
        };
        (
            connection.state.clone(),
            connection.cmd_tx.clone(),
            connection.prompt_lock.clone(),
        )
    };
    let _prompt = prompt_lock.lock().await;
    {
        let snapshot = state.write().await;
        if snapshot.turn_generation != task.generation {
            return Ok(false);
        }
        if !task.resume_turn {
            return Ok(true);
        }
        if !snapshot.turn_in_flight && !snapshot.turn_completion_pending {
            return Ok(false);
        }
        sender
            .try_send(ConnectionCommand::Cancel)
            .map_err(|error| error.to_string())?;
    }
    wait_paused(&state, task.generation).await
}

async fn wait_paused(
    state: &tokio::sync::RwLock<SessionState>,
    generation: i64,
) -> Result<bool, String> {
    let deadline = tokio::time::Instant::now() + PAUSE_TIMEOUT;
    loop {
        let snapshot = state.read().await;
        if !snapshot.turn_in_flight && !snapshot.turn_completion_pending {
            let ended_normally =
                snapshot
                    .last_completed_turn_harvest
                    .as_ref()
                    .is_some_and(|turn| {
                        turn.turn_generation == generation && turn.stop_reason != "cancelled"
                    });
            return Ok(!ended_normally);
        }
        drop(snapshot);
        if tokio::time::Instant::now() >= deadline {
            return Err("update_blocked:pause_timeout".into());
        }
        tokio::time::sleep(PAUSE_POLL).await;
    }
}
