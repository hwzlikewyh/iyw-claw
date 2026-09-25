use std::time::Instant;

use crate::models::agent::AgentType;

// 一次环境准备使用一个诊断 ID；不记录配置内容、凭证或用户目录。
pub(super) struct RuntimeTiming {
    id: uuid::Uuid,
    agent: AgentType,
    resumed: bool,
    stage: &'static str,
    started: Instant,
    finished: bool,
    degraded: bool,
}

impl RuntimeTiming {
    pub(super) fn new(agent: AgentType, resumed: bool) -> Self {
        let value = Self {
            id: uuid::Uuid::new_v4(),
            agent,
            resumed,
            stage: "storage_settings",
            started: Instant::now(),
            finished: false,
            degraded: false,
        };
        value.log("started");
        value
    }

    pub(super) fn next(&mut self, stage: &'static str) {
        self.log(if self.degraded { "degraded" } else { "ok" });
        self.stage = stage;
        self.started = Instant::now();
        self.degraded = false;
        self.log("started");
    }

    pub(super) fn degraded(&mut self) {
        self.degraded = true;
    }

    pub(super) fn finish(mut self) {
        self.log(if self.degraded { "degraded" } else { "ok" });
        self.finished = true;
    }

    fn log(&self, outcome: &'static str) {
        tracing::info!(runtime_trace_id = %self.id, agent = %self.agent,
            resumed = self.resumed, stage = self.stage, outcome,
            duration_ms = self.started.elapsed().as_millis(),
            "[ACP][runtime-env] stage");
    }
}

impl Drop for RuntimeTiming {
    fn drop(&mut self) {
        if !self.finished {
            tracing::warn!(runtime_trace_id = %self.id, agent = %self.agent,
                resumed = self.resumed, stage = self.stage,
                duration_ms = self.started.elapsed().as_millis(),
                "[ACP][runtime-env] stage did not complete");
        }
    }
}
