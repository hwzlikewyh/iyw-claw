use serde_json::Value;
use sha2::{Digest, Sha256};

use super::capability_registry::CAPABILITY_BINDINGS;

pub(super) fn search_score(
    id: &str,
    category: &str,
    aliases: &[String],
    description: &str,
    query: &str,
) -> Option<usize> {
    let haystack =
        format!("{id} {category} {} {description}", aliases.join(" ")).to_ascii_lowercase();
    let score = query
        .split_whitespace()
        .filter(|token| haystack.contains(token))
        .count();
    if score == 0 && !haystack.contains(query) {
        return None;
    }
    Some(score + usize::from(id.eq_ignore_ascii_case(query)) * 100)
}

pub(super) fn first_sentence(description: &str) -> String {
    description
        .split_once(". ")
        .map_or(description, |(first, _)| first)
        .trim()
        .to_string()
}

pub(super) fn capability_category(id: &str) -> String {
    id.split('.').nth(1).unwrap_or("other").to_string()
}

pub(super) fn capability_aliases(id: &str) -> Vec<String> {
    let parts = id.split('.').collect::<Vec<_>>();
    let end = parts.len().saturating_sub(1);
    let route = parts.get(1..end).unwrap_or_default();
    let mut aliases = route
        .iter()
        .map(|part| (*part).to_string())
        .collect::<Vec<_>>();
    if !route.is_empty() {
        aliases.push(route.join(" "));
    }
    aliases.sort();
    aliases.dedup();
    aliases
}

pub(super) fn required_inputs(schema: &Value) -> Vec<String> {
    schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect()
}

pub(super) fn digest(value: &impl serde::Serialize) -> Result<String, serde_json::Error> {
    let encoded = serde_json::to_vec(value)?;
    Ok(format!("sha256:{:x}", Sha256::digest(encoded)))
}

pub(super) fn public_text(text: &str) -> String {
    CAPABILITY_BINDINGS
        .iter()
        .fold(text.to_string(), |value, (tool_name, capability_id)| {
            value.replace(tool_name, capability_id)
        })
}

pub(super) fn public_value(value: Value) -> Value {
    match value {
        Value::String(text) => Value::String(public_text(&text)),
        Value::Array(values) => Value::Array(values.into_iter().map(public_value).collect()),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, public_value(value)))
                .collect(),
        ),
        other => other,
    }
}
