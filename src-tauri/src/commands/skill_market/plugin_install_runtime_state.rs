use sha2::{Digest, Sha256};

use crate::app_error::AppCommandError;
use crate::db::service::plugin_installation_service::PluginInstallationRecord;
use crate::db::service::plugin_runtime_state_service::{
    PluginActivationInput, PluginPermissionGrantInput, PluginRuntimeStateInput,
};

use super::plugin_install_context::PreparedPluginInstall;
use super::plugin_types::SkillPluginManifest;

pub(super) fn trust_state(plugin: &PreparedPluginInstall) -> &'static str {
    if plugin.plugin.manifest.schema_version >= 2 {
        "trusted"
    } else {
        "legacy"
    }
}

pub(super) fn permissions_digest(
    manifest: &SkillPluginManifest,
) -> Result<String, AppCommandError> {
    let bytes = serde_json::to_vec(&manifest.permissions).map_err(|error| {
        AppCommandError::configuration_invalid("Plugin permissions cannot be serialized")
            .with_detail(error.to_string())
    })?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

pub(super) fn runtime_state_input(
    plugin: &PreparedPluginInstall,
    permissions_digest: String,
) -> Result<PluginRuntimeStateInput, AppCommandError> {
    if plugin.plugin.manifest.schema_version < 2 {
        return Ok(PluginRuntimeStateInput::default());
    }
    let mut activations = Vec::new();
    for component in &plugin.plugin.manifest.components {
        if component.kind != "connector" {
            continue;
        }
        let config = component.config.as_ref().ok_or_else(|| {
            AppCommandError::configuration_invalid("Plugin connector config is missing")
        })?;
        activations.push(activation_input(&component.key, config));
    }
    Ok(PluginRuntimeStateInput {
        activations,
        permission_grants: vec![PluginPermissionGrantInput {
            scope: "global".to_string(),
            workspace_key: String::new(),
            permissions_digest,
            granted_permissions_json: "{}".to_string(),
            permission_ceiling_json: serde_json::to_string(&plugin.plugin.manifest.permissions)
                .map_err(|error| {
                    AppCommandError::configuration_invalid(
                        "Plugin permission ceiling cannot be serialized",
                    )
                    .with_detail(error.to_string())
                })?,
            grant_state: "pending".to_string(),
            granted_at: None,
        }],
    })
}

fn activation_input(component_key: &str, config: &serde_json::Value) -> PluginActivationInput {
    let scope = match config["activation"]["scope"].as_str() {
        Some("installation") => "global",
        _ => "workspace",
    };
    PluginActivationInput {
        component_key: component_key.to_string(),
        scope: scope.to_string(),
        workspace_key: String::new(),
        agent_type: String::new(),
        requested_enabled: false,
        routing_mode: config["routing"]["mode"]
            .as_str()
            .unwrap_or("host_gateway")
            .to_string(),
        policy_source: "install_default".to_string(),
    }
}

pub(super) fn runtime_state_from_record(
    record: &PluginInstallationRecord,
) -> PluginRuntimeStateInput {
    PluginRuntimeStateInput {
        activations: record
            .activations
            .iter()
            .map(activation_from_model)
            .collect(),
        permission_grants: record
            .permission_grants
            .iter()
            .map(grant_from_model)
            .collect(),
    }
}

fn activation_from_model(
    value: &crate::db::entities::plugin_activation_policy::Model,
) -> PluginActivationInput {
    PluginActivationInput {
        component_key: value.component_key.clone(),
        scope: value.scope.clone(),
        workspace_key: value.workspace_key.clone(),
        agent_type: value.agent_type.clone(),
        requested_enabled: value.requested_enabled,
        routing_mode: value.routing_mode.clone(),
        policy_source: value.policy_source.clone(),
    }
}

fn grant_from_model(
    value: &crate::db::entities::plugin_permission_grant::Model,
) -> PluginPermissionGrantInput {
    PluginPermissionGrantInput {
        scope: value.scope.clone(),
        workspace_key: value.workspace_key.clone(),
        permissions_digest: value.permissions_digest.clone(),
        granted_permissions_json: value.granted_permissions_json.clone(),
        permission_ceiling_json: value.granted_permissions_json.clone(),
        grant_state: value.grant_state.clone(),
        granted_at: value.granted_at,
    }
}
