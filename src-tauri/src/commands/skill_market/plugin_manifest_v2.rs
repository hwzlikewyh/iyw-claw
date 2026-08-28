use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::acp::skill_package::PackageFile;
use crate::app_error::AppCommandError;

use super::plugin_components::{valid_key, validate_summary_bindings};
use super::plugin_manifest::{find_file, invalid_plugin, parse_document, IYW_MANIFEST};
mod build;

use build::Builder;

use super::plugin_types::{
    deserialize_null_vec, SkillPluginBinding, SkillPluginComponent, SkillPluginManifest,
    SkillPluginPermissions,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManifestV2 {
    schema_version: u32,
    name: String,
    version: String,
    targets: Vec<String>,
    components: ComponentsV2,
    permissions: SkillPluginPermissions,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ComponentsV2 {
    #[serde(default, deserialize_with = "deserialize_null_vec")]
    skills: Vec<SkillV2>,
    #[serde(default, deserialize_with = "deserialize_null_vec")]
    runtimes: Vec<RuntimeV2>,
    #[serde(default, deserialize_with = "deserialize_null_vec")]
    connectors: Vec<ConnectorV2>,
    #[serde(default, deserialize_with = "deserialize_null_vec")]
    capabilities: Vec<CapabilityV2>,
    #[serde(default, deserialize_with = "deserialize_null_vec")]
    apps: Vec<AppV2>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SkillV2 {
    key: String,
    path: String,
    #[serde(default, deserialize_with = "deserialize_null_vec")]
    requires_connectors: Vec<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeV2 {
    key: String,
    kind: String,
    entrypoint: String,
    dependencies: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConnectorV2 {
    key: String,
    transport: String,
    runtime_key: String,
    routing: RoutingV2,
    activation: ActivationV2,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RoutingV2 {
    mode: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ActivationV2 {
    mode: String,
    scope: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CapabilityV2 {
    key: String,
    id: String,
    connector_key: String,
    tool_name: String,
    schema_path: String,
    description: String,
    intent_terms: Vec<String>,
    read_only_hint: bool,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AppV2 {
    key: String,
    connector_key: String,
    resource_uri: String,
    capability_key: String,
    display_modes: Vec<String>,
}

pub(super) fn parse_plugin_package_v2(
    files: &[PackageFile],
    slug: &str,
    version: &str,
) -> Result<SkillPluginManifest, AppCommandError> {
    for forbidden in [
        ".codex-plugin/plugin.json",
        ".claude-plugin/plugin.json",
        ".mcp.json",
    ] {
        if find_file(files, forbidden).is_some() {
            return Err(invalid_plugin(format!(
                "Plugin v2 HostGateway package must not contain {forbidden}"
            )));
        }
    }
    let value: ManifestV2 = parse_document(files, IYW_MANIFEST)?;
    if value.schema_version != 2
        || value.name != slug
        || value.version != version
        || value.targets != ["iyw-claw"]
    {
        return Err(invalid_plugin("Plugin v2 identity is invalid"));
    }
    validate_permissions(&value.permissions)?;
    let mut builder = Builder::new(files);
    builder.add_runtimes(value.components.runtimes)?;
    builder.add_connectors(value.components.connectors)?;
    builder.add_skills(value.components.skills)?;
    builder.add_capabilities(slug, value.components.capabilities)?;
    builder.add_apps(value.components.apps)?;
    let mut manifest = SkillPluginManifest {
        schema_version: 2,
        name: value.name,
        version: value.version,
        targets: value.targets,
        components: builder.components,
        bindings: builder.bindings,
        permissions: Some(value.permissions),
        manifest_digest: None,
    };
    sort_manifest(&mut manifest);
    manifest.manifest_digest = Some(manifest_digest(&manifest)?);
    Ok(manifest)
}

pub(super) fn validate_summary_v2(
    value: &SkillPluginManifest,
    slug: &str,
    version: &str,
) -> Result<(), AppCommandError> {
    let permissions = value
        .permissions
        .as_ref()
        .ok_or_else(|| invalid_plugin("Plugin v2 install plan permissions are missing"))?;
    if value.name != slug
        || value.version != version
        || value.targets != ["iyw-claw"]
        || value.components.is_empty()
        || value
            .manifest_digest
            .as_deref()
            .is_none_or(|digest| digest.len() != 71 || !digest.starts_with("sha256:"))
    {
        return Err(invalid_plugin("Plugin v2 install plan metadata is invalid"));
    }
    validate_permissions(permissions)?;
    let expected_digest = manifest_digest(value)?;
    if value.manifest_digest.as_deref() != Some(expected_digest.as_str()) {
        return Err(invalid_plugin("Plugin v2 install plan digest is invalid"));
    }
    let mut kinds: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for component in &value.components {
        if !matches!(
            component.kind.as_str(),
            "skill" | "runtime" | "connector" | "capability" | "app"
        ) || !valid_key(&component.key)
            || component.config.is_none()
            || !kinds
                .entry(&component.kind)
                .or_default()
                .insert(component.key.clone())
        {
            return Err(invalid_plugin("Plugin v2 components are invalid"));
        }
    }
    let empty = BTreeSet::new();
    validate_summary_bindings(
        &value.bindings,
        kinds.get("skill").unwrap_or(&empty),
        kinds.get("connector").unwrap_or(&empty),
    )
}

fn sort_manifest(value: &mut SkillPluginManifest) {
    value.components.sort_by(|left, right| {
        (&left.kind, &left.key, &left.path, &left.server_key).cmp(&(
            &right.kind,
            &right.key,
            &right.path,
            &right.server_key,
        ))
    });
    value.bindings.sort();
}

fn manifest_digest(value: &SkillPluginManifest) -> Result<String, AppCommandError> {
    let mut value = value.clone();
    value.manifest_digest = None;
    let value = serde_json::to_value(value).map_err(|error| {
        invalid_plugin("Plugin v2 manifest cannot be canonicalized").with_detail(error.to_string())
    })?;
    let bytes = serde_json::to_string(&value).map_err(|error| {
        invalid_plugin("Plugin v2 manifest cannot be canonicalized").with_detail(error.to_string())
    })?;
    let bytes = bytes
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    Ok(format!("sha256:{:x}", Sha256::digest(bytes.as_bytes())))
}

fn component(
    kind: &str,
    key: String,
    path: String,
    server_key: String,
    config: Value,
) -> SkillPluginComponent {
    SkillPluginComponent {
        kind: kind.into(),
        key,
        path,
        server_key,
        config: Some(config),
    }
}

fn config(value: impl Serialize) -> Result<Value, AppCommandError> {
    serde_json::to_value(value).map_err(|error| {
        invalid_plugin("Plugin v2 component is invalid").with_detail(error.to_string())
    })
}

fn validate_permissions(value: &SkillPluginPermissions) -> Result<(), AppCommandError> {
    for path in value.workspace.read.iter().chain(&value.workspace.write) {
        if path.is_empty()
            || path.starts_with('/')
            || path.contains('\\')
            || path
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err(invalid_plugin("Plugin v2 workspace permission is unsafe"));
        }
    }
    for domain in value
        .network
        .connect_domains
        .iter()
        .chain(&value.network.resource_domains)
        .chain(&value.network.frame_domains)
        .chain(&value.network.base_uri_domains)
    {
        let parsed = reqwest::Url::parse(domain).ok();
        if domain.chars().any(|character| {
            character.is_control() || character.is_whitespace() || character == '\\'
        }) || parsed.as_ref().is_none_or(|url| {
            url.scheme() != "https"
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
                || !matches!(url.path(), "" | "/")
        }) {
            return Err(invalid_plugin(
                "Plugin v2 network permission must use HTTPS",
            ));
        }
    }
    if value.host.iter().any(|item| {
        !matches!(
            item.as_str(),
            "send-message"
                | "clipboard-write"
                | "open-link"
                | "camera"
                | "microphone"
                | "geolocation"
        )
    }) {
        return Err(invalid_plugin("Plugin v2 host permission is unsupported"));
    }
    Ok(())
}
