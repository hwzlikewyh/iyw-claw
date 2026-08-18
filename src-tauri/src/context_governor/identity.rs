use std::path::Path;

use sha2::{Digest, Sha256};

const PLAN_ID_DOMAIN: &[u8] = b"iyw-claw:context-plan:v1\0";
const CONNECTION_ID_DOMAIN: &[u8] = b"iyw-claw:context-connection:v1\0";
const CONVERSATION_ID_DOMAIN: &[u8] = b"iyw-claw:context-conversation:v1\0";
const WORKSPACE_ID_DOMAIN: &[u8] = b"iyw-claw:context-workspace:v1\0";
const HERMES_HOME_DOMAIN: &[u8] = b"iyw-claw:context-hermes-home:v1\0";
const MAX_LABEL_CHARS: usize = 64;

pub(super) fn plan_id(connection_id: &str, turn_nonce: u64) -> String {
    stable_hash_parts(
        PLAN_ID_DOMAIN,
        &[connection_id.as_bytes(), &turn_nonce.to_le_bytes()],
    )
}

pub(super) fn connection_hash(connection_id: &str) -> String {
    stable_hash(CONNECTION_ID_DOMAIN, connection_id.as_bytes())
}

pub(super) fn conversation_hash(conversation_id: Option<i32>) -> String {
    let value = conversation_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    stable_hash(CONVERSATION_ID_DOMAIN, value.as_bytes())
}

pub(super) fn workspace_hash(workspace: Option<&Path>) -> String {
    let value = workspace
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unknown".to_string());
    stable_hash(WORKSPACE_ID_DOMAIN, value.as_bytes())
}

pub(super) fn hermes_home_hash(home: &Path) -> String {
    stable_hash(HERMES_HOME_DOMAIN, home.to_string_lossy().as_bytes())
}

pub(super) fn bounded_label(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_LABEL_CHARS)
        .collect()
}

pub(super) fn bounded_reason_code(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    let valid = !normalized.is_empty()
        && normalized.chars().count() <= MAX_LABEL_CHARS
        && normalized
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'));
    valid
        .then_some(normalized)
        .unwrap_or_else(|| "other".to_string())
}

fn stable_hash(domain: &[u8], value: &[u8]) -> String {
    stable_hash_parts(domain, &[value])
}

fn stable_hash_parts(domain: &[u8], values: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for value in values {
        hasher.update((value.len() as u64).to_le_bytes());
        hasher.update(value);
    }
    format!("{:x}", hasher.finalize())
}
