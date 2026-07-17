mod accounting;
mod episode;
mod ledger;
mod state;
mod tail;

pub(crate) use ledger::PromptLedger;

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::acp::session_state::SessionState;
use crate::models::agent::AgentType;
use crate::web::event_bridge::{emit_with_state, EventEmitter};

use state::WatchState;

pub(crate) struct BackgroundWatchGuard(tokio::task::JoinHandle<()>);

impl Drop for BackgroundWatchGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub(crate) fn spawn_if_claude(
    connection_id: &str,
    agent_type: AgentType,
    state: Arc<RwLock<SessionState>>,
    emitter: EventEmitter,
    cwd: String,
    ledger: Arc<PromptLedger>,
) -> Option<BackgroundWatchGuard> {
    if agent_type != AgentType::ClaudeCode {
        return None;
    }
    let connection_id = connection_id.to_string();
    let handle = tokio::spawn(async move {
        run_watch(connection_id, state, emitter, cwd, ledger).await;
    });
    Some(BackgroundWatchGuard(handle))
}

async fn run_watch(
    connection_id: String,
    state: Arc<RwLock<SessionState>>,
    emitter: EventEmitter,
    cwd: String,
    ledger: Arc<PromptLedger>,
) {
    let spawn_epoch = std::time::SystemTime::now();
    let mut first_arm = true;
    let mut watcher: Option<WatchState> = None;
    loop {
        let delay = watcher
            .as_ref()
            .map(WatchState::poll_delay)
            .unwrap_or(std::time::Duration::from_secs(1));
        tokio::time::sleep(delay).await;

        let snapshot = {
            let state = state.read().await;
            (
                state.external_id.clone(),
                state.external_id_changed_at,
                state.status == crate::acp::types::ConnectionStatus::Prompting,
                state.last_turn_ended_abnormally,
            )
        };
        let Some(session_id) = snapshot.0 else {
            continue;
        };
        if watcher.as_ref().map(|value| value.session_id.as_str()) != Some(session_id.as_str()) {
            let epoch = if first_arm {
                spawn_epoch
            } else {
                snapshot.1.unwrap_or_else(std::time::SystemTime::now)
            };
            let Some(file) = crate::parsers::claude::find_session_file(&session_id) else {
                continue;
            };
            let offset = tail::baseline_offset_since(&file, epoch)
                .or_else(|| std::fs::metadata(&file).ok().map(|metadata| metadata.len()))
                .unwrap_or(0);
            match watcher.as_mut() {
                Some(value) => value.rearm(session_id.clone(), file, offset),
                None => watcher = Some(WatchState::new(session_id.clone(), file, offset)),
            }
            first_arm = false;
        }

        let Some(current) = watcher.take() else {
            continue;
        };
        let ledger = Arc::clone(&ledger);
        let cwd = cwd.clone();
        let connection_for_tick = connection_id.clone();
        let joined = tokio::task::spawn_blocking(move || {
            let mut current = current;
            let event = current.tick(&ledger, &cwd, &connection_for_tick, snapshot.2, snapshot.3);
            (current, event)
        })
        .await;
        let Ok((returned, event)) = joined else {
            continue;
        };
        watcher = Some(returned);
        if let Some(event) = event {
            emit_with_state(&state, &emitter, event).await;
        }
    }
}

#[cfg(test)]
mod tests;
