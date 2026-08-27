use std::collections::BTreeSet;

use crate::user_memory::{UserMemoryCapabilityReason, UserMemoryContextSnapshot};

pub(super) fn memory_reason_codes(memory: &UserMemoryContextSnapshot) -> Vec<String> {
    let reasons = [
        memory.capabilities.read_context.reason,
        memory.capabilities.read_documents.reason,
        memory.capabilities.confirmed_append.reason,
        memory.capabilities.candidate_proposal.reason,
    ];
    let mut reasons = reasons
        .into_iter()
        .map(capability_reason_code)
        .collect::<BTreeSet<_>>();
    if !memory.recall_tool_enabled {
        reasons.insert("recall_tool_disabled".to_string());
    }
    reasons.into_iter().collect()
}

fn capability_reason_code(reason: UserMemoryCapabilityReason) -> String {
    serde_json::to_value(reason)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}
