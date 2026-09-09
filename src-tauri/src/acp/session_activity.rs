use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::runtime_observation::RuntimeObservation;
use super::session_state::{ToolCallState, ToolCallStatus};
use super::types::{AcpEvent, ConnectionStatus};

#[path = "session_activity_tools.rs"]
mod tools;

const EMIT_INTERVAL: Duration = Duration::from_secs(1);
const MAX_PROCESS_OBSERVATIONS: usize = 16;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProcessObservation {
    pub turn_generation: i64,
    pub item_id: String,
    pub process_id: String,
    pub checked_at: DateTime<Utc>,
    pub output_at: Option<DateTime<Utc>>,
    pub status: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionActivitySnapshot {
    pub turn_generation: i64,
    pub started_at: Option<DateTime<Utc>>,
    pub text_at: Option<DateTime<Utc>>,
    pub thinking_at: Option<DateTime<Utc>>,
    pub tool_output_at: Option<DateTime<Utc>>,
    pub tool_started_at: Option<DateTime<Utc>>,
    pub retrying_since: Option<DateTime<Utc>>,
    pub processes: Vec<ProcessObservation>,
    pub sampled_at: DateTime<Utc>,
}

#[derive(Debug, Default)]
pub struct SessionActivity {
    pub(super) snapshot: SessionActivitySnapshot,
    last_emitted: Option<Instant>,
    pub(super) urgent: bool,
}

impl SessionActivity {
    pub fn snapshot(&self) -> SessionActivitySnapshot {
        let mut snapshot = self.snapshot.clone();
        snapshot.sampled_at = Utc::now();
        snapshot
    }

    pub fn take_update(&mut self) -> Option<SessionActivitySnapshot> {
        if !self.urgent
            && self
                .last_emitted
                .is_some_and(|at| at.elapsed() < EMIT_INTERVAL)
        {
            return None;
        }
        self.urgent = false;
        self.last_emitted = Some(Instant::now());
        Some(self.snapshot())
    }

    pub fn observe(&mut self, event: &AcpEvent, context: ActivityContext<'_>) -> bool {
        let now = Utc::now();
        match event {
            AcpEvent::StatusChanged {
                status: ConnectionStatus::Prompting,
            }
            | AcpEvent::UserMessage { .. } => {
                self.begin_turn(context.generation, now);
                true
            }
            AcpEvent::StatusChanged {
                status: ConnectionStatus::Disconnected | ConnectionStatus::Error,
            } => {
                self.mark_processes_unknown();
                false
            }
            AcpEvent::RuntimeObservation { observation } => {
                self.observe_runtime(observation, context.tools, now)
            }
            AcpEvent::ContentDelta { text } if context.prompting && !text.is_empty() => {
                self.urgent |= self.snapshot.text_at.is_none();
                self.snapshot.text_at = Some(now);
                self.clear_retry();
                true
            }
            AcpEvent::Thinking { text } if context.prompting && !text.is_empty() => {
                self.urgent |= self.snapshot.thinking_at.is_none();
                self.snapshot.thinking_at = Some(now);
                self.clear_retry();
                true
            }
            _ => self.observe_tool_or_boundary(event, context, now),
        }
    }

    fn begin_turn(&mut self, generation: i64, now: DateTime<Utc>) {
        if self.snapshot.turn_generation == generation && self.snapshot.started_at.is_some() {
            return;
        }
        let processes = std::mem::take(&mut self.snapshot.processes);
        self.snapshot = SessionActivitySnapshot {
            turn_generation: generation,
            started_at: Some(now),
            processes,
            ..Default::default()
        };
        self.urgent = true;
    }

    pub(super) fn clear_retry(&mut self) {
        if self.snapshot.retrying_since.take().is_some() {
            tracing::info!(
                turn_generation = self.snapshot.turn_generation,
                "[ACP] runtime retry observation settled"
            );
            self.urgent = true;
        }
    }

    fn mark_processes_unknown(&mut self) {
        for process in &mut self.snapshot.processes {
            if process.status == "running" {
                process.status = "unknown".to_string();
            }
        }
        self.urgent = true;
        self.clear_retry();
    }

    fn observe_runtime(
        &mut self,
        observation: &RuntimeObservation,
        tools: &BTreeMap<String, ToolCallState>,
        now: DateTime<Utc>,
    ) -> bool {
        match observation {
            RuntimeObservation::Retry => {
                if self.snapshot.retrying_since.is_none() {
                    self.snapshot.retrying_since = Some(now);
                    tracing::info!("[ACP] runtime reported retrying the active turn");
                }
            }
            RuntimeObservation::TerminalPoll {
                item_id,
                process_id,
            } => {
                if !tools.contains_key(item_id)
                    && !self
                        .snapshot
                        .processes
                        .iter()
                        .any(|process| process.item_id == *item_id)
                {
                    return false;
                }
                if tools.get(item_id).is_some_and(|tool| {
                    matches!(
                        tool.status,
                        ToolCallStatus::Completed | ToolCallStatus::Failed
                    )
                }) {
                    return false;
                }
                if !self.record_poll(item_id, process_id, now) {
                    return false;
                }
            }
        }
        self.urgent = true;
        true
    }

    fn record_poll(&mut self, item_id: &str, process_id: &str, now: DateTime<Utc>) -> bool {
        let generation = self.snapshot.turn_generation;
        if let Some(process) = self
            .snapshot
            .processes
            .iter_mut()
            .find(|p| p.item_id == item_id)
        {
            if process.process_id != process_id || process.status != "running" {
                return false;
            }
            process.checked_at = now;
            process.turn_generation = generation;
            return true;
        }
        if self.snapshot.processes.len() >= MAX_PROCESS_OBSERVATIONS {
            self.snapshot.processes.remove(0);
        }
        self.snapshot.processes.push(ProcessObservation {
            turn_generation: generation,
            item_id: item_id.to_string(),
            process_id: process_id.to_string(),
            checked_at: now,
            output_at: None,
            status: "running".to_string(),
        });
        tracing::info!(
            turn_generation = generation,
            item_id,
            process_id,
            "[ACP] background command first observed running"
        );
        true
    }
}

pub struct ActivityContext<'a> {
    pub generation: i64,
    pub prompting: bool,
    pub tools: &'a BTreeMap<String, ToolCallState>,
}
