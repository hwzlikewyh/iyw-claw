use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use super::types::AgentOffer;
use crate::acp::error::AcpError;

const BUNDLE_SCHEMA_VERSION: u32 = 1;
const BUNDLE_BUILDER: &str = "managed-component-sync";
const BUNDLE_MANIFEST_NAME: &str = "iyw-agent-bundle.json";
const MAX_BUNDLE_MANIFEST_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BundleManifest {
    schema_version: u32,
    registry_id: String,
    version: String,
    delivery_kind: String,
    target: String,
    arch: String,
    entrypoints: BTreeMap<String, String>,
    components: Vec<BundleComponent>,
    #[serde(default)]
    runtime_requirements: BTreeMap<String, String>,
    created_by: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
struct BundleComponent {
    component_key: String,
    package_name: String,
    package_version: String,
    #[serde(default)]
    registry_integrity: String,
}

pub(super) struct ValidatedBundle {
    entrypoints: BTreeMap<String, PathBuf>,
}

impl ValidatedBundle {
    pub(super) fn entrypoint(&self, command: &str) -> Option<&Path> {
        self.entrypoints.get(command).map(PathBuf::as_path)
    }
}

pub(super) fn validate_bundle_manifest(
    root: &Path,
    offer: &AgentOffer,
    required_commands: &[&str],
) -> Result<ValidatedBundle, AcpError> {
    let manifest = read_manifest(root)?;
    validate_identity(&manifest, offer)?;
    validate_components(&manifest, offer)?;
    validate_runtime_requirements(&manifest, offer)?;
    let entrypoints = validate_entrypoints(root, &manifest.entrypoints, required_commands)?;
    Ok(ValidatedBundle { entrypoints })
}

fn read_manifest(root: &Path) -> Result<BundleManifest, AcpError> {
    let path = root.join(BUNDLE_MANIFEST_NAME);
    let metadata = std::fs::metadata(&path).map_err(|error| {
        AcpError::DownloadFailed(format!("Agent bundle manifest is unavailable: {error}"))
    })?;
    if metadata.len() > MAX_BUNDLE_MANIFEST_BYTES {
        return Err(AcpError::DownloadFailed(
            "Agent bundle manifest is too large".into(),
        ));
    }
    let bytes = std::fs::read(&path).map_err(|error| {
        AcpError::DownloadFailed(format!("Agent bundle manifest is unavailable: {error}"))
    })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        AcpError::DownloadFailed(format!("Agent bundle manifest is invalid: {error}"))
    })
}

fn validate_identity(manifest: &BundleManifest, offer: &AgentOffer) -> Result<(), AcpError> {
    let valid = manifest.schema_version == BUNDLE_SCHEMA_VERSION
        && manifest.created_by == BUNDLE_BUILDER
        && manifest.registry_id == offer.registry_id
        && manifest.version == offer.version
        && manifest.delivery_kind == offer.delivery.kind
        && manifest.target == offer.delivery.target
        && manifest.arch == offer.delivery.arch;
    valid.then_some(()).ok_or_else(|| {
        AcpError::DownloadFailed("Agent bundle identity does not match the resolved offer".into())
    })
}

fn validate_components(manifest: &BundleManifest, offer: &AgentOffer) -> Result<(), AcpError> {
    let mut actual = manifest.components.iter().collect::<Vec<_>>();
    actual.sort();
    let mut expected = offer
        .delivery
        .components
        .iter()
        .map(|value| BundleComponent {
            component_key: value.component_key.clone(),
            package_name: value.package_name.clone(),
            package_version: value.package_version.clone(),
            registry_integrity: value.registry_integrity.clone(),
        })
        .collect::<Vec<_>>();
    expected.sort();
    (actual.into_iter().cloned().collect::<Vec<_>>() == expected)
        .then_some(())
        .ok_or_else(|| {
            AcpError::DownloadFailed(
                "Agent bundle components do not match the resolved offer".into(),
            )
        })
}

fn validate_runtime_requirements(
    manifest: &BundleManifest,
    offer: &AgentOffer,
) -> Result<(), AcpError> {
    let expected = expected_runtime_requirements(offer);
    let required_keys: &[&str] = match offer.delivery.kind.as_str() {
        "npm" => &["node"],
        "uvx" => &["uv", "python"],
        _ => {
            return Err(AcpError::DownloadFailed(
                "Agent bundle kind is unsupported".into(),
            ))
        }
    };
    let required = required_keys
        .iter()
        .all(|key| expected.get(*key).is_some_and(|value| !value.is_empty()));
    (required && manifest.runtime_requirements == expected)
        .then_some(())
        .ok_or_else(|| {
            AcpError::DownloadFailed(
                "Agent bundle runtime requirements do not match the resolved offer".into(),
            )
        })
}

fn expected_runtime_requirements(offer: &AgentOffer) -> BTreeMap<String, String> {
    [
        ("node", offer.delivery.node_required.trim()),
        ("uv", offer.delivery.uv_required.trim()),
        ("python", offer.delivery.python_required.trim()),
    ]
    .into_iter()
    .filter(|(_, value)| !value.is_empty())
    .map(|(key, value)| (key.to_string(), value.to_string()))
    .collect()
}

fn validate_entrypoints(
    root: &Path,
    values: &BTreeMap<String, String>,
    required_commands: &[&str],
) -> Result<BTreeMap<String, PathBuf>, AcpError> {
    if values.len() != required_commands.len() {
        return Err(AcpError::DownloadFailed(
            "Agent bundle entrypoint set does not match the compiled capability".into(),
        ));
    }
    let mut result = BTreeMap::new();
    for command in required_commands {
        let relative = values.get(*command).ok_or_else(|| {
            AcpError::DownloadFailed(format!("Agent bundle entrypoint is missing: {command}"))
        })?;
        let relative = Path::new(relative);
        if !safe_relative_path(relative) {
            return Err(AcpError::DownloadFailed(
                "Agent bundle entrypoint path is unsafe".into(),
            ));
        }
        let path = root.join(relative);
        if !path.is_file() {
            return Err(AcpError::DownloadFailed(format!(
                "Agent bundle entrypoint is unavailable: {command}"
            )));
        }
        result.insert((*command).to_string(), path);
    }
    Ok(result)
}

fn safe_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|item| matches!(item, Component::Normal(_)))
}
