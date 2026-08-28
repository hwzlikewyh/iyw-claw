//! Runtime model catalog with complete and Agent-scoped layers.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock, RwLock};

use crate::acp::provider_overlay_formats::MANAGED_MODEL_IDS;
use crate::acp::registry;
use crate::models::agent::AgentType;

pub use super::model_catalog_types::{
    ImageInputMode, ModelCapabilities, ModelCapabilitySnapshot, ModelLimits,
};
use super::model_catalog_types::{
    ModelCatalogLayer, PersistedCatalog, PersistedCatalogV4, PersistedModel, RuntimeCatalog,
};

const PERSIST_FILE_NAME: &str = "model-catalog.json";
const PERSIST_VERSION: u32 = 4;
const PREVIOUS_PERSIST_VERSIONS: [u32; 2] = [2, 3];

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
    load_persisted().unwrap_or_else(|| RuntimeCatalog {
        complete: layer_from_models(
            MANAGED_MODEL_IDS
                .iter()
                .map(|id| legacy_model((*id).to_string()))
                .collect(),
        ),
        scoped: HashMap::new(),
        agent_platform_ids: HashMap::new(),
    })
}

fn persist_path() -> Option<PathBuf> {
    let data_dir = std::env::var_os("IYW_CLAW_DATA_DIR")?;
    (!data_dir.is_empty()).then(|| PathBuf::from(data_dir).join(PERSIST_FILE_NAME))
}

fn load_persisted() -> Option<RuntimeCatalog> {
    let raw = std::fs::read_to_string(persist_path()?).ok()?;
    let persisted: PersistedCatalog = serde_json::from_str(&raw).ok()?;
    match persisted {
        PersistedCatalog::Legacy(ids) => Some(RuntimeCatalog {
            complete: layer_from_models(ids.into_iter().map(legacy_model).collect()),
            scoped: HashMap::new(),
            agent_platform_ids: HashMap::new(),
        }),
        PersistedCatalog::Current(value) if PREVIOUS_PERSIST_VERSIONS.contains(&value.version) => {
            Some(RuntimeCatalog {
                complete: layer_from_models(value.models),
                scoped: HashMap::new(),
                agent_platform_ids: HashMap::new(),
            })
        }
        PersistedCatalog::Scoped(value) if value.version == PERSIST_VERSION => {
            let scoped = value
                .scoped
                .into_iter()
                .map(|(platform_id, models)| (platform_id, layer_from_models(models)))
                .collect();
            Some(RuntimeCatalog {
                complete: layer_from_models(value.models),
                scoped,
                agent_platform_ids: value.agent_platform_ids,
            })
        }
        _ => None,
    }
}

fn legacy_model(id: String) -> PersistedModel {
    PersistedModel {
        id,
        capabilities: ModelCapabilities::default(),
        image_input_mode: ImageInputMode::None,
        limits: ModelLimits::default(),
    }
}

pub(super) fn layer_from_models(models: Vec<PersistedModel>) -> ModelCatalogLayer {
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
    ModelCatalogLayer { ids, capabilities }
}

fn persisted_models(layer: &ModelCatalogLayer) -> Vec<PersistedModel> {
    layer
        .ids
        .iter()
        .map(|id| {
            let snapshot = layer.capabilities.get(id).copied().unwrap_or_default();
            PersistedModel {
                id: (*id).to_string(),
                capabilities: snapshot.capabilities,
                image_input_mode: snapshot.image_input_mode,
                limits: snapshot.limits,
            }
        })
        .collect()
}

fn persist(value: &RuntimeCatalog) {
    let Some(path) = persist_path() else { return };
    let payload = PersistedCatalogV4 {
        version: PERSIST_VERSION,
        models: persisted_models(&value.complete),
        scoped: value
            .scoped
            .iter()
            .map(|(platform_id, layer)| (platform_id.clone(), persisted_models(layer)))
            .collect(),
        agent_platform_ids: value.agent_platform_ids.clone(),
    };
    let Ok(json) = serde_json::to_string(&payload) else {
        return;
    };
    if let Err(error) = std::fs::write(path, json) {
        tracing::warn!(%error, "[ModelCatalog] failed to persist catalog");
    }
}

pub fn update_from_payload(payload: &serde_json::Value) -> bool {
    let Some(complete) = super::model_catalog_payload::layer_from_payload(payload) else {
        return false;
    };
    if complete.ids.is_empty() {
        return false;
    }
    let changed = {
        let mut current = catalog().write().expect("catalog poisoned");
        let changed = current.complete != complete;
        current.complete = complete;
        changed
    };
    if changed {
        let snapshot = catalog().read().expect("catalog poisoned").clone();
        tracing::info!(
            model_count = snapshot.complete.ids.len(),
            "[ModelCatalog] complete catalog updated"
        );
        persist(&snapshot);
    }
    changed
}

/// Replace only one Agent Platform scoped layer. Empty `data` is authoritative.
pub fn replace_scoped_from_payload(
    agent: AgentType,
    platform_id: &str,
    payload: &serde_json::Value,
) -> bool {
    let Some(layer) = super::model_catalog_payload::layer_from_payload(payload) else {
        return false;
    };
    let platform_id = platform_id.trim();
    if platform_id.is_empty() {
        return false;
    }
    let registry_id = registry::registry_id_for(agent).to_string();
    let changed = {
        let mut current = catalog().write().expect("catalog poisoned");
        let changed = current.scoped.get(platform_id) != Some(&layer)
            || current
                .agent_platform_ids
                .get(&registry_id)
                .map(String::as_str)
                != Some(platform_id);
        current.scoped.insert(platform_id.to_string(), layer);
        current
            .agent_platform_ids
            .insert(registry_id, platform_id.to_string());
        changed
    };
    if changed {
        let snapshot = catalog().read().expect("catalog poisoned").clone();
        let model_count = snapshot
            .scoped
            .get(platform_id)
            .map_or(0, |layer| layer.ids.len());
        tracing::info!(
            agent = %agent,
            platform_id,
            model_count,
            authoritative_empty = model_count == 0,
            "[ModelCatalog] Agent-scoped catalog updated"
        );
        persist(&snapshot);
    }
    changed
}

fn active_layer(agent: AgentType) -> Option<ModelCatalogLayer> {
    let current = catalog().read().expect("catalog poisoned");
    let registry_id = registry::registry_id_for(agent);
    current
        .agent_platform_ids
        .get(registry_id)
        .and_then(|platform_id| current.scoped.get(platform_id))
        .cloned()
}

pub fn has_authoritative_empty_catalog(agent: AgentType) -> bool {
    active_layer(agent).is_some_and(|layer| layer.ids.is_empty())
}

pub fn all_model_ids() -> Vec<&'static str> {
    catalog()
        .read()
        .expect("catalog poisoned")
        .complete
        .ids
        .clone()
}

pub fn model_capabilities(model: &str) -> Option<ModelCapabilitySnapshot> {
    let normalized = model.trim();
    let current = catalog().read().expect("catalog poisoned");
    current
        .complete
        .capabilities
        .iter()
        .find_map(|(id, value)| id.eq_ignore_ascii_case(normalized).then_some(*value))
        .or_else(|| {
            current.scoped.values().find_map(|layer| {
                layer
                    .capabilities
                    .iter()
                    .find_map(|(id, value)| id.eq_ignore_ascii_case(normalized).then_some(*value))
            })
        })
}

pub fn model_ids_for(agent: AgentType) -> Vec<&'static str> {
    active_layer(agent)
        .map(|layer| layer.ids)
        .unwrap_or_else(all_model_ids)
}

pub fn default_model_for(agent: AgentType) -> &'static str {
    model_ids_for(agent)
        .into_iter()
        .next()
        .unwrap_or(MANAGED_MODEL_IDS[0])
}

pub fn compaction_threshold(model: Option<&str>, context_window: u64) -> Option<u64> {
    if let Some(model) = model {
        if let Some(value) = model_capabilities(model)
            .and_then(|snapshot| snapshot.limits.compaction_at_tokens)
            .filter(|value| *value > 0)
        {
            return Some(value);
        }
    }
    match context_window {
        1_000_000 => Some(358_000),
        200_000 => Some(120_000),
        _ => None,
    }
}
