use super::*;

impl UpstreamClient {
    pub(crate) async fn observe_goal_turn(
        &self,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<(), UpstreamError> {
        let mut harness = self.harness.lock().await;
        if let Some(active) = harness.active_turn_for(thread_id) {
            return if active.turn_id == turn_id {
                Ok(())
            } else {
                Err(UpstreamError::InvalidResponse(
                    "goal turn overlaps the active turn".into(),
                ))
            };
        }
        let binding = harness
            .binding(thread_id)
            .ok_or_else(|| UpstreamError::InvalidRequest("unknown Codex goal thread".into()))?;
        let access = SessionAccess {
            external_id: &binding.external_id,
            connection_id: &binding.connection_id,
            generation: binding.generation,
            runtime_fingerprint: &binding.runtime_fingerprint,
        };
        harness.begin_turn(access, TurnBinding::new(thread_id, turn_id)?)?;
        Ok(())
    }
}
