use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

use crate::models::agent::AgentType;

#[derive(Clone, Debug)]
pub(crate) struct StartupTrace {
    inner: Arc<StartupTraceInner>,
}

#[derive(Debug)]
struct StartupTraceInner {
    id: String,
    agent_type: AgentType,
    resumed: bool,
    source: &'static str,
    accepted_at: Instant,
    connection_id: Mutex<Option<String>>,
    host_key: Mutex<Option<String>>,
    first_prompt_logged: AtomicBool,
}

pub(crate) struct StartupStage {
    trace: StartupTrace,
    name: &'static str,
    started_at: Instant,
    finished: bool,
}

impl StartupTrace {
    pub(crate) fn new(agent_type: AgentType, resumed: bool, source: &'static str) -> Self {
        let trace = Self {
            inner: Arc::new(StartupTraceInner {
                id: uuid::Uuid::new_v4().to_string(),
                agent_type,
                resumed,
                source,
                accepted_at: Instant::now(),
                connection_id: Mutex::new(None),
                host_key: Mutex::new(None),
                first_prompt_logged: AtomicBool::new(false),
            }),
        };
        trace.log("request_accepted", "started", Duration::ZERO);
        trace
    }

    pub(crate) fn bind_connection(&self, connection_id: impl Into<String>) {
        *lock(&self.inner.connection_id) = Some(connection_id.into());
    }

    pub(crate) fn bind_host_key(&self, host_key: impl Into<String>) {
        *lock(&self.inner.host_key) = Some(host_key.into());
    }

    pub(crate) fn stage(&self, name: &'static str) -> StartupStage {
        self.log(name, "started", Duration::ZERO);
        StartupStage {
            trace: self.clone(),
            name,
            started_at: Instant::now(),
            finished: false,
        }
    }

    pub(crate) fn record(&self, name: &'static str, outcome: &'static str, elapsed: Duration) {
        self.log(name, outcome, elapsed);
    }

    pub(crate) fn first_prompt_dispatched(&self) {
        if self.inner.first_prompt_logged.swap(true, Ordering::AcqRel) {
            return;
        }
        self.log(
            "first_prompt_dispatch",
            "dispatched",
            self.inner.accepted_at.elapsed(),
        );
    }

    fn log(&self, stage: &'static str, outcome: &'static str, elapsed: Duration) {
        let connection_id = lock(&self.inner.connection_id).clone().unwrap_or_default();
        let host_key = lock(&self.inner.host_key).clone().unwrap_or_default();
        tracing::info!(
            startup_trace_id = self.inner.id,
            connection_id,
            agent = %self.inner.agent_type,
            resumed = self.inner.resumed,
            source = self.inner.source,
            host_key,
            stage,
            outcome,
            duration_ms = elapsed.as_millis(),
            since_accepted_ms = self.inner.accepted_at.elapsed().as_millis(),
            "[ACP][startup] stage"
        );
    }
}

impl StartupStage {
    pub(crate) fn finish(mut self, outcome: &'static str) {
        self.trace
            .log(self.name, outcome, self.started_at.elapsed());
        self.finished = true;
    }
}

impl Drop for StartupStage {
    fn drop(&mut self) {
        if !self.finished {
            self.trace
                .log(self.name, "aborted", self.started_at.elapsed());
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
