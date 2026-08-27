use std::path::Path;

use serde::Deserialize;

use crate::db::service::plugin_installation_service::PluginInstallationRecord;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurrentPointer {
    version: String,
    content_sha256: String,
    object_sha256: String,
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
    let Some(plugin_root) = version_root.parent().and_then(Path::parent) else {
        return false;
    };
    let pointer = std::fs::read(plugin_root.join("current.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<CurrentPointer>(&bytes).ok());
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
