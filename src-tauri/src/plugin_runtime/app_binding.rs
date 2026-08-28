use serde_json::Value;

use super::registry::PluginDescriptor;
use super::router::ResolvedRoute;
use super::types::{PluginAppIntent, PluginCallContext, PluginInvokeError};

pub(super) struct AppIntentInput<'a> {
    pub plugin: &'a PluginDescriptor,
    pub route: &'a ResolvedRoute,
    pub context: &'a PluginCallContext,
    pub requested_mode: Option<&'a str>,
    pub launch_payload: Value,
}

pub(super) fn resolve_app_intent(
    input: AppIntentInput<'_>,
) -> Result<Option<PluginAppIntent>, PluginInvokeError> {
    let matches = app_matches(input.plugin, input.route);
    if matches.is_empty() {
        return Ok(None);
    }
    if matches.len() != 1 {
        return Err(contract_error("Capability must bind to at most one app"));
    }
    let (component, config) = matches[0];
    if config["connectorKey"].as_str() != Some(input.route.connector_key.as_str()) {
        return Err(contract_error(
            "Plugin app connector does not match capability",
        ));
    }
    let resource_uri = config["resourceUri"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| contract_error("Plugin app resource URI is missing"))?;
    let display_mode = resolve_display_mode(config, input.requested_mode)?;
    validate_launch_payload(&input.launch_payload)?;
    Ok(Some(PluginAppIntent {
        connection_id: input.context.connection_id.clone(),
        plugin_slug: input.plugin.slug.clone(),
        plugin_version: input.plugin.version.clone(),
        app_key: component.key.clone(),
        resource_uri: resource_uri.to_string(),
        display_mode,
        workspace_key: input.context.workspace_key.clone(),
        permission_revision: input.context.permission_revision.clone(),
        launch_payload: input.launch_payload,
    }))
}

fn app_matches<'a>(
    plugin: &'a PluginDescriptor,
    route: &ResolvedRoute,
) -> Vec<(
    &'a crate::commands::skill_market::SkillPluginComponent,
    &'a Value,
)> {
    plugin
        .manifest
        .components
        .iter()
        .filter(|component| component.kind == "app")
        .filter_map(|component| component.config.as_ref().map(|config| (component, config)))
        .filter(|(_, config)| {
            config["capabilityKey"].as_str() == Some(route.capability_key.as_str())
        })
        .collect()
}

fn resolve_display_mode(
    config: &Value,
    requested: Option<&str>,
) -> Result<String, PluginInvokeError> {
    let modes = config["displayModes"]
        .as_array()
        .ok_or_else(|| contract_error("Plugin app display modes are missing"))?;
    let requested = requested.unwrap_or("inline");
    if !modes.iter().any(|mode| mode.as_str() == Some(requested)) {
        return Err(contract_error(
            "Requested plugin app display mode is unavailable",
        ));
    }
    Ok(requested.to_string())
}

fn validate_launch_payload(value: &Value) -> Result<(), PluginInvokeError> {
    const MAX_LAUNCH_PAYLOAD_BYTES: usize = 256 * 1024;
    let size = serde_json::to_vec(value)
        .map_err(|error| contract_error(error.to_string()))?
        .len();
    if size > MAX_LAUNCH_PAYLOAD_BYTES {
        return Err(contract_error("Plugin app launch payload is too large"));
    }
    Ok(())
}

fn contract_error(message: impl Into<String>) -> PluginInvokeError {
    PluginInvokeError::before_effect("plugin_contract_mismatch", message)
}
