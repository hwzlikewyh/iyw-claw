//! Runtime model catalog with persisted effective image capabilities.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock, RwLock};

use crate::acp::provider_overlay_formats::MANAGED_MODEL_IDS;
use crate::models::agent::AgentType;

pub use super::model_catalog_types::{
    ImageInputMode, ModelCapabilities, ModelCapabilitySnapshot, ModelLimits,
};
use super::model_catalog_types::{
    PersistedCatalog, PersistedCatalogV2, PersistedModel, RuntimeCatalog,
};

const PERSIST_FILE_NAME: &str = "model-catalog.json";
const PERSIST_VERSION: u32 = 3;
const PREVIOUS_PERSIST_VERSION: u32 = 2;

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

fn catalog() -> &'static RwLock<RuntimeCatalog> {
    static CATALOG: OnceLock<RwLock<RuntimeCatalog>> = OnceLock::new();
    CATALOG.get_or_init(|| RwLock::new(initial_catalog()))
}

fn initial_catalog() -> RuntimeCatalog {
    load_persisted().unwrap_or_else(|| {
        tracing::info!(
            "[ModelCatalog] no persisted catalog, using built-in seed ({} models)",
            MANAGED_MODEL_IDS.len()
        );
        RuntimeCatalog {
            ids: MANAGED_MODEL_IDS.to_vec(),
            capabilities: HashMap::new(),
        }
    })
}

fn persist_path() -> Option<PathBuf> {
    let data_dir = std::env::var_os("IYW_CLAW_DATA_DIR")?;
    if data_dir.is_empty() {
        return None;
    }
    Some(PathBuf::from(data_dir).join(PERSIST_FILE_NAME))
}

fn load_persisted() -> Option<RuntimeCatalog> {
    let path = persist_path()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    let persisted: PersistedCatalog = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!("[ModelCatalog] failed to parse persisted catalog: {error}");
            return None;
        }
    };
    let models = match persisted {
        PersistedCatalog::Legacy(ids) => ids
            .into_iter()
            .map(|id| PersistedModel {
                id,
                capabilities: ModelCapabilities::default(),
                image_input_mode: ImageInputMode::None,
                limits: ModelLimits::default(),
            })
            .collect(),
        PersistedCatalog::Current(value)
            if matches!(value.version, PREVIOUS_PERSIST_VERSION | PERSIST_VERSION) =>
        {
            value.models
        }
        PersistedCatalog::Current(value) => {
            tracing::warn!(
                "[ModelCatalog] unsupported persisted version {}",
                value.version
            );
            return None;
        }
    };
    runtime_catalog(models)
}

fn runtime_catalog(models: Vec<PersistedModel>) -> Option<RuntimeCatalog> {
    let mut ids = Vec::new();
    let mut capabilities = HashMap::new();
    let mut seen = HashSet::new();
    for model in models {
        let id = model.id.trim();
        if id.is_empty() || !seen.insert(id.to_string()) {
            continue;
        }
        let id = intern(id);
        ids.push(id);
        capabilities.insert(
            id,
            ModelCapabilitySnapshot {
                capabilities: model.capabilities,
                image_input_mode: model.image_input_mode,
                limits: model.limits,
            },
        );
    }
    (!ids.is_empty()).then_some(RuntimeCatalog { ids, capabilities })
}

fn persist(value: &RuntimeCatalog) {
    let Some(path) = persist_path() else {
        return;
    };
    let models = value
        .ids
        .iter()
        .map(|id| {
            let snapshot = value.capabilities.get(id).copied().unwrap_or_default();
            PersistedModel {
                id: (*id).to_string(),
                capabilities: snapshot.capabilities,
                image_input_mode: snapshot.image_input_mode,
                limits: snapshot.limits,
            }
        })
        .collect();
    let payload = PersistedCatalogV2 {
        version: PERSIST_VERSION,
        models,
    };
    if let Ok(json) = serde_json::to_string(&payload) {
        if let Err(error) = std::fs::write(&path, json) {
            tracing::warn!("[ModelCatalog] failed to persist catalog: {error}");
        }
    }
}

pub fn update_from_payload(payload: &serde_json::Value) -> bool {
    let Some(next) = catalog_from_payload(payload) else {
        return false;
    };
    let changed = {
        let mut current = catalog().write().expect("catalog poisoned");
        let changed = *current != next;
        *current = next.clone();
        changed
    };
    if changed {
        tracing::info!("[ModelCatalog] catalog updated ({} models)", next.ids.len());
        persist(&next);
    }
    changed
}

/// Merge an Agent-scoped catalog response into the complete catalog.
///
/// A filtered response is authoritative only for the Agent that requested it;
/// replacing the global catalog here would silently remove models used by
/// other Agents. New IDs are added and existing metadata is refreshed.
pub fn merge_from_payload(payload: &serde_json::Value) -> bool {
    let Some(incoming) = catalog_from_payload(payload) else {
        return false;
    };
    let changed = {
        let mut current = catalog().write().expect("catalog poisoned");
        let mut next = current.clone();
        for id in incoming.ids {
            if !next.ids.contains(&id) {
                next.ids.push(id);
            }
            if let Some(snapshot) = incoming.capabilities.get(&id) {
                next.capabilities.insert(id, *snapshot);
            }
        }
        let changed = *current != next;
        *current = next.clone();
        changed
    };
    if changed {
        let snapshot = catalog().read().expect("catalog poisoned").clone();
        tracing::info!(
            "[ModelCatalog] merged Agent-scoped catalog ({} models)",
            snapshot.ids.len()
        );
        persist(&snapshot);
    }
    changed
}

fn catalog_from_payload(payload: &serde_json::Value) -> Option<RuntimeCatalog> {
    let entries = payload.get("data").and_then(serde_json::Value::as_array)?;
    let models = entries.iter().filter_map(parse_payload_model).collect();
    runtime_catalog(models)
}

fn parse_payload_model(value: &serde_json::Value) -> Option<PersistedModel> {
    let id = value.get("id")?.as_str()?.trim();
    if id.is_empty() {
        return None;
    }
    let capabilities = value
        .get("capabilities")
        .and_then(serde_json::Value::as_object)
        .map(parse_capabilities)
        .unwrap_or_default();
    let image_input_mode = value
        .pointer("/image_input/mode")
        .and_then(serde_json::Value::as_str)
        .and_then(parse_image_input_mode)
        .unwrap_or(if capabilities.vision {
            ImageInputMode::Native
        } else {
            ImageInputMode::None
        });
    let limits = value
        .get("limits")
        .and_then(serde_json::Value::as_object)
        .map(parse_limits)
        .unwrap_or_default();
    Some(PersistedModel {
        id: id.to_string(),
        capabilities,
        image_input_mode,
        limits,
    })
}

fn parse_limits(value: &serde_json::Map<String, serde_json::Value>) -> ModelLimits {
    let limit = |key: &str| value.get(key).and_then(serde_json::Value::as_u64);
    ModelLimits {
        context_window: limit("context_window"),
        max_input_tokens: limit("max_input_tokens"),
        max_output_tokens: limit("max_output_tokens"),
    }
}

fn parse_capabilities(value: &serde_json::Map<String, serde_json::Value>) -> ModelCapabilities {
    let enabled = |key: &str| value.get(key).and_then(serde_json::Value::as_bool) == Some(true);
    ModelCapabilities {
        streaming: enabled("streaming"),
        tool_calling: enabled("tool_calling"),
        parallel_tool_calling: enabled("parallel_tool_calling"),
        web_search: enabled("web_search"),
        vision: enabled("vision"),
        audio_input: enabled("audio_input"),
        structured_output: enabled("structured_output"),
        prompt_cache: enabled("prompt_cache"),
        image_generation: enabled("image_generation"),
        image_editing: enabled("image_editing"),
    }
}

fn parse_image_input_mode(value: &str) -> Option<ImageInputMode> {
    match value {
        "native" => Some(ImageInputMode::Native),
        "fallback" => Some(ImageInputMode::Fallback),
        "none" => Some(ImageInputMode::None),
        _ => None,
    }
}

pub fn all_model_ids() -> Vec<&'static str> {
    catalog().read().expect("catalog poisoned").ids.clone()
}

pub fn model_capabilities(model: &str) -> Option<ModelCapabilitySnapshot> {
    let normalized = model.trim();
    catalog()
        .read()
        .expect("catalog poisoned")
        .capabilities
        .iter()
        .find_map(|(id, value)| id.eq_ignore_ascii_case(normalized).then_some(*value))
}

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
