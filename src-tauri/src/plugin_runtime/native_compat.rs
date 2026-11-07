use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

use super::types::{PluginInvokeError, RuntimeLaunchSpec};

const METADATA_FILE: &str = "native-plugin.json";
const MAX_METADATA_BYTES: u64 = 1024 * 1024;
const MAX_STARTUP_SECONDS: u64 = 600;
const MAX_TOOL_SECONDS: u64 = 7200;
const MAX_RESOURCE_TEMPLATES: usize = 256;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NativePlugin {
    schema_version: u32,
    source: Value,
    connectors: BTreeMap<String, NativeConnector>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeConnector {
    pub entrypoint: String,
    pub env_vars: Vec<String>,
    pub data_dir_env_vars: Vec<String>,
    startup_timeout_sec: u64,
    tool_timeout_sec: u64,
    pub model_tools: Vec<String>,
    pub app_tools: Vec<String>,
    pub resources: Vec<Value>,
    #[serde(default)]
    pub resource_templates: Vec<Value>,
    #[serde(default)]
    pub wasm_unsafe_eval: bool,
    preferred_display_modes: BTreeMap<String, String>,
}

pub(crate) fn connector(
    root: &Path,
    key: &str,
) -> Result<Option<NativeConnector>, PluginInvokeError> {
    let path = root.join(METADATA_FILE);
    let metadata = match std::fs::metadata(&path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(invalid(error.to_string())),
    };
    if metadata.len() > MAX_METADATA_BYTES {
        return Err(invalid("Native plugin metadata is oversized"));
    }
    let value: NativePlugin =
        serde_json::from_slice(&std::fs::read(path).map_err(|error| invalid(error.to_string()))?)
            .map_err(|error| invalid(error.to_string()))?;
    if value.schema_version != 1 || !value.source.is_object() {
        return Err(invalid("Unsupported native plugin metadata"));
    }
    let connector = value
        .connectors
        .get(key)
        .cloned()
        .ok_or_else(|| invalid(format!("Native connector metadata is missing: {key}")))?;
    validate(&connector)?;
    Ok(Some(connector))
}

fn validate(value: &NativeConnector) -> Result<(), PluginInvokeError> {
    if !(1..=MAX_STARTUP_SECONDS).contains(&value.startup_timeout_sec)
        || !(1..=MAX_TOOL_SECONDS).contains(&value.tool_timeout_sec)
        || value.entrypoint.is_empty()
        || value.env_vars.iter().any(|key| !safe_environment_key(key))
        || value.data_dir_env_vars.iter().any(|key| {
            !value.env_vars.contains(key) || !key.to_ascii_uppercase().ends_with("_DATA_DIR")
        })
        || value.resources.iter().any(|item| {
            item["uri"]
                .as_str()
                .is_none_or(|uri| !uri.starts_with("ui://"))
        })
        || value.resource_templates.len() > MAX_RESOURCE_TEMPLATES
        || value.resource_templates.iter().any(|item| {
            item["uriTemplate"]
                .as_str()
                .is_none_or(|template| !super::native_resources::valid_template(template))
        })
    {
        return Err(invalid("Native connector metadata is invalid"));
    }
    Ok(())
}

fn safe_environment_key(key: &str) -> bool {
    let mut bytes = key.bytes();
    let valid_first = bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_');
    let upper = key.to_ascii_uppercase();
    valid_first
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        && ![
            "IYW_",
            "CODEX_",
            "CLAUDE_",
            "OPENAI_",
            "ANTHROPIC_",
            "AWS_",
            "AZURE_",
        ]
        .iter()
        .any(|prefix| upper.starts_with(prefix))
        && !matches!(
            upper.as_str(),
            "NODE_OPTIONS" | "NODE_PATH" | "LD_PRELOAD" | "DYLD_INSERT_LIBRARIES"
        )
}

pub(super) fn options(
    spec: &RuntimeLaunchSpec,
) -> Result<Option<NativeConnector>, PluginInvokeError> {
    let options = connector(&spec.install_root, &spec.key.connector_key)?;
    if options
        .as_ref()
        .is_some_and(|value| value.entrypoint != spec.entrypoint)
    {
        return Err(invalid(
            "Native runtime entrypoint does not match its component",
        ));
    }
    Ok(options)
}

pub(super) fn startup_timeout(spec: &RuntimeLaunchSpec) -> Result<Duration, PluginInvokeError> {
    Ok(Duration::from_secs(
        options(spec)?.map_or(15, |value| value.startup_timeout_sec),
    ))
}

pub(super) fn tool_timeout(spec: &RuntimeLaunchSpec) -> Result<Duration, PluginInvokeError> {
    Ok(Duration::from_secs(
        options(spec)?.map_or(120, |value| value.tool_timeout_sec),
    ))
}

pub(super) fn validate_visibility(
    tools: &BTreeMap<String, rmcp::model::Tool>,
    spec: &RuntimeLaunchSpec,
) -> Result<(), PluginInvokeError> {
    let Some(options) = options(spec)? else {
        return Ok(());
    };
    for expected in &spec.expected_tools {
        let tool = tools
            .get(&expected.name)
            .ok_or_else(|| invalid("Native tool is missing"))?;
        let value = serde_json::to_value(tool).map_err(|error| invalid(error.to_string()))?;
        let visibility = value["_meta"]["ui"]["visibility"].as_array();
        let allows = |target: &str| {
            visibility.is_none_or(|items| items.iter().any(|item| item.as_str() == Some(target)))
        };
        if allows("model") != options.model_tools.contains(&expected.name)
            || allows("app") != options.app_tools.contains(&expected.name)
        {
            return Err(invalid(format!(
                "Native tool visibility changed: {}",
                expected.name
            )));
        }
    }
    Ok(())
}

pub(crate) fn model_tool_allowed(root: &Path, connector_key: &str, tool: &str) -> bool {
    connector(root, connector_key)
        .map(|value| value.is_none_or(|value| value.model_tools.iter().any(|name| name == tool)))
        .unwrap_or(false)
}

pub(super) fn app_tool_allowed(root: &Path, connector_key: &str, tool: &str) -> bool {
    connector(root, connector_key)
        .map(|value| value.is_none_or(|value| value.app_tools.iter().any(|name| name == tool)))
        .unwrap_or(false)
}

pub(super) fn preferred_mode(root: &Path, connector_key: &str, uri: &str) -> Option<String> {
    connector(root, connector_key)
        .ok()
        .flatten()?
        .preferred_display_modes
        .get(uri)
        .cloned()
}

pub(crate) fn capability_timeout(capability_id: &str) -> Option<Duration> {
    let snapshot = super::registry::global_snapshot()?;
    for plugin in snapshot.plugins.values() {
        let component = plugin
            .manifest
            .components
            .iter()
            .filter_map(|item| item.config.as_ref())
            .find(|config| config["id"].as_str() == Some(capability_id));
        let Some(config) = component else { continue };
        let options = connector(
            Path::new(&plugin.install_root),
            config["connectorKey"].as_str()?,
        )
        .ok()
        .flatten()?;
        return Some(Duration::from_secs(
            options.startup_timeout_sec + options.tool_timeout_sec + 30,
        ));
    }
    None
}

pub(super) fn app_call_timeout(plugin_slug: &str, app_key: &str) -> Option<Duration> {
    let snapshot = super::registry::global_snapshot()?;
    let plugin = snapshot.plugins.get(plugin_slug)?;
    let app = plugin
        .manifest
        .components
        .iter()
        .find(|item| item.kind == "app" && item.key == app_key)?
        .config
        .as_ref()?;
    let options = connector(
        Path::new(&plugin.install_root),
        app["connectorKey"].as_str()?,
    )
    .ok()
    .flatten()?;
    Some(Duration::from_secs(options.tool_timeout_sec + 30))
}

fn invalid(message: impl Into<String>) -> PluginInvokeError {
    PluginInvokeError::before_effect("plugin_contract_mismatch", message)
}
