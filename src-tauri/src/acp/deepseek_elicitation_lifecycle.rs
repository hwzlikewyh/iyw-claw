use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::watch;

use super::{QuestionLifecycle, SessionState, QUESTION_STATE_INTERVAL};

#[derive(Default, Clone)]
pub(super) struct ElicitationRegistry {
    pending: Arc<Mutex<HashMap<String, watch::Sender<bool>>>>,
}

pub(super) struct ElicitationLease {
    registry: ElicitationRegistry,
    id: String,
    cancelled: watch::Receiver<bool>,
}

impl ElicitationRegistry {
    pub(super) fn register(&self, id: &str) -> Option<ElicitationLease> {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(cancel) = pending.get(id) {
            if *cancel.borrow() {
                pending.remove(id);
            }
            return None;
        }
        let (cancel, cancelled) = watch::channel(false);
        pending.insert(id.to_string(), cancel);
        Some(ElicitationLease {
            registry: self.clone(),
            id: id.to_string(),
            cancelled,
        })
    }

    pub(super) fn cancel(&self, id: &str) {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(cancel) = pending.get(id) {
            let _ = cancel.send(true);
        } else {
            const MAX_EARLY_CANCELLATIONS: usize = 1024;
            if pending.len() < MAX_EARLY_CANCELLATIONS {
                pending.insert(id.to_string(), watch::channel(true).0);
            }
        }
    }
}

impl ElicitationLease {
    fn is_cancelled(&self) -> bool {
        *self.cancelled.borrow()
    }

    pub(super) async fn cancelled(&mut self) {
        let _ = self.cancelled.wait_for(|cancelled| *cancelled).await;
    }
}

impl Drop for ElicitationLease {
    fn drop(&mut self) {
        self.registry
            .pending
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.id);
    }
}

pub(super) async fn wait_for_answer(
    registered: &mut crate::acp::question::RegisteredQuestion,
    context: (&Arc<tokio::sync::RwLock<SessionState>>, &str),
    lifecycle: &mut QuestionLifecycle,
) -> Option<crate::acp::question::QuestionOutcome> {
    let (state, connection_id) = context;
    let track_turn = lifecycle.track_turn;
    let generation = lifecycle.turn_generation;
    let questions = Arc::clone(&lifecycle.questions);
    let pending = &mut lifecycle.request;
    let deadline = lifecycle.deadline;
    let mut interval = tokio::time::interval(QUESTION_STATE_INTERVAL);
    loop {
        tokio::select! {
            _ = async {
                match deadline {
                    Some(deadline) => tokio::time::sleep_until(deadline).await,
                    None => std::future::pending::<()>().await,
                }
            } => {
                questions.cancel_question(connection_id, &registered.question_id).await;
                return None;
            }
            answer = &mut registered.answer_rx => {
                let cancelled = pending.as_ref().is_some_and(ElicitationLease::is_cancelled);
                if cancelled || (track_turn && stale_turn(state, generation).await) { return None; }
                return answer.ok();
            }
            _ = async {
                match pending.as_mut() {
                    Some(request) => request.cancelled().await,
                    None => std::future::pending::<()>().await,
                }
            } => {
                questions.cancel_question(connection_id, &registered.question_id).await;
                return None;
            }
            _ = interval.tick(), if track_turn => {
                if stale_turn(state, generation).await {
                    questions.cancel_question(connection_id, &registered.question_id).await;
                    return None;
                }
            }
        }
    }
}

async fn stale_turn(state: &Arc<tokio::sync::RwLock<SessionState>>, generation: i64) -> bool {
    let snapshot = state.read().await;
    snapshot.turn_generation != generation || !snapshot.turn_in_flight
}

pub(super) async fn wait_for_form(
    plan: &super::FormPlan,
    registered: &mut crate::acp::question::RegisteredQuestion,
    context: (
        &Arc<tokio::sync::RwLock<SessionState>>,
        &str,
        &mut QuestionLifecycle,
    ),
) -> Option<crate::acp::question::QuestionOutcome> {
    let (state, connection_id, lifecycle) = context;
    let mut result = wait_for_answer(registered, (state, connection_id), lifecycle).await?;
    if result.declined {
        return Some(result);
    }
    for questions in plan
        .specs()
        .chunks(crate::acp::question::MAX_QUESTIONS)
        .skip(1)
    {
        if lifecycle.track_turn && stale_turn(state, lifecycle.turn_generation).await {
            return None;
        }
        if lifecycle
            .request
            .as_ref()
            .is_some_and(ElicitationLease::is_cancelled)
        {
            return None;
        }
        *registered = lifecycle
            .questions
            .register_question(connection_id, questions.to_vec())
            .await?;
        let next = wait_for_answer(registered, (state, connection_id), lifecycle).await?;
        if next.declined {
            return Some(next);
        }
        result.answers.extend(next.answers);
    }
    Some(result)
}
