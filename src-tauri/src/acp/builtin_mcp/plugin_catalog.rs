use std::path::Path;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::models::AgentType;
use crate::plugin_runtime::registry::{PluginDescriptor, PluginRegistrySnapshot};

use super::features::FeatureSnapshot;

const MAX_QUERY_CHARS: usize = 256;

pub(super) struct PluginCapabilityRegistry;

impl PluginCapabilityRegistry {
    pub(super) fn search(
        snapshot: Option<&PluginRegistrySnapshot>,
        _features: &FeatureSnapshot,
        query: &str,
        cwd: &Path,
        agent_type: AgentType,
        limit: usize,
    ) -> Vec<Value> {
        let terms = query
            .split_whitespace()
            .map(str::to_ascii_lowercase)
            .collect::<Vec<_>>();
        if terms.is_empty() || query.chars().count() > MAX_QUERY_CHARS {
            return Vec::new();
        }
        let mut matches = snapshot
            .into_iter()
            .flat_map(|snapshot| snapshot.plugins.values())
            .filter(|plugin| plugin.manifest.schema_version >= 2)
            .flat_map(|plugin| capabilities(plugin, snapshot, (cwd, agent_type)))
            .filter_map(|capability| {
                let haystack = capability
                    .get("_searchText")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let score = terms
                    .iter()
                    .filter(|term| haystack.contains(term.as_str()))
                    .count();
                (score > 0).then_some((score, capability))
            })
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| right.0.cmp(&left.0));
        matches
            .into_iter()
            .take(limit)
            .map(|(_, mut value)| {
                if let Some(object) = value.as_object_mut() {
                    object.remove("_searchText");
                }
                value
            })
            .collect()
    }

    pub(super) fn read(
        snapshot: Option<&PluginRegistrySnapshot>,
        capability_id: &str,
        _features: &FeatureSnapshot,
        cwd: &Path,
        agent_type: AgentType,
    ) -> Option<Value> {
        snapshot
            .into_iter()
            .flat_map(|snapshot| snapshot.plugins.values())
            .filter(|plugin| plugin.manifest.schema_version >= 2)
            .flat_map(|plugin| capabilities(plugin, snapshot, (cwd, agent_type)))
            .find_map(|mut capability| {
                (capability.get("capability_id").and_then(Value::as_str) == Some(capability_id))
                    .then(|| {
                        capability
                            .as_object_mut()
                            .and_then(|object| object.remove("_searchText"));
                        capability
                    })
            })
    }
}

fn capabilities(plugin: &PluginDescriptor, snapshot: Option<&PluginRegistrySnapshot>, context: (&Path, AgentType)) -> Vec<Value> {
    let (cwd, agent_type) = context;
    if !agent_type_supports_host_gateway(agent_type) {
        return Vec::new();
    }
    plugin
        .manifest
        .components
        .iter()
        .filter(|component| component.kind == "capability")
        .filter_map(|component| {
            let config = component.config.as_ref()?;
            let id = config.get("id")?.as_str()?;
            let connector = config.get("connectorKey")?.as_str()?;
            let tool_name = config.get("toolName")?.as_str()?;
            if !crate::plugin_runtime::native_compat::model_tool_allowed(
                Path::new(&plugin.install_root),
                connector,
                tool_name,
            ) {
                return None;
            }
            let schema_path = config.get("schemaPath")?.as_str()?;
            let (input_schema, schema_digest) = load_schema(plugin, schema_path)?;
            let unavailable_reason = unavailable_reason(plugin, connector, (snapshot, cwd, agent_type));
            let status = if unavailable_reason.is_none() {
                "available"
            } else {
                "unavailable"
            };
            let description = config
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let intent_terms = config
                .get("intentTerms")
                .cloned()
                .unwrap_or_else(|| Value::Array(Vec::new()));
            let search_text = format!("{} {} {} {}", id, description, intent_terms, plugin.slug)
                .to_ascii_lowercase();
            Some(json!({
                "capability_id": id,
                "summary": description,
                "description": description,
                "category": "plugin",
                "aliases": [plugin.slug],
                "intent_terms": intent_terms,
                "when_to_use": description,
                "required_inputs": [],
                "schema_digest": schema_digest,
                "input_schema": input_schema,
                "status": status,
                "unavailable_reason": unavailable_reason,
                "plugin_slug": plugin.slug,
                "plugin_version": plugin.version,
                "_searchText": search_text,
            }))
        })
        .collect()
}

fn unavailable_reason(
    plugin: &PluginDescriptor,
    connector: &str,
    context: (Option<&PluginRegistrySnapshot>, &Path, AgentType),
) -> Option<&'static str> {
    let (snapshot, cwd, agent_type) = context;
    if !plugin.available {
        return Some("plugin_unavailable");
    }
    if !activation_enabled(plugin, connector, cwd, agent_type) {
        return Some("connector_disabled");
    }
    if !permission_granted(plugin, cwd) {
        return Some("permission_pending");
    }
    let workspace = workspace_key(cwd);
    if snapshot.is_none_or(|snapshot| crate::plugin_runtime::dependency_scope::require(plugin, &snapshot.plugins, (&workspace, agent_type.as_wire().as_ref())).is_err()) {
        return Some("dependency_unavailable");
    }
    None
}

fn load_schema(plugin: &PluginDescriptor, relative: &str) -> Option<(Value, String)> {
    let root = std::fs::canonicalize(&plugin.install_root).ok()?;
    let path = std::fs::canonicalize(root.join(relative)).ok()?;
    if !path.starts_with(&root) {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() > 1 << 20 {
        return None;
    }
    let value = serde_json::from_slice(&bytes).ok()?;
    Some((value, format!("sha256:{:x}", Sha256::digest(&bytes))))
}

fn activation_enabled(
    plugin: &PluginDescriptor,
    connector: &str,
    cwd: &Path,
    agent_type: AgentType,
) -> bool {
    let workspace_key = workspace_key(cwd);
    let agent_type = agent_type.as_wire();
    crate::plugin_runtime::policy::activation_enabled(plugin, (connector, &workspace_key, agent_type.as_ref()))
}

fn permission_granted(plugin: &PluginDescriptor, cwd: &Path) -> bool {
    let workspace_key = workspace_key(cwd);
    crate::plugin_runtime::policy::permission_granted(plugin, &workspace_key)
}

fn workspace_key(cwd: &Path) -> String {
    crate::commands::skill_inventory::workspace_key(Some(cwd.to_string_lossy().as_ref()))
}

fn agent_type_supports_host_gateway(agent_type: AgentType) -> bool {
    crate::acp::connection::agent_supports_builtin_mcp(agent_type)
}
