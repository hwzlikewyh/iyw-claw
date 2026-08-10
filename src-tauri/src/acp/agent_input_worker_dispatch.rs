use std::time::Duration;

use crate::acp::agent_input_capabilities::feedback_text;
use crate::acp::agent_input_dispatch::emit_current;
use crate::acp::agent_input_worker::{WorkerContext, WorkerSnapshot};
use crate::acp::{AgentInputItem, AgentInputStatus, AgentInputStrategy};
use crate::db::service::agent_input_outbox_service;

const DISPATCH_CLAIM_RETRY_DELAY: Duration = Duration::from_secs(1);

pub(super) async fn dispatch_feedback(
    context: &WorkerContext<'_>,
    item: &AgentInputItem,
    snapshot: &WorkerSnapshot,
) {
    let Some(text) = feedback_text(&item.payload) else {
        return;
    };
    if !context
        .mark_dispatching(
            item,
            snapshot.turn_generation,
            AgentInputStrategy::CooperativeFeedback,
        )
        .await
    {
        return;
    }
    emit_current(context.db, context.state, context.emitter, &item.id).await;
    if let Err(error) = context
        .manager
        .submit_feedback_with_id(context.conn_id, item.id.clone(), text)
        .await
    {
        tracing::warn!(input_id = %item.id, error = %error, "[agent-input] feedback dispatch fell back");
        context
            .transition_item(
                &item.id,
                AgentInputStatus::FallbackQueued,
                error.to_string(),
            )
            .await;
    }
}

pub(super) async fn dispatch_next(
    context: &WorkerContext<'_>,
    item: AgentInputItem,
    snapshot: WorkerSnapshot,
) {
    let target_generation = snapshot.turn_generation.saturating_add(1);
    if !context
        .mark_dispatching(&item, target_generation, AgentInputStrategy::DeferredNext)
        .await
    {
        return;
    }
    emit_current(context.db, context.state, context.emitter, &item.id).await;
    if let Some(mode_id) = item.payload.mode_id.clone() {
        if let Err(error) = context.manager.set_mode(context.conn_id, mode_id).await {
            context
                .transition_item(&item.id, AgentInputStatus::Failed, error.to_string())
                .await;
            return;
        }
    }
    let result = context
        .manager
        .send_prompt_linked_with_message_id(
            context.db,
            context.conn_id,
            item.payload.blocks,
            Some(snapshot.folder_id),
            Some(snapshot.conversation_id),
            None,
            Some(item.id.clone()),
        )
        .await;
    if let Err(error) = result {
        context
            .transition_item(&item.id, AgentInputStatus::Failed, error.to_string())
            .await;
    }
}

impl WorkerContext<'_> {
    pub(super) async fn recover_stale_dispatch(&self, item: &AgentInputItem) {
        let current_generation = self.state.read().await.turn_generation;
        let prompt_was_accepted = item.strategy == Some(AgentInputStrategy::DeferredNext)
            && item
                .target_turn_generation
                .is_some_and(|target| target <= current_generation);
        let (status, reason) = if prompt_was_accepted {
            (
                AgentInputStatus::Consumed,
                "dispatch_consumption_recovered_after_turn_settlement",
            )
        } else {
            (
                AgentInputStatus::FallbackQueued,
                "dispatch_claim_recovered_after_turn_settlement",
            )
        };
        tracing::warn!(
            input_id = %item.id,
            target_turn_generation = item.target_turn_generation,
            current_generation,
            strategy = item.strategy.map(AgentInputStrategy::as_str),
            recovered_status = status.as_str(),
            "[agent-input] recovering stale dispatch claim"
        );
        if !self.transition_item(&item.id, status, reason.into()).await {
            tokio::time::sleep(DISPATCH_CLAIM_RETRY_DELAY).await;
        }
    }

    async fn mark_dispatching(
        &self,
        item: &AgentInputItem,
        generation: i64,
        strategy: AgentInputStrategy,
    ) -> bool {
        match agent_input_outbox_service::mark_dispatching(
            &self.db.conn,
            &item.id,
            self.conn_id,
            generation,
            strategy,
        )
        .await
        {
            Ok(changed) => changed,
            Err(error) => {
                tracing::error!(input_id = %item.id, error = %error, "[agent-input] dispatch claim failed");
                tokio::time::sleep(DISPATCH_CLAIM_RETRY_DELAY).await;
                false
            }
        }
    }

    async fn transition_item(&self, id: &str, status: AgentInputStatus, reason: String) -> bool {
        let changed = agent_input_outbox_service::transition_status(
            &self.db.conn,
            id,
            AgentInputStatus::Dispatching,
            status,
            Some(reason),
        )
        .await;
        match changed {
            Ok(true) => {
                emit_current(self.db, self.state, self.emitter, id).await;
                true
            }
            Ok(false) => false,
            Err(error) => {
                tracing::error!(input_id = id, error = %error, "[agent-input] terminal transition failed");
                false
            }
        }
    }
}
