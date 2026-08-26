use std::collections::BTreeMap;
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::Deserialize;

use crate::acp::skill_package::{PackageFile, ValidatedSkillPackage};
use crate::app_error::AppCommandError;

use super::plugin_components::{
    build_manifest, valid_key, validate_summary_bindings, validate_summary_components,
};
use super::plugin_types::SkillPluginManifest;

pub(super) const CODEX_MANIFEST: &str = ".codex-plugin/plugin.json";
const CLAUDE_MANIFEST: &str = ".claude-plugin/plugin.json";
pub(super) const IYW_MANIFEST: &str = ".iyw-plugin.json";
const MCP_MANIFEST: &str = ".mcp.json";
pub(super) const MAX_MANIFEST_BYTES: usize = 1024 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NativeManifest {
    pub(super) name: String,
    pub(super) version: String,
    #[serde(default)]
    pub(super) skills: String,
    #[serde(default)]
    pub(super) mcp_servers: Option<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PortableManifestV1 {
    pub(super) schema_version: u32,
    pub(super) name: String,
    pub(super) version: String,
    pub(super) targets: Vec<String>,
    pub(super) components: PortableComponentsV1,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PortableComponentsV1 {
    #[serde(default)]
    pub(super) skills: Vec<PortableSkill>,
    #[serde(default)]
    pub(super) connectors: Vec<PortableConnector>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PortableSkill {
    pub(super) key: String,
    pub(super) path: String,
    #[serde(default)]
    pub(super) requires_connectors: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PortableConnector {
    pub(super) key: String,
    pub(super) server_key: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct McpManifest {
    #[serde(default)]
    mcp_servers: BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginManifestHeader {
    schema_version: u32,
}

pub(super) struct ValidatedPluginPackage {
    pub(super) manifest: SkillPluginManifest,
    pub(super) connector_specs: BTreeMap<String, serde_json::Value>,
}

pub(super) fn validate_plugin_package(
    package: &ValidatedSkillPackage,
    expected: &SkillPluginManifest,
    slug: &str,
    version: &str,
) -> Result<ValidatedPluginPackage, AppCommandError> {
    validate_plugin_summary(expected, slug, version)?;
    let actual = parse_plugin_package(&package.files, slug, version)?;
    if canonical_manifest(actual.clone()) != canonical_manifest(expected.clone()) {
        return Err(invalid_plugin(
            "Plugin package components do not match the install plan",
        ));
    }
    let connector_specs = parse_connector_specs(&package.files)?;
    Ok(ValidatedPluginPackage {
        manifest: actual,
        connector_specs,
    })
}

pub(super) fn validate_plugin_summary(
    manifest: &SkillPluginManifest,
    slug: &str,
    version: &str,
) -> Result<(), AppCommandError> {
    if manifest.schema_version == 2 {
        return super::plugin_manifest_v2::validate_summary_v2(manifest, slug, version);
    }
    if manifest.schema_version != 1
        || manifest.name != slug
        || manifest.version != version
        || manifest.targets != ["codex", "claude_code"]
        || manifest.components.is_empty()
    {
        return Err(invalid_plugin("Plugin install plan metadata is invalid"));
    }
    let (skills, connectors) = validate_summary_components(&manifest.components)?;
    validate_summary_bindings(&manifest.bindings, &skills, &connectors)
}

fn parse_plugin_package(
    files: &[PackageFile],
    slug: &str,
    version: &str,
) -> Result<SkillPluginManifest, AppCommandError> {
    let header: PluginManifestHeader = parse_document(files, IYW_MANIFEST)?;
    if header.schema_version == 2 {
        return super::plugin_manifest_v2::parse_plugin_package_v2(files, slug, version);
    }
    if header.schema_version != 1 {
        return Err(invalid_plugin(format!(
            "Unsupported plugin schemaVersion {}",
            header.schema_version
        )));
    }
    let codex: NativeManifest = parse_document(files, CODEX_MANIFEST)?;
    let claude: NativeManifest = parse_document(files, CLAUDE_MANIFEST)?;
    let portable: PortableManifestV1 = parse_document(files, IYW_MANIFEST)?;
    validate_identity(&codex, &claude, &portable, slug, version)?;
    let servers = parse_mcp_servers(files, &codex)?;
    build_manifest(files, portable, servers.keys().cloned().collect())
}

fn validate_identity(
    codex: &NativeManifest,
    claude: &NativeManifest,
    portable: &PortableManifestV1,
    slug: &str,
    version: &str,
) -> Result<(), AppCommandError> {
    if codex.name != slug
        || claude.name != slug
        || portable.name != slug
        || codex.version != version
        || claude.version != version
        || portable.version != version
        || portable.schema_version != 1
        || portable.targets != ["codex", "claude_code"]
    {
        return Err(invalid_plugin(
            "Plugin manifests have inconsistent identities",
        ));
    }
    Ok(())
}

fn parse_mcp_servers(
    files: &[PackageFile],
    codex: &NativeManifest,
) -> Result<BTreeMap<String, serde_json::Value>, AppCommandError> {
    let has_manifest = find_file(files, MCP_MANIFEST).is_some();
    if !has_manifest {
        if codex.mcp_servers.is_some() {
            return Err(invalid_plugin(
                "Codex mcpServers must be absent without .mcp.json",
            ));
        }
        return Ok(BTreeMap::new());
    }
    if codex.mcp_servers.as_ref().and_then(|value| value.as_str()) != Some("./.mcp.json") {
        return Err(invalid_plugin(
            "Codex mcpServers must reference ./.mcp.json",
        ));
    }
    let manifest: McpManifest = parse_document(files, MCP_MANIFEST)?;
    if manifest.mcp_servers.keys().any(|key| !valid_key(key)) {
        return Err(invalid_plugin("Plugin contains an invalid MCP server key"));
    }
    Ok(manifest.mcp_servers)
}

fn parse_connector_specs(
    files: &[PackageFile],
) -> Result<BTreeMap<String, serde_json::Value>, AppCommandError> {
    if find_file(files, MCP_MANIFEST).is_none() {
        return Ok(BTreeMap::new());
    }
    let manifest: McpManifest = parse_document(files, MCP_MANIFEST)?;
    Ok(manifest.mcp_servers)
}

pub(super) fn parse_document<T: DeserializeOwned>(
    files: &[PackageFile],
    path: &str,
) -> Result<T, AppCommandError> {
    let bytes = find_file(files, path)
        .filter(|bytes| !bytes.is_empty() && bytes.len() <= MAX_MANIFEST_BYTES)
        .ok_or_else(|| invalid_plugin(format!("Missing or oversized {path}")))?;
    serde_json::from_slice(bytes)
        .map_err(|error| invalid_plugin(format!("Invalid {path}")).with_detail(error.to_string()))
}

pub(super) fn find_file<'a>(files: &'a [PackageFile], path: &str) -> Option<&'a [u8]> {
    files
        .iter()
        .find(|file| file.path == Path::new(path))
        .map(|file| file.bytes.as_slice())
}

fn canonical_manifest(mut value: SkillPluginManifest) -> SkillPluginManifest {
    value.components.sort_by(|left, right| {
        (&left.kind, &left.key, &left.path, &left.server_key).cmp(&(
            &right.kind,
            &right.key,
            &right.path,
            &right.server_key,
        ))
    });
    value.bindings.sort();
    value
}

pub(super) fn invalid_plugin(message: impl Into<String>) -> AppCommandError {
    AppCommandError::configuration_invalid(message)
}
