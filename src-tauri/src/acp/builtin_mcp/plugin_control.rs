use rmcp::ErrorData;
use serde_json::{json, Map, Value};

pub(super) const INSTALL_CAPABILITY: &str = "iyw.plugins.install.request.v1";
pub(super) const ENABLE_CAPABILITY: &str = "iyw.plugins.enable.request.v1";

pub(super) enum PluginControlRequest {
    Install(PluginInstallRequest),
    Enable(PluginEnableRequest),
}

pub(super) struct PluginInstallRequest {
    pub skill_id: String,
    pub version: String,
    pub plugin_name: String,
}

pub(super) struct PluginEnableRequest {
    pub plugin_slug: String,
}

pub(super) fn search_summaries(query: &str) -> Vec<Value> {
    let lowered = query.to_ascii_lowercase();
    let mut values = Vec::new();
    if is_plugin_action(&lowered, query, "install", "安装") {
        values.push(install_summary());
    }
    if is_plugin_action(&lowered, query, "enable", "启用")
        || lowered.contains("permission")
        || query.contains("授权")
    {
        values.push(enable_summary());
    }
    values
}

pub(super) fn read_detail(capability_id: &str) -> Option<Value> {
    match capability_id {
        INSTALL_CAPABILITY => Some(install_detail()),
        ENABLE_CAPABILITY => Some(enable_detail()),
        _ => None,
    }
}

pub(super) fn parse_request(
    capability_id: &str,
    arguments: Map<String, Value>,
) -> Result<Option<PluginControlRequest>, ErrorData> {
    match capability_id {
        INSTALL_CAPABILITY => Ok(Some(PluginControlRequest::Install(PluginInstallRequest {
            skill_id: bounded_string(&arguments, "skill_id", 64)?,
            version: bounded_string(&arguments, "version", 64)?,
            plugin_name: bounded_string(&arguments, "plugin_name", 128)?,
        }))),
        ENABLE_CAPABILITY => Ok(Some(PluginControlRequest::Enable(PluginEnableRequest {
            plugin_slug: bounded_string(&arguments, "plugin_slug", 128)?,
        }))),
        _ => Ok(None),
    }
}

fn is_plugin_action(lowered: &str, original: &str, english: &str, chinese: &str) -> bool {
    (lowered.contains("plugin") || original.contains("插件"))
        && (lowered.contains(english) || original.contains(chinese))
}

fn install_summary() -> Value {
    json!({
        "capability_id": INSTALL_CAPABILITY,
        "summary": "Request approval before downloading a verified plugin.",
        "category": "plugin",
        "aliases": ["install plugin", "安装插件"],
        "intent_terms": ["plugin", "install", "安装", "插件"],
        "when_to_use": "Use when a concrete plugin and version are not installed.",
        "required_inputs": ["skill_id", "version", "plugin_name"],
        "schema_digest": "builtin:plugin-install-request:v1",
        "status": "install_required"
    })
}

fn enable_summary() -> Value {
    json!({
        "capability_id": ENABLE_CAPABILITY,
        "summary": "Request approval to enable an installed plugin for this workspace and Agent.",
        "category": "plugin",
        "aliases": ["enable plugin", "authorize plugin", "启用插件", "授权插件"],
        "intent_terms": ["plugin", "enable", "permission", "插件", "启用", "授权"],
        "when_to_use": "Use when an installed plugin reports connector_disabled or permission_pending.",
        "required_inputs": ["plugin_slug"],
        "schema_digest": "builtin:plugin-enable-request:v1",
        "status": "available"
    })
}

fn install_detail() -> Value {
    detail(
        install_summary(),
        "Request approval before downloading verified executable plugin code.",
        json!({
            "type": "object",
            "required": ["skill_id", "version", "plugin_name"],
            "properties": {
                "skill_id": {"type": "string"},
                "version": {"type": "string"},
                "plugin_name": {"type": "string"}
            },
            "additionalProperties": false
        }),
    )
}

fn enable_detail() -> Value {
    detail(
        enable_summary(),
        "Request approval for the installed plugin's declared permissions in this workspace and Agent.",
        json!({
            "type": "object",
            "required": ["plugin_slug"],
            "properties": {"plugin_slug": {"type": "string"}},
            "additionalProperties": false
        }),
    )
}

fn detail(mut capability: Value, description: &str, input_schema: Value) -> Value {
    capability["description"] = json!(description);
    capability["input_schema"] = input_schema;
    capability["status"] = json!("available");
    let digest = capability["schema_digest"].clone();
    json!({"capability": capability, "catalog_digest": digest})
}

fn bounded_string(
    arguments: &Map<String, Value>,
    name: &str,
    max_chars: usize,
) -> Result<String, ErrorData> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.chars().count() <= max_chars)
        .map(str::to_string)
        .ok_or_else(|| ErrorData::invalid_params(format!("{name} is required"), None))
}
