use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::db::service::plugin_installation_service::PluginInstallationRecord;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CurrentPointer {
    pub version: String,
    pub content_sha256: String,
    pub object_sha256: String,
    #[serde(default)]
    pub market: Option<crate::commands::acp::MarketSkillMarker>,
    #[serde(default)]
    pub plugin_dependencies: Vec<i64>,
    #[serde(default)]
    pub account_scope: String,
}

pub(super) fn scope_matches(record: &PluginInstallationRecord, scope: Option<&str>) -> bool {
    read_pointer(Path::new(&record.installation.install_root)).is_some_and(|pointer| {
        !pointer.account_scope.is_empty() && Some(pointer.account_scope.as_str()) == scope
    })
}

const MAX_POINTER_BYTES: u64 = 128 * 1024;

pub(crate) fn read_pointer(version_root: &Path) -> Option<CurrentPointer> {
    let path = version_root.parent()?.parent()?.join("current.json");
    if std::fs::metadata(&path).ok()?.len() > MAX_POINTER_BYTES {
        return None;
    }
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

pub(super) fn valid_current_pointer(record: &PluginInstallationRecord) -> bool {
    let version_root = Path::new(&record.installation.install_root);
    let expected_root = crate::acp::agent_storage::AgentStoragePaths::active().map(|paths| {
        paths
            .plugins_dir()
            .join(&record.installation.slug)
            .join("versions")
            .join(&record.installation.version)
    });
    if expected_root.as_deref() != Some(version_root) {
        return false;
    }
    let pointer = read_pointer(version_root);
    version_root.is_dir()
        && pointer.is_some_and(|value| {
            value.version == record.installation.version
                && value.content_sha256 == record.installation.content_sha256
                && value.object_sha256 == record.installation.object_sha256
        })
}

pub(super) fn log_recovery_artifacts() {
    let Some(paths) = crate::acp::agent_storage::AgentStoragePaths::active() else {
        return;
    };
    for (kind, root) in [
        ("staging", paths.staging_dir().join("plugins")),
        ("trash", paths.trash_dir().join("plugins")),
    ] {
        let count = std::fs::read_dir(&root)
            .ok()
            .map(|entries| entries.filter_map(Result::ok).count())
            .unwrap_or_default();
        if count > 0 {
            tracing::warn!(
                kind,
                count,
                path = %root.display(),
                "[plugin-registry] recovery artifacts retained"
            );
        }
    }
}
