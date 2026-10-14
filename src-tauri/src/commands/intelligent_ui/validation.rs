use crate::app_error::AppCommandError;
use crate::models::message::{ContentBlock, MessageTurn, TurnRole};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const MAX_STATE_BYTES: usize = 4096;
const MAX_SOURCE_BYTES: usize = 200_000;
const MAX_STATE_DEPTH: usize = 4;
const MAX_NAME_BYTES: usize = 64;

pub fn invalid(reason: &str) -> AppCommandError {
    AppCommandError::invalid_input(reason)
}

pub fn source_hash(source: &str) -> String {
    format!("{:x}", Sha256::digest(source.trim().as_bytes()))
}

pub fn normalize_state(state: &Value) -> Result<String, AppCommandError> {
    let object = state
        .as_object()
        .ok_or_else(|| invalid("UI state must be an object"))?;
    if object.keys().any(|key| !valid_name(key)) || !valid_value(state, 0) {
        return Err(invalid("Invalid UI state name or depth"));
    }
    let json = serde_json::to_string(state).map_err(|error| invalid(&error.to_string()))?;
    if json.len() > MAX_STATE_BYTES {
        return Err(invalid("UI state exceeds 4KiB"));
    }
    Ok(json)
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME_BYTES
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        && !matches!(name, "__proto__" | "prototype" | "constructor")
}

fn valid_value(value: &Value, depth: usize) -> bool {
    if depth > MAX_STATE_DEPTH {
        return false;
    }
    match value {
        Value::Object(values) => values
            .iter()
            .all(|(key, value)| valid_name(key) && valid_value(value, depth + 1)),
        Value::Array(values) => values.iter().all(|value| valid_value(value, depth + 1)),
        _ => true,
    }
}

pub fn resolve_turn<'a>(
    turns: &'a [MessageTurn],
    block_id: &str,
    source: &str,
) -> Result<&'a MessageTurn, AppCommandError> {
    if !valid_name(block_id) || source.len() > MAX_SOURCE_BYTES {
        return Err(invalid("Invalid UI block"));
    }
    let mut found = None;
    for turn in turns
        .iter()
        .filter(|turn| matches!(turn.role, TurnRole::Assistant))
    {
        let count: usize = turn
            .blocks
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(
                    super::fences::scan(text)
                        .iter()
                        .filter(|fence| fence.key == block_id && fence.body == source.trim())
                        .count(),
                ),
                _ => None,
            })
            .sum();
        if count > 1 || (count == 1 && found.is_some()) {
            return Err(invalid("Repeated UI source cannot be persisted"));
        }
        if count == 1 {
            found = Some(turn);
        }
    }
    found.ok_or_else(|| invalid("UI block is not in a completed assistant reply"))
}
