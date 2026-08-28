use serde::Serialize;

use crate::commands::skill_market::SkillPluginPermissions;
use crate::plugin_runtime::registry::PluginDescriptor;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAppResourceMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub csp: Option<PluginAppResourceCsp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<PluginAppResourcePermissions>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAppResourceCsp {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connect_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_uri_domains: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAppResourcePermissions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera: Option<PluginAppPermissionGrant>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub microphone: Option<PluginAppPermissionGrant>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geolocation: Option<PluginAppPermissionGrant>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clipboard_write: Option<PluginAppPermissionGrant>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginAppPermissionGrant {}

pub fn from_resource(
    resource: &rmcp::model::ReadResourceResult,
    plugin: &PluginDescriptor,
    workspace_key: &str,
) -> Option<PluginAppResourceMeta> {
    let raw = resource.contents.iter().find_map(content_meta)?;
    let ui = raw.get("ui")?;
    let ceiling = plugin.manifest.permissions.as_ref();
    let granted = granted_permissions(plugin, workspace_key)?;
    let csp = ui
        .get("csp")
        .map(|value| csp_meta(value, ceiling, &granted));
    let permissions = ui
        .get("permissions")
        .map(|value| permission_meta(value, ceiling, &granted));
    (csp.is_some() || permissions.is_some()).then_some(PluginAppResourceMeta { csp, permissions })
}

fn content_meta(content: &rmcp::model::ResourceContents) -> Option<serde_json::Value> {
    match content {
        rmcp::model::ResourceContents::TextResourceContents { meta, .. }
        | rmcp::model::ResourceContents::BlobResourceContents { meta, .. } => meta
            .as_ref()
            .and_then(|value| serde_json::to_value(value).ok()),
    }
}

fn csp_meta(
    value: &serde_json::Value,
    ceiling: Option<&SkillPluginPermissions>,
    granted: &SkillPluginPermissions,
) -> PluginAppResourceCsp {
    let network = ceiling.map(|value| &value.network);
    let allowed = &granted.network;
    PluginAppResourceCsp {
        connect_domains: domains(
            value,
            "connectDomains",
            network.map(|value| &value.connect_domains),
            &allowed.connect_domains,
        ),
        resource_domains: domains(
            value,
            "resourceDomains",
            network.map(|value| &value.resource_domains),
            &allowed.resource_domains,
        ),
        frame_domains: domains(
            value,
            "frameDomains",
            network.map(|value| &value.frame_domains),
            &allowed.frame_domains,
        ),
        base_uri_domains: domains(
            value,
            "baseUriDomains",
            network.map(|value| &value.base_uri_domains),
            &allowed.base_uri_domains,
        ),
    }
}

fn permission_meta(
    value: &serde_json::Value,
    ceiling: Option<&SkillPluginPermissions>,
    granted: &SkillPluginPermissions,
) -> PluginAppResourcePermissions {
    let ceiling_host = ceiling.map_or(&[][..], |value| value.host.as_slice());
    let allowed = &granted.host;
    let can = |key: &str, permission: &str| {
        value
            .get(key)
            .is_some_and(|item| item.is_object() || item == true)
            && ceiling_host.iter().any(|item| item == permission)
            && allowed.iter().any(|item| item == permission)
    };
    PluginAppResourcePermissions {
        camera: can("camera", "camera").then_some(PluginAppPermissionGrant {}),
        microphone: can("microphone", "microphone").then_some(PluginAppPermissionGrant {}),
        geolocation: can("geolocation", "geolocation").then_some(PluginAppPermissionGrant {}),
        clipboard_write: can("clipboardWrite", "clipboard-write")
            .then_some(PluginAppPermissionGrant {}),
    }
}

fn domains(
    value: &serde_json::Value,
    key: &str,
    ceiling: Option<&Vec<String>>,
    granted: &[String],
) -> Option<Vec<String>> {
    let values = value.get(key)?.as_array()?;
    Some(
        values
            .iter()
            .filter_map(serde_json::Value::as_str)
            .filter(|domain| {
                ceiling.is_some_and(|items| items.iter().any(|item| item == *domain))
                    && granted.iter().any(|item| item == *domain)
            })
            .map(str::to_string)
            .collect(),
    )
}

fn granted_permissions(
    plugin: &PluginDescriptor,
    workspace_key: &str,
) -> Option<SkillPluginPermissions> {
    plugin
        .permission_grants
        .iter()
        .find(|grant| {
            grant.permissions_digest == plugin.permissions_digest
                && grant.grant_state == "granted"
                && (grant.scope == "global" || grant.workspace_key == workspace_key)
        })
        .and_then(|grant| serde_json::from_str(&grant.granted_permissions_json).ok())
}
