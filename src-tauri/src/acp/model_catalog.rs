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
    load_persisted().unwrap_or_else(|| {
        tracing::info!(
            "[ModelCatalog] no persisted catalog, using built-in seed ({} models): {:?}",
            MANAGED_MODEL_IDS.len(),
            MANAGED_MODEL_IDS
        );
        MANAGED_MODEL_IDS.to_vec()
    })
}

fn persist_path() -> Option<PathBuf> {
    let data_dir = std::env::var_os("IYW_CLAW_DATA_DIR")?;
    if data_dir.is_empty() {
        return None;
    }
    Some(PathBuf::from(data_dir).join(PERSIST_FILE_NAME))
}

fn load_persisted() -> Option<Vec<&'static str>> {
    let path = persist_path()?;
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) => {
            tracing::debug!(
                "[ModelCatalog] no persisted catalog at {}: {}",
                path.display(),
                error
            );
            return None;
        }
    };
    let ids: Vec<String> = match serde_json::from_str(&raw) {
        Ok(ids) => ids,
        Err(error) => {
            tracing::warn!(
                "[ModelCatalog] failed to parse persisted catalog: {}",
                error
            );
            return None;
        }
    };
    let ids: Vec<&'static str> = ids
        .iter()
        .map(String::as_str)
        .filter(|id| !id.trim().is_empty())
        .map(intern)
        .collect();
    if ids.is_empty() {
        tracing::warn!("[ModelCatalog] persisted catalog is empty, will use seed");
        return None;
    }
    tracing::info!(
        "[ModelCatalog] loaded {} models from disk: {:?}",
        ids.len(),
        ids
    );
    Some(ids)
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
