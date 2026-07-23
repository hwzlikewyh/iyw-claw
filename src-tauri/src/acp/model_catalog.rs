//! Runtime model catalog: the online gateway list drives what agents run.
//!
//! Historically every agent's model surface (spawn env like `ANTHROPIC_MODEL`
//! / `CODEBUDDY_MODEL`, and native config rewrites for codex/grok/kimi/…) was
//! pinned to the hardcoded `MANAGED_MODEL_IDS`, so a gateway-side model launch
//! required an app release. This module makes the hardcoded list only a
//! *seed*: whenever the app fetches `/v1/models` (login, the UI's periodic
//! 30-minute refresh), the parsed ids replace the in-memory catalog and are
//! persisted under the data dir, so the next launch starts from the last
//! known online catalog even before sign-in.
//!
//! The gateway response is authoritative for every agent. The fusion gateway
//! owns protocol conversion, while `/v1/models` currently exposes no
//! per-agent capability field; filtering by model-name prefixes here would
//! silently discard models that the gateway explicitly made available.
//!
//! Interning: ids from the online catalog are leaked into `&'static str`
//! (deduplicated), so the long-standing `&'static` signatures of
//! `managed_model_ids_for` / `managed_default_model_for` keep working across
//! every config writer. Growth is bounded by the set of distinct ids ever
//! seen in one process lifetime.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock, RwLock};

use crate::acp::provider_overlay_formats::MANAGED_MODEL_IDS;
use crate::models::agent::AgentType;

const PERSIST_FILE_NAME: &str = "model-catalog.json";

fn interner() -> &'static Mutex<HashSet<&'static str>> {
    static INTERNER: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    INTERNER.get_or_init(|| Mutex::new(HashSet::new()))
}

fn intern(value: &str) -> &'static str {
    let mut set = interner().lock().expect("interner poisoned");
    if let Some(existing) = set.get(value) {
        return existing;
    }
    let leaked: &'static str = Box::leak(value.to_string().into_boxed_str());
    set.insert(leaked);
    leaked
}

fn catalog() -> &'static RwLock<Vec<&'static str>> {
    static CATALOG: OnceLock<RwLock<Vec<&'static str>>> = OnceLock::new();
    CATALOG.get_or_init(|| RwLock::new(initial_catalog()))
}

/// Seed order matters: it is the catalog order until the first online fetch.
fn initial_catalog() -> Vec<&'static str> {
    load_persisted().unwrap_or_else(|| MANAGED_MODEL_IDS.to_vec())
}

fn persist_path() -> Option<PathBuf> {
    let data_dir = std::env::var_os("IYW_CLAW_DATA_DIR")?;
    if data_dir.is_empty() {
        return None;
    }
    Some(PathBuf::from(data_dir).join(PERSIST_FILE_NAME))
}

fn load_persisted() -> Option<Vec<&'static str>> {
    let raw = std::fs::read_to_string(persist_path()?).ok()?;
    let ids: Vec<String> = serde_json::from_str(&raw).ok()?;
    let ids: Vec<&'static str> = ids
        .iter()
        .map(String::as_str)
        .filter(|id| !id.trim().is_empty())
        .map(intern)
        .collect();
    (!ids.is_empty()).then_some(ids)
}

fn persist(ids: &[&'static str]) {
    let Some(path) = persist_path() else {
        return;
    };
    if let Ok(json) = serde_json::to_string(ids) {
        if let Err(error) = std::fs::write(&path, json) {
            tracing::warn!("[ModelCatalog] failed to persist catalog: {error}");
        }
    }
}

/// Feed a raw `/v1/models` response into the catalog. Accepts the standard
/// `{"data": [{"id": ...}, ...]}` shape; anything unparsable or empty leaves
/// the current catalog untouched (an outage must never shrink model lists).
/// Returns true when the catalog was updated.
pub fn update_from_payload(payload: &serde_json::Value) -> bool {
    let Some(entries) = payload.get("data").and_then(|value| value.as_array()) else {
        return false;
    };
    let mut ids: Vec<&'static str> = Vec::new();
    let mut seen = HashSet::new();
    for entry in entries {
        let Some(id) = entry.get("id").and_then(|value| value.as_str()) else {
            continue;
        };
        let id = id.trim();
        if id.is_empty() || !seen.insert(id.to_string()) {
            continue;
        }
        ids.push(intern(id));
    }
    if ids.is_empty() {
        return false;
    }
    let changed = {
        let mut cached = catalog().write().expect("catalog poisoned");
        let changed = *cached != ids;
        *cached = ids.clone();
        changed
    };
    if changed {
        tracing::info!("[ModelCatalog] catalog updated ({} models)", ids.len());
        persist(&ids);
    }
    changed
}

pub fn all_model_ids() -> Vec<&'static str> {
    catalog().read().expect("catalog poisoned").clone()
}

// Historical family derivation is retained only for regression coverage of
// the bundled seed. Runtime selection must use the gateway catalog verbatim.

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModelFamily {
    Anthropic,
    OpenAi,
    Google,
    DeepSeek,
    Qwen,
    Doubao,
    Other,
}

#[cfg(test)]
fn family_of(model_id: &str) -> ModelFamily {
    let id = model_id.to_ascii_lowercase();
    if id.starts_with("claude") {
        ModelFamily::Anthropic
    } else if id.starts_with("gpt") || id.starts_with("o1") || id.starts_with("o3") {
        ModelFamily::OpenAi
    } else if id.starts_with("gemini") {
        ModelFamily::Google
    } else if id.starts_with("deepseek") {
        ModelFamily::DeepSeek
    } else if id.starts_with("qwen") {
        ModelFamily::Qwen
    } else if id.starts_with("doubao") {
        ModelFamily::Doubao
    } else {
        ModelFamily::Other
    }
}

#[cfg(test)]
fn family_allowed(agent: AgentType, family: ModelFamily) -> bool {
    use ModelFamily::*;
    match agent {
        AgentType::Grok => true,
        AgentType::Codex => matches!(family, OpenAi | DeepSeek | Other),
        AgentType::ClaudeCode => matches!(family, Anthropic | OpenAi),
        AgentType::Gemini => matches!(family, Google | OpenAi),
        _ => matches!(family, DeepSeek | Qwen | Other),
    }
}

#[cfg(test)]
fn primary_family(agent: AgentType) -> ModelFamily {
    match agent {
        AgentType::ClaudeCode => ModelFamily::Anthropic,
        AgentType::Gemini => ModelFamily::Google,
        AgentType::Codex | AgentType::Grok => ModelFamily::OpenAi,
        _ => ModelFamily::DeepSeek,
    }
}

#[cfg(test)]
fn scoped_ids(agent: AgentType, source: &[&'static str]) -> Vec<&'static str> {
    let primary = primary_family(agent);
    let mut head = Vec::new();
    let mut tail = Vec::new();
    for id in source {
        let family = family_of(id);
        if !family_allowed(agent, family) {
            continue;
        }
        if family == primary {
            head.push(*id);
        } else {
            tail.push(*id);
        }
    }
    head.extend(tail);
    head
}

/// Models exposed by the gateway, preserving its order for every agent.
/// Never empty: the bundled seed remains the offline/startup fallback so
/// config writers can index `[0]` safely.
pub fn model_ids_for(_agent: AgentType) -> Vec<&'static str> {
    let ids = all_model_ids();
    if !ids.is_empty() {
        return ids;
    }
    MANAGED_MODEL_IDS.to_vec()
}

pub fn default_model_for(agent: AgentType) -> &'static str {
    model_ids_for(agent)[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The derivation must reproduce the historical hardcoded per-agent lists
    /// byte-for-byte when the catalog equals the seed — upgrades keep every
    /// agent on exactly the models it had before.
    #[test]
    fn seed_catalog_reproduces_historical_agent_lists() {
        let seed = MANAGED_MODEL_IDS.to_vec();
        assert_eq!(
            scoped_ids(AgentType::Codex, &seed),
            vec!["gpt-5.4", "deepseek-v4-pro", "deepseek-v4-flash"]
        );
        assert_eq!(
            scoped_ids(AgentType::ClaudeCode, &seed),
            vec!["claude-opus-4-6", "gpt-5.4"]
        );
        assert_eq!(
            scoped_ids(AgentType::Gemini, &seed),
            vec!["gemini-3.1-pro-preview", "gpt-5.4"]
        );
        assert_eq!(scoped_ids(AgentType::Grok, &seed).len(), seed.len());
        assert_eq!(
            scoped_ids(AgentType::OpenCode, &seed),
            vec!["deepseek-v4-pro", "deepseek-v4-flash", "qwen3.7-max"]
        );
    }

    #[test]
    fn payload_update_reshapes_agent_lists_and_ignores_garbage() {
        // Unknown/new models join by family; garbage payloads are inert.
        let payload = serde_json::json!({
            "data": [
                {"id": "claude-opus-4-7"},
                {"id": "gpt-6"},
                {"id": "deepseek-v5"},
                {"id": "  "},
                {"id": "claude-opus-4-7"},
            ]
        });
        let mut ids = Vec::new();
        for entry in payload["data"].as_array().unwrap() {
            if let Some(id) = entry["id"].as_str() {
                let id = id.trim();
                if !id.is_empty() && !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        let interned: Vec<&'static str> = ids.into_iter().map(intern).collect();
        assert_eq!(
            scoped_ids(AgentType::ClaudeCode, &interned),
            vec!["claude-opus-4-7", "gpt-6"]
        );
        assert_eq!(
            scoped_ids(AgentType::OpenCode, &interned),
            vec!["deepseek-v5"]
        );
        assert!(!update_from_payload(&serde_json::json!({"data": []})));
        assert!(!update_from_payload(&serde_json::json!({"error": "x"})));
    }

    #[test]
    fn scoping_never_returns_empty_for_any_agent() {
        for agent in crate::acp::registry::all_acp_agents() {
            assert!(
                !model_ids_for(agent).is_empty(),
                "{agent} must always have at least one model"
            );
        }
    }
}
