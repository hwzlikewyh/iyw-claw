use std::path::PathBuf;

use serde::Deserialize;

use crate::user_memory::{CompanionHealthReason, CompanionHealthSnapshot, CompanionHealthStatus};

const MAX_DETAIL_CHARS: usize = 1_024;
const MAX_HEALTH_FIELD_CHARS: usize = 128;
const MAX_ADVERTISED_TOOLS: usize = 64;

#[derive(Debug, Deserialize)]
struct CompanionManifest {
    name: String,
    version: String,
    protocol_version: u32,
    tools: Vec<String>,
}

pub fn parse_companion_manifest(path: PathBuf, raw: &str) -> CompanionHealthSnapshot {
    let mut health = snapshot(path);
    let mut manifest = match serde_json::from_str::<CompanionManifest>(raw.trim()) {
        Ok(manifest) => manifest,
        Err(error) => return malformed(health, error),
    };
    health.detected_version = Some(bounded_field(&manifest.version));
    health.advertised_tools = bounded_tools(std::mem::take(&mut manifest.tools));
    apply_compatibility(&mut health, manifest);
    health
}

pub(crate) fn bounded_detail(detail: impl AsRef<str>) -> String {
    detail.as_ref().chars().take(MAX_DETAIL_CHARS).collect()
}

fn snapshot(selected_path: PathBuf) -> CompanionHealthSnapshot {
    CompanionHealthSnapshot {
        selected_path: Some(selected_path),
        ..CompanionHealthSnapshot::default()
    }
}

fn malformed(
    mut health: CompanionHealthSnapshot,
    error: serde_json::Error,
) -> CompanionHealthSnapshot {
    health.status = CompanionHealthStatus::ProbeFailed;
    health.reason = CompanionHealthReason::ManifestMalformed;
    health.detail = Some(bounded_detail(error.to_string()));
    health
}

fn apply_compatibility(health: &mut CompanionHealthSnapshot, manifest: CompanionManifest) {
    if manifest.name != "iyw-claw-mcp" {
        incompatible(
            health,
            CompanionHealthReason::NameMismatch,
            format!("unexpected companion name: {}", manifest.name),
        );
    } else if manifest.version != env!("CARGO_PKG_VERSION") {
        incompatible(
            health,
            CompanionHealthReason::VersionMismatch,
            format!("detected companion version {}", manifest.version),
        );
    } else if manifest.protocol_version != 1 {
        incompatible(
            health,
            CompanionHealthReason::ProtocolMismatch,
            format!("detected protocol {}", manifest.protocol_version),
        );
    } else {
        health.status = CompanionHealthStatus::Ready;
        health.reason = CompanionHealthReason::Ready;
    }
}

fn incompatible(
    health: &mut CompanionHealthSnapshot,
    reason: CompanionHealthReason,
    detail: String,
) {
    health.status = CompanionHealthStatus::Incompatible;
    health.reason = reason;
    health.detail = Some(bounded_detail(detail));
}

fn bounded_field(field: &str) -> String {
    field.chars().take(MAX_HEALTH_FIELD_CHARS).collect()
}

fn bounded_tools(tools: Vec<String>) -> Vec<String> {
    let mut tools = tools
        .into_iter()
        .map(|tool| bounded_field(&tool))
        .collect::<Vec<_>>();
    tools.sort();
    tools.dedup();
    tools.truncate(MAX_ADVERTISED_TOOLS);
    tools
}
