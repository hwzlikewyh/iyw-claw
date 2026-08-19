use std::path::Path;
use std::time::Instant;

use serde::Serialize;

use crate::models::agent::AgentType;
use crate::user_memory::UserMemoryContextSnapshot;

use super::identity::{
    bounded_label, bounded_reason_code, connection_hash, conversation_hash, plan_id, workspace_hash,
};
use super::lifecycle::{apply_called_observations, memory_lifecycle, CapabilityLifecycleReceipt};
use super::memory_reason::memory_reason_codes;
use super::{HermesNativeMemoryDiagnostics, HermesNativeMemoryState};

const ESTIMATED_CHARS_PER_TOKEN: usize = 4;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MemoryGenerationsReceipt {
    historical_context_generation: String,
    recall_source_generation: &'static str,
    index_generation: String,
    ranking_version: &'static str,
}

pub(crate) struct ContextPlanStart<'a> {
    pub connection_id: &'a str,
    pub conversation_id: Option<i32>,
    pub workspace: Option<&'a Path>,
    pub turn_generation: i64,
    pub turn_nonce: u64,
    pub agent_type: AgentType,
    pub managed_agent_version: Option<&'a str>,
    pub hermes_memory: HermesNativeMemoryDiagnostics,
    pub hermes_shared_home_connections: Option<u16>,
    pub memory: &'a UserMemoryContextSnapshot,
    pub context_loaded: bool,
}

pub(crate) struct ContextPlanFinish<'a> {
    pub stop_reason: &'a str,
    pub memory_calls: crate::acp::memory_turn::MemoryCapabilityCalls,
}

#[derive(Debug)]
pub(crate) struct ContextPlanReceiptSeed {
    started_at: Instant,
    plan_id: String,
    connection_hash: String,
    conversation_hash: String,
    workspace_hash: String,
    turn_generation: i64,
    agent_type: AgentType,
    managed_agent_version: Option<String>,
    hermes_native_memory_provider: &'static str,
    hermes_shared_home_connections: Option<u16>,
    memory_context_chars: usize,
    estimated_tokens: usize,
    memory_generations: MemoryGenerationsReceipt,
    memory_lifecycle: Vec<CapabilityLifecycleReceipt>,
    reason_codes: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContextPlanReceipt {
    pub plan_id: String,
    pub connection_hash: String,
    pub conversation_hash: String,
    pub workspace_hash: String,
    pub turn_generation: i64,
    pub agent_type: AgentType,
    pub managed_agent_version: Option<String>,
    pub hermes_native_memory_provider: &'static str,
    pub hermes_shared_home_connections: Option<u16>,
    pub adapter_mode: &'static str,
    pub memory_context_chars: usize,
    pub estimated_tokens: usize,
    pub duration_ms: u64,
    pub stop_reason: String,
    pub outcome: &'static str,
    memory_generations: MemoryGenerationsReceipt,
    memory_lifecycle: Vec<CapabilityLifecycleReceipt>,
    pub reason_codes: Vec<String>,
}

impl ContextPlanReceiptSeed {
    pub fn new(input: ContextPlanStart<'_>) -> Self {
        let memory_context_chars = input
            .memory
            .rendered
            .as_deref()
            .map(str::chars)
            .map(Iterator::count)
            .unwrap_or_default();
        Self {
            started_at: Instant::now(),
            plan_id: plan_id(input.connection_id, input.turn_nonce),
            connection_hash: connection_hash(input.connection_id),
            conversation_hash: conversation_hash(input.conversation_id),
            workspace_hash: workspace_hash(input.workspace),
            turn_generation: input.turn_generation,
            agent_type: input.agent_type,
            managed_agent_version: input.managed_agent_version.map(bounded_label),
            hermes_native_memory_provider: input.hermes_memory.state.as_str(),
            hermes_shared_home_connections: input.hermes_shared_home_connections,
            memory_context_chars,
            estimated_tokens: estimated_tokens(memory_context_chars),
            memory_generations: memory_generations(input.memory),
            memory_lifecycle: memory_lifecycle(input.memory, input.context_loaded),
            reason_codes: receipt_reason_codes(
                input.memory,
                input.hermes_memory,
                input.hermes_shared_home_connections,
            ),
        }
    }

    pub fn finish(mut self, finish: ContextPlanFinish<'_>) -> ContextPlanReceipt {
        apply_called_observations(&mut self.memory_lifecycle, finish.memory_calls);
        let stop_reason = bounded_reason_code(finish.stop_reason);
        let outcome = if finish.stop_reason == "end_turn" {
            "completed"
        } else {
            "degraded"
        };
        if outcome == "degraded" {
            self.reason_codes.push(format!("turn_stop:{stop_reason}"));
        }
        ContextPlanReceipt {
            plan_id: self.plan_id,
            connection_hash: self.connection_hash,
            conversation_hash: self.conversation_hash,
            workspace_hash: self.workspace_hash,
            turn_generation: self.turn_generation,
            agent_type: self.agent_type,
            managed_agent_version: self.managed_agent_version,
            hermes_native_memory_provider: self.hermes_native_memory_provider,
            hermes_shared_home_connections: self.hermes_shared_home_connections,
            adapter_mode: "acp_wire",
            memory_context_chars: self.memory_context_chars,
            estimated_tokens: self.estimated_tokens,
            duration_ms: elapsed_millis(self.started_at),
            stop_reason,
            outcome,
            memory_generations: self.memory_generations,
            memory_lifecycle: self.memory_lifecycle,
            reason_codes: self.reason_codes,
        }
    }
}

impl ContextPlanReceipt {
    pub fn memory_generations_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&self.memory_generations)
    }

    pub fn memory_lifecycle_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&self.memory_lifecycle)
    }

    pub fn reason_codes_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&self.reason_codes)
    }

    pub fn encoded_len(&self) -> Result<usize, serde_json::Error> {
        serde_json::to_vec(self).map(|encoded| encoded.len())
    }
}

fn memory_generations(memory: &UserMemoryContextSnapshot) -> MemoryGenerationsReceipt {
    MemoryGenerationsReceipt {
        historical_context_generation: memory
            .historical_context_generation
            .clone()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "unknown".to_string()),
        recall_source_generation: "not_enabled",
        index_generation: memory
            .recall_index_generation
            .map(|generation| generation.to_string())
            .unwrap_or_else(|| "not_enabled".to_string()),
        ranking_version: "not_enabled",
    }
}

fn receipt_reason_codes(
    memory: &UserMemoryContextSnapshot,
    hermes: HermesNativeMemoryDiagnostics,
    shared_home_connections: Option<u16>,
) -> Vec<String> {
    let mut reasons = memory_reason_codes(memory);
    if matches!(hermes.state, HermesNativeMemoryState::Yes) {
        reasons.push("native_memory_detected".to_string());
    }
    if let Some(reason) = hermes.reason_code {
        reasons.push(reason.to_string());
    }
    if shared_home_connections.is_some_and(|count| count >= 2) {
        reasons.push("hermes_shared_home_memory_risk".to_string());
    }
    reasons.sort();
    reasons.dedup();
    reasons
}

fn estimated_tokens(chars: usize) -> usize {
    chars.saturating_add(ESTIMATED_CHARS_PER_TOKEN - 1) / ESTIMATED_CHARS_PER_TOKEN
}

fn elapsed_millis(started_at: Instant) -> u64 {
    started_at.elapsed().as_millis().min(u64::MAX as u128) as u64
}
