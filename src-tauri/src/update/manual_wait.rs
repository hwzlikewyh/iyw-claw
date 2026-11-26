use super::{desktop::Task, offer, preferences, silent_safety, state};
use crate::acp::manager::{ConnectionManager, RelaunchClaim};
use state::{AppUpdateLifecycle as Lifecycle, AppUpdateState};
use std::time::Duration;
use tauri::Manager;
const WAIT_POLL: Duration = Duration::from_secs(1);

pub(super) async fn wait(
    task: &Task,
    identity: &offer::Identity,
) -> Result<Option<RelaunchClaim>, String> {
    let waiting_id = uuid::Uuid::new_v4().to_string();
    state::mutate(&task.handle, &task.emitter(), |state| {
        state.status = Lifecycle::WaitingForTasks;
        state.error = None;
        state.waiting_id = Some(waiting_id.clone());
    });
    loop {
        let before = state::snapshot(&task.handle);
        if before.status != Lifecycle::WaitingForTasks
            || before.waiting_id.as_ref() != Some(&waiting_id)
        {
            return Ok(None);
        }
        if before.recovering_tasks {
            tokio::time::sleep(WAIT_POLL).await;
            continue;
        }
        let blocker = match current_blocker(task, identity).await {
            Ok(blocker) => blocker,
            Err(error) => {
                fail_waiting(task, &waiting_id, error);
                return Ok(None);
            }
        };
        if blocker.is_none() {
            let manager = task.app.state::<ConnectionManager>();
            if let Ok(claim) = manager.claim_relaunch().await {
                if transition_waiting((task, &waiting_id), Lifecycle::Installing, None).is_some() {
                    return Ok(Some(claim));
                }
                return Ok(None);
            }
        }
        if transition_waiting((task, &waiting_id), Lifecycle::WaitingForTasks, blocker).is_none() {
            return Ok(None);
        }
        tokio::time::sleep(WAIT_POLL).await;
    }
}

fn transition_waiting(
    (task, waiting_id): (&Task, &str),
    status: Lifecycle,
    blocker: Option<&str>,
) -> Option<AppUpdateState> {
    let snapshot = {
        let mut state = task
            .handle
            .write()
            .unwrap_or_else(|error| error.into_inner());
        if state.waiting_id.as_deref() != Some(waiting_id)
            || state.status != Lifecycle::WaitingForTasks
        {
            return None;
        }
        if state.status == status && state.blocked_reason.as_deref() == blocker {
            return Some(state.clone());
        }
        state.seq += 1;
        state.status = status;
        if status != Lifecycle::WaitingForTasks {
            state.waiting_id = None;
        }
        state.blocked_reason = blocker.map(str::to_owned);
        state.clone()
    };
    crate::web::event_bridge::emit_event(
        &task.emitter(),
        state::APP_UPDATE_STATE_CHANNEL,
        &snapshot,
    );
    Some(snapshot)
}

pub(super) fn cancel(task: &Task) -> AppUpdateState {
    let before = state::snapshot(&task.handle);
    let Some(id) = before.waiting_id.as_deref() else {
        return before;
    };
    transition_waiting((task, id), Lifecycle::ReadyToInstall, None).unwrap_or(before)
}

async fn current_blocker(
    task: &Task,
    identity: &offer::Identity,
) -> Result<Option<&'static str>, String> {
    let settings = preferences::load(&task.conn)
        .await
        .map_err(|error| error.to_string())?;
    if !super::promotion::accepts_channel(&settings, &identity.channel, identity.release_id.as_deref()) {
        return Err("Update channel changed".into());
    }
    silent_safety::task_blocker_with_idle_delay(&task.app, Duration::ZERO).await
}

fn fail_waiting(task: &Task, waiting_id: &str, error: String) {
    let snapshot = {
        let mut state = task
            .handle
            .write()
            .unwrap_or_else(|error| error.into_inner());
        if state.status != Lifecycle::WaitingForTasks
            || state.waiting_id.as_deref() != Some(waiting_id)
        {
            return;
        }
        state.seq += 1;
        state.status = Lifecycle::Error;
        state.waiting_id = None;
        state.blocked_reason = None;
        state.error = Some(error.clone());
        state.clone()
    };
    tracing::error!(%error, "[app-update] scheduled update failed");
    crate::web::event_bridge::emit_event(
        &task.emitter(),
        state::APP_UPDATE_STATE_CHANNEL,
        &snapshot,
    );
}
