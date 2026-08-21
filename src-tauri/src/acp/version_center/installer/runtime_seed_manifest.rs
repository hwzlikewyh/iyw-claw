use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::runtime::platform_dir_name;
use crate::acp::version_center::capability;
use crate::app_error::AppCommandError;

const SCHEMA_VERSION: u32 = 2;
const CREATED_BY: &str = "iyw-runtime-seed-builder";
const MAX_MANIFEST_BYTES: u64 = 32 * 1024 * 1024;
const REQUIRED_COMPONENTS: [(&str, &str); 4] = [
    ("node", "runtime_tool"),
    ("git", "runtime_tool"),
    ("uv", "runtime_tool"),
    ("codex-acp", "npm_agent"),
];

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RuntimeSeedManifest {
    schema_version: u32,
    created_by: String,
    app_version: String,
    target: String,
    arch: String,
    platform: String,
    pub(super) components: Vec<RuntimeSeedComponent>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RuntimeSeedComponent {
    pub(super) id: String,
    pub(super) kind: String,
    pub(super) version: String,
    pub(super) archive: String,
    pub(super) archive_sha256: String,
    pub(super) archive_size: u64,
    pub(super) sha256: String,
    pub(super) total_size: u64,
    pub(super) entrypoints: BTreeMap<String, String>,
    pub(super) files: Vec<RuntimeSeedFile>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RuntimeSeedFile {
    pub(super) path: String,
    pub(super) size: u64,
    pub(super) sha256: String,
    #[serde(default)]
    pub(super) executable: bool,
}

impl RuntimeSeedManifest {
    pub(super) fn read(seed_root: &Path) -> Result<Self, AppCommandError> {
        let path = seed_root.join("manifest.json");
        let metadata = std::fs::symlink_metadata(&path).map_err(AppCommandError::io)?;
        if !metadata.file_type().is_file() || metadata.len() > MAX_MANIFEST_BYTES {
            return Err(invalid("Runtime seed manifest is too large"));
        }
        let raw = std::fs::read(path).map_err(AppCommandError::io)?;
        let mut manifest: Self = serde_json::from_slice(&raw).map_err(|error| {
            invalid("Runtime seed manifest is invalid").with_detail(error.to_string())
        })?;
        manifest.normalize_paths()?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub(super) fn component(&self, id: &str) -> Option<&RuntimeSeedComponent> {
        self.components.iter().find(|item| item.id == id)
    }

    fn normalize_paths(&mut self) -> Result<(), AppCommandError> {
        for component in &mut self.components {
            component.archive = normalize_relative_path(&component.archive)?;
            for path in component.entrypoints.values_mut() {
                *path = normalize_relative_path(path)?;
            }
            for file in &mut component.files {
                file.path = normalize_relative_path(&file.path)?;
            }
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), AppCommandError> {
        self.validate_identity()?;
        let mut ids = BTreeSet::new();
        let mut archives = BTreeSet::new();
        for component in &self.components {
            component.validate()?;
            if !ids.insert(component.id.as_str()) || !archives.insert(component.archive.as_str()) {
                return Err(invalid("Runtime seed contains duplicate components"));
            }
        }
        let complete = REQUIRED_COMPONENTS
            .iter()
            .all(|(id, kind)| self.component(id).is_some_and(|item| item.kind == *kind));
        complete
            .then_some(())
            .ok_or_else(|| invalid("Runtime seed component set is incomplete"))
    }

    fn validate_identity(&self) -> Result<(), AppCommandError> {
        let expected_version = env!("CARGO_PKG_VERSION");
        let expected_target = capability::current_target_triple();
        let expected_arch = capability::current_arch();
        let expected_platform = platform_dir_name();
        let matches = self.schema_version == SCHEMA_VERSION
            && self.created_by == CREATED_BY
            && self.app_version == expected_version
            && self.target == expected_target
            && self.arch == expected_arch
            && self.platform == expected_platform;
        if !matches {
            tracing::warn!(
                expected_version,
                manifest_version = %self.app_version,
                expected_target,
                manifest_target = %self.target,
                expected_arch,
                manifest_arch = %self.arch,
                expected_platform,
                manifest_platform = %self.platform,
                "[runtime-seed] manifest identity mismatch"
            );
        }
        matches.then_some(()).ok_or_else(|| {
            invalid("Runtime seed identity does not match this application").with_detail(format!(
                "expected_version={expected_version}; manifest_version={}; expected_target={expected_target}; manifest_target={}; expected_arch={expected_arch}; manifest_arch={}; expected_platform={expected_platform}; manifest_platform={}",
                self.app_version, self.target, self.arch, self.platform
            ))
        })
    }
}

impl RuntimeSeedComponent {
    fn validate(&self) -> Result<(), AppCommandError> {
        semver::Version::parse(&self.version)
            .map_err(|_| invalid("Runtime seed component version is invalid"))?;
        if !valid_sha256(&self.sha256)
            || !valid_sha256(&self.archive_sha256)
            || self.archive_size == 0
            || !valid_relative_path(&self.archive)
        {
            return Err(invalid("Runtime seed component metadata is invalid"));
        }
        if !self.archive.starts_with("components/")
            || !self.archive.ends_with(".tar.gz")
            || self.files.is_empty()
        {
            return Err(invalid("Runtime seed component root is invalid"));
        }
        let mut paths = BTreeSet::new();
        let mut total_size = 0_u64;
        for file in &self.files {
            if !valid_relative_path(&file.path)
                || !valid_sha256(&file.sha256)
                || !paths.insert(file.path.as_str())
            {
                return Err(invalid("Runtime seed file metadata is invalid"));
            }
            total_size = total_size
                .checked_add(file.size)
                .ok_or_else(|| invalid("Runtime seed component size overflow"))?;
        }
        if total_size != self.total_size {
            return Err(invalid(
                "Runtime seed component size does not match its files",
            ));
        }
        if !component_digest(&self.files).eq_ignore_ascii_case(&self.sha256) {
            return Err(invalid("Runtime seed component SHA-256 mismatch"));
        }
        self.validate_entrypoints(&paths)
    }

    fn validate_entrypoints(&self, files: &BTreeSet<&str>) -> Result<(), AppCommandError> {
        let expected: &[&str] = match self.id.as_str() {
            "node" => &["node", "npm"],
            "git" => &["git"],
            "uv" => &["uv", "uvx"],
            "codex-acp" => &["codex-acp"],
            _ => return Err(invalid("Runtime seed component is unsupported")),
        };
        let matches = self.entrypoints.len() == expected.len()
            && expected.iter().all(|key| {
                self.entrypoints
                    .get(*key)
                    .is_some_and(|path| valid_relative_path(path) && files.contains(path.as_str()))
            });
        matches
            .then_some(())
            .ok_or_else(|| invalid("Runtime seed entrypoint set is invalid"))
    }

    pub(super) fn source_archive(&self, seed_root: &Path) -> PathBuf {
        seed_root.join(&self.archive)
    }
}

fn component_digest(files: &[RuntimeSeedFile]) -> String {
    let mut hasher = Sha256::new();
    for file in files {
        hasher.update(file.path.as_bytes());
        hasher.update([0]);
        hasher.update(file.size.to_string().as_bytes());
        hasher.update([0]);
        hasher.update(file.sha256.as_bytes());
        hasher.update(b"\n");
    }
    format!("{:x}", hasher.finalize())
}

fn valid_relative_path(value: &str) -> bool {
    normalize_relative_path(value).is_ok()
}

fn normalize_relative_path(value: &str) -> Result<String, AppCommandError> {
    let value = value.replace('\\', "/");
    let windows_absolute = value.as_bytes().get(1).is_some_and(|byte| *byte == b':');
    let valid = !value.is_empty()
        && !value.starts_with('/')
        && !windows_absolute
        && !value.chars().any(char::is_control)
        && value
            .split('/')
            .all(|part| !part.is_empty() && !matches!(part, "." | ".."));
    valid
        .then_some(value)
        .ok_or_else(|| invalid("Runtime seed path is invalid"))
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn invalid(message: &str) -> AppCommandError {
    AppCommandError::invalid_input(message)
}
