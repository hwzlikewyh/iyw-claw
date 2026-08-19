use std::sync::OnceLock;

use super::{ContextPlanFinish, ContextPlanReceiptSeed, ContextPlanStart};

const TELEMETRY_ENV: &str = "IYW_CLAW_CONTEXT_GOVERNOR_TELEMETRY";

pub(crate) fn start_context_plan(input: ContextPlanStart<'_>) -> Option<ContextPlanReceiptSeed> {
    telemetry_enabled().then(|| ContextPlanReceiptSeed::new(input))
}

pub(crate) fn finish_context_plan(seed: ContextPlanReceiptSeed, finish: ContextPlanFinish<'_>) {
    let receipt = seed.finish(finish);
    let encoded = encode_receipt_fields(&receipt);
    let Ok((memory_generations, memory_lifecycle, reason_codes, receipt_bytes)) = encoded else {
        tracing::warn!(
            target: "context_governor",
            plan_id = %receipt.plan_id,
            "context plan receipt serialization failed"
        );
        return;
    };
    tracing::info!(
        target: "context_governor",
        plan_id = %receipt.plan_id,
        connection_hash = %receipt.connection_hash,
        conversation_hash = %receipt.conversation_hash,
        workspace_hash = %receipt.workspace_hash,
        turn_generation = receipt.turn_generation,
        agent_type = ?receipt.agent_type,
        managed_agent_version = ?receipt.managed_agent_version,
        hermes_native_memory_provider = receipt.hermes_native_memory_provider,
        hermes_shared_home_connections = ?receipt.hermes_shared_home_connections,
        adapter_mode = receipt.adapter_mode,
        memory_context_chars = receipt.memory_context_chars,
        estimated_tokens = receipt.estimated_tokens,
        duration_ms = receipt.duration_ms,
        stop_reason = %receipt.stop_reason,
        outcome = receipt.outcome,
        memory_generations_json = %memory_generations,
        memory_lifecycle_json = %memory_lifecycle,
        reason_codes_json = %reason_codes,
        receipt_bytes,
        "context plan shadow receipt completed"
    );
}

fn encode_receipt_fields(
    receipt: &super::types::ContextPlanReceipt,
) -> Result<(String, String, String, usize), serde_json::Error> {
    Ok((
        receipt.memory_generations_json()?,
        receipt.memory_lifecycle_json()?,
        receipt.reason_codes_json()?,
        receipt.encoded_len()?,
    ))
}

fn telemetry_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var(TELEMETRY_ENV)
            .ok()
            .as_deref()
            .map(parse_enabled)
            .unwrap_or(true)
    })
}

fn parse_enabled(value: &str) -> bool {
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "0" | "false" | "no" | "off"
    )
}
