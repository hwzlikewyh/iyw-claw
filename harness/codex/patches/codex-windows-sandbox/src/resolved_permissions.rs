use anyhow::Result;
use codex_protocol::models::PermissionProfile;
use codex_protocol::permissions::FileSystemPath;
use codex_protocol::permissions::FileSystemSandboxEntry;
use codex_protocol::permissions::FileSystemSandboxKind;
use codex_protocol::permissions::FileSystemSandboxPolicy;
use codex_protocol::permissions::FileSystemSpecialPath::Root;
use codex_protocol::permissions::NetworkSandboxPolicy;
use codex_protocol::permissions::ReadDenyMatcher;
use codex_utils_absolute_path::AbsolutePathBuf;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

/// Windows-local view of the runtime permission profile.
///
/// Most Windows sandbox code needs resolved runtime permissions plus a few
/// Windows-specific path conventions, not the user/config-facing
/// `PermissionProfile` enum itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedWindowsSandboxPermissions {
    file_system: FileSystemSandboxPolicy,
    network: NetworkSandboxPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WindowsWritableRoot {
    pub(crate) root: PathBuf,
    pub(crate) read_only_subpaths: Vec<PathBuf>,
}

/// Restricted-token family needed to enforce a Windows permission profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsSandboxTokenMode {
    ReadOnlyCapability,
    WritableRootsCapability,
}

/// Chooses the restricted-token family needed for a managed permission profile.
pub fn token_mode_for_permission_profile(
    permission_profile: &PermissionProfile,
    workspace_roots: &[AbsolutePathBuf],
    cwd: &Path,
    env_map: &HashMap<String, String>,
) -> Result<WindowsSandboxTokenMode> {
    let permissions =
        ResolvedWindowsSandboxPermissions::try_from_permission_profile_for_workspace_roots(
            permission_profile,
            workspace_roots,
        )?;
    if permissions.file_system.has_full_disk_write_access() {
        anyhow::bail!(
            "permission profile requests full-disk filesystem writes, which cannot be enforced by the Windows sandbox"
        );
    }
    if permissions.writable_roots_for_cwd(cwd, env_map).is_empty() {
        Ok(WindowsSandboxTokenMode::ReadOnlyCapability)
    } else {
        Ok(WindowsSandboxTokenMode::WritableRootsCapability)
    }
}

impl ResolvedWindowsSandboxPermissions {
    pub fn try_from_permission_profile(permission_profile: &PermissionProfile) -> Result<Self> {
        if !matches!(permission_profile, PermissionProfile::Managed { .. }) {
            anyhow::bail!(
                "only managed permission profiles can be enforced by the Windows sandbox"
            );
        }
        let (file_system, network) = permission_profile.to_runtime_permissions();
        if !matches!(file_system.kind, FileSystemSandboxKind::Restricted) {
            anyhow::bail!(
                "only restricted managed filesystem permissions can be enforced by the Windows sandbox"
            );
        }
        Ok(Self {
            file_system,
            network,
        })
    }

    /// Resolves a managed permission profile and binds symbolic `:workspace_roots`
    /// entries to the workspace roots supplied by the caller.
    pub fn try_from_permission_profile_for_workspace_roots(
        permission_profile: &PermissionProfile,
        workspace_roots: &[AbsolutePathBuf],
    ) -> Result<Self> {
        let mut permissions = Self::try_from_permission_profile(permission_profile)?;
        permissions.file_system = permissions
            .file_system
            .materialize_project_roots_with_workspace_roots(workspace_roots);
        Ok(permissions)
    }

    pub(crate) fn should_apply_network_block(&self) -> bool {
        !self.network.is_enabled()
    }

    pub(crate) fn network_policy(&self) -> NetworkSandboxPolicy {
        self.network
    }

    /// Rejects filesystem policies that the elevated Windows sandbox cannot
    /// enforce safely.
    pub fn validate_elevated_filesystem_policy(&self, cwd: &Path) -> Result<()> {
        let root = cwd
            .ancestors()
            .last()
            .ok_or_else(|| anyhow::anyhow!("command cwd has no filesystem root"))?;
        let root_is_denied = ReadDenyMatcher::try_new_for_local_paths(&self.file_system, cwd)
            .map_err(anyhow::Error::msg)?
            .is_some_and(|matcher| matcher.is_local_path_read_denied(root));
        if !self.file_system.can_read_local_path_with_cwd(root, cwd) || root_is_denied {
            anyhow::bail!("elevated Windows sandbox requires effective `:root` read access");
        }
        Ok(())
    }

    pub(crate) fn has_full_disk_read_access(&self) -> bool {
        self.file_system.has_full_disk_read_access()
    }

    pub(crate) fn has_symbolic_root_read_access(&self, cwd: &Path) -> bool {
        self.file_system.entries.iter().any(|entry| {
            matches!(&entry.path, FileSystemPath::Special { value: Root })
                && entry.access.can_read()
        }) && cwd
            .ancestors()
            .last()
            .is_some_and(|root| self.file_system.can_read_local_path_with_cwd(root, cwd))
    }

    pub(crate) fn include_platform_defaults(&self) -> bool {
        self.file_system.include_platform_defaults()
    }

    pub(crate) fn readable_roots_for_cwd(&self, cwd: &Path) -> Vec<PathBuf> {
        self.file_system
            .get_readable_roots_with_cwd(cwd)
            .into_iter()
            .map(AbsolutePathBuf::into_path_buf)
            .collect()
    }

    pub(crate) fn uses_write_capabilities_for_cwd(
        &self,
        cwd: &Path,
        env_map: &HashMap<String, String>,
    ) -> bool {
        !self.writable_roots_for_cwd(cwd, env_map).is_empty()
    }

    pub(crate) fn writable_roots_for_cwd(
        &self,
        cwd: &Path,
        env_map: &HashMap<String, String>,
    ) -> Vec<WindowsWritableRoot> {
        let mut file_system = self.file_system.clone();
        resolve_workload_temp_paths(&mut file_system, env_map);
        file_system
            .get_writable_roots_with_cwd(cwd)
            .into_iter()
            .map(|root| WindowsWritableRoot {
                root: root.root.into_path_buf(),
                read_only_subpaths: root
                    .read_only_subpaths
                    .into_iter()
                    .map(AbsolutePathBuf::into_path_buf)
                    .collect(),
            })
            .collect()
    }
}

/// Replace `:tmpdir` with absolute TEMP/TMP paths from the completed Windows
/// workload environment. Use the same key order as the child's environment
/// block; never use host TEMP/TMP. Preserve access and missing-path rules so
/// the ordinary policy evaluator still handles read/deny carveouts.
pub fn resolve_workload_temp_paths(
    file_system: &mut FileSystemSandboxPolicy,
    workload_env: &HashMap<String, String>,
) {
    use codex_protocol::permissions::FileSystemSpecialPath;

    file_system.entries = std::mem::take(&mut file_system.entries)
        .into_iter()
        .flat_map(|entry| match &entry.path {
            FileSystemPath::Special {
                value: FileSystemSpecialPath::Tmpdir,
            } => crate::process::ordered_env_entries(workload_env)
                .into_iter()
                .filter(|(key, value)| {
                    (key.eq_ignore_ascii_case("TEMP") || key.eq_ignore_ascii_case("TMP"))
                        && Path::new(value).is_absolute()
                })
                .filter_map(|(_, value)| AbsolutePathBuf::from_absolute_path(value).ok())
                .map(|path| FileSystemSandboxEntry {
                    path: path.into(),
                    ..entry.clone()
                })
                .collect(),
            FileSystemPath::Special {
                value: FileSystemSpecialPath::SlashTmp,
            } => Vec::new(),
            _ => vec![entry],
        })
        .collect();
}
