use serde_json::Value;
use sha2::{Digest, Sha256};

use super::capability_intents::{intent_metadata, CapabilityIntentMetadata};
use super::capability_registry::CAPABILITY_BINDINGS;

pub(super) use super::capability_intents::validate_intent_metadata;

pub(super) fn search_score(
    id: &str,
    category: &str,
    aliases: &[String],
    intent_terms: &[String],
    negative_terms: &[String],
    description: &str,
    query: &str,
) -> Option<usize> {
    let query = normalize(query);
    let haystack = normalize(&format!(
        "{id} {category} {} {} {description}",
        aliases.join(" "),
        intent_terms.join(" ")
    ));
    let token_score = query
        .split_whitespace()
        .filter(|token| haystack.contains(*token))
        .count();
    let alias_exact = aliases.iter().any(|alias| normalize(alias) == query);
    let phrase = !query.is_empty() && haystack.contains(&query);
    let negative_hits = negative_terms
        .iter()
        .filter(|term| haystack.contains(&normalize(term)))
        .count();
    if token_score == 0 && !phrase && !alias_exact {
        return None;
    }
    let intent_score = intent_terms
        .iter()
        .filter(|term| query.contains(&normalize(term)))
        .count();
    Some(
        usize::from(normalize(id) == query) * 1000
            + usize::from(alias_exact) * 100
            + usize::from(phrase) * 25
            + token_score.saturating_mul(10)
            + intent_score
                .saturating_mul(5)
                .saturating_sub(negative_hits.saturating_mul(3)),
    )
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| character.to_lowercase())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
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

pub(super) fn capability_aliases(id: &str, tool_name: &str) -> Vec<String> {
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
    if let Some(metadata) = intent_metadata(tool_name) {
        aliases.extend(metadata.aliases.iter().map(|alias| (*alias).to_string()));
    }
    aliases.sort();
    aliases.dedup();
    aliases
}

pub(super) fn intent_terms(metadata: CapabilityIntentMetadata) -> Vec<String> {
    metadata
        .intent_terms
        .iter()
        .map(|term| (*term).to_string())
        .collect()
}

pub(super) fn negative_terms(metadata: CapabilityIntentMetadata) -> Vec<String> {
    metadata
        .negative_terms
        .iter()
        .map(|term| (*term).to_string())
        .collect()
}

pub(super) fn when_to_use(metadata: CapabilityIntentMetadata) -> String {
    metadata.when_to_use.to_string()
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
