//! 旧模型的本地摘要失败时，用当前已选模型恢复，保持历史提交的原有边界。

use super::*;
use crate::compact_model_fallback::record_model_fallback;
use crate::compact_model_fallback::should_retry_responses_with_current_model;

pub(super) struct ResponsesCompaction {
    pub(super) session: Arc<Session>,
    pub(super) step_context: Arc<StepContext>,
    pub(super) replacement_step_context: Arc<StepContext>,
    pub(super) input: Vec<ResolvedUserInput>,
    pub(super) world_state: Arc<WorldState>,
    pub(super) metadata: CompactionTurnMetadata,
    pub(super) item: TurnItem,
}

impl ResponsesCompaction {
    pub(super) async fn run(&self) -> CodexResult<String> {
        let turn = &self.step_context.turn;
        if !self.session.defer_compaction_start(turn, self.metadata.phase()) {
            self.session.emit_turn_item_started(turn, &self.item).await;
        }
        let history_version = self.session.clone_history().await.history_version();
        let result = self.attempt(Arc::clone(&self.step_context)).await;
        let Err(error) = &result else {
            return result;
        };
        let current = &self.replacement_step_context.turn;
        if !should_retry_responses_with_current_model(error, turn, current)
            || self.session.clone_history().await.history_version() != history_version
        {
            return result;
        }
        tracing::warn!(
            reason = ?self.metadata.reason(),
            "previous-model local compaction failed before history commit; trying the selected model"
        );
        self.session
            .set_last_known_step_context(&self.replacement_step_context)
            .await;
        let fallback = self.attempt(Arc::clone(&self.replacement_step_context)).await;
        record_model_fallback(
            &self.session.services.session_telemetry,
            &turn.model_info().slug,
            &current.model_info().slug,
            self.metadata.reason(),
            self.metadata.implementation(),
            fallback.as_ref().err(),
        );
        fallback
    }

    async fn attempt(&self, step_context: Arc<StepContext>) -> CodexResult<String> {
        run_compact_task_inner_impl(
            Arc::clone(&self.session),
            step_context,
            Arc::clone(&self.replacement_step_context),
            self.input.clone(),
            Arc::clone(&self.world_state),
            (self.metadata, self.item.clone()),
        )
        .await
    }
}
