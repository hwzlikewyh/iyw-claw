use std::time::Instant;

use super::{lock, StartupTrace};

#[derive(Debug)]
pub(super) struct TurnTrace {
    generation: i64,
    dispatched_at: Instant,
    first_event: bool,
    first_content: bool,
    first_thinking: bool,
}

impl StartupTrace {
    pub(crate) fn prompt_dispatched(&self, generation: i64) {
        *lock(&self.inner.turn) = Some(TurnTrace {
            generation,
            dispatched_at: Instant::now(),
            first_event: false,
            first_content: false,
            first_thinking: false,
        });
        self.first_prompt_dispatched();
        self.log_turn("turn_dispatch", generation, 0);
    }

    pub(crate) fn observe_turn_event(&self, generation: i64, kind: &'static str) {
        let observation = {
            let mut turn = lock(&self.inner.turn);
            let Some(turn) = turn.as_mut() else {
                return;
            };
            if turn.generation != generation {
                return;
            }
            let first_event = !turn.first_event;
            let first_content = kind == "content" && !turn.first_content;
            let first_thinking = kind == "thinking" && !turn.first_thinking;
            turn.first_event = true;
            turn.first_content |= first_content;
            turn.first_thinking |= first_thinking;
            (
                turn.generation,
                turn.dispatched_at.elapsed().as_millis(),
                first_event,
                first_content,
                first_thinking,
            )
        };
        if observation.2 {
            self.log_turn("turn_first_event", observation.0, observation.1);
        }
        if observation.3 {
            self.log_turn("turn_first_content", observation.0, observation.1);
        }
        if observation.4 {
            self.log_turn("turn_first_thinking", observation.0, observation.1);
        }
    }

    fn log_turn(&self, stage: &'static str, generation: i64, duration_ms: u128) {
        let connection_id = lock(&self.inner.connection_id).clone().unwrap_or_default();
        tracing::info!(startup_trace_id = self.inner.id, connection_id,
            agent = %self.inner.agent_type, turn_generation = generation, stage,
            duration_ms, "[ACP][turn-timing] stage");
    }
}
