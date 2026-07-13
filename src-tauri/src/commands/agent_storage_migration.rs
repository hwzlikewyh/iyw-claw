use std::fs;
use std::path::{Path, PathBuf};

use sea_orm::DatabaseConnection;

use crate::acp::agent_storage::{
    load_config, save_config, validate_root, AgentStorageConfig, AgentStoragePaths,
};
use crate::acp::registry::{self, AgentDistribution};
use crate::app_error::AppCommandError;
use crate::db::service::agent_setting_service;
use crate::models::agent::AgentType;

use super::agent_storage::{build_status, AgentStorageStatus};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MigrationActivity {
    pub active_connections: bool,
    pub active_installs: bool,
}

pub async fn migrate_agent_storage_core(
    conn: &DatabaseConnection,
    destination: PathBuf,
    allow_system_drive: bool,
    system_drive: Option<&str>,
    activity: MigrationActivity,
) -> Result<AgentStorageStatus, AppCommandError> {
    let installed = installed_agents(conn).await?;
    migrate_agent_storage_with_verifier_core(
        conn,
        destination,
        allow_system_drive,
        system_drive,
        activity,
        move |paths| verify_migrated_storage(paths, &installed),
    )
    .await
}

pub(crate) async fn migrate_agent_storage_with_verifier_core(
    conn: &DatabaseConnection,
    destination: PathBuf,
    allow_system_drive: bool,
    system_drive: Option<&str>,
    activity: MigrationActivity,
    verifier: impl FnOnce(&AgentStoragePaths) -> Result<(), String>,
) -> Result<AgentStorageStatus, AppCommandError> {
    if activity.active_connections || activity.active_installs {
        return Err(AppCommandError::task_execution_failed(
            "Stop active Agent sessions and installation tasks before migrating storage",
        ));
    }
    let mut config = load_config(conn)
        .await
        .map_err(|error| AppCommandError::agent_storage_invalid(error.to_string()))?
        .ok_or_else(|| {
            AppCommandError::agent_storage_not_initialized("Agent storage is not initialized")
        })?;
    let source = config
        .root
        .clone()
        .filter(|_| config.initialized)
        .ok_or_else(|| {
            AppCommandError::agent_storage_not_initialized("Agent storage is not initialized")
        })?;
    if destination.is_absolute() {
        validate_migration_paths(&source, &destination)?;
    }
    let destination_existed = destination.exists();
    let validation = validate_root(&destination, system_drive);
    if !validation.writable {
        return Err(AppCommandError::agent_storage_invalid(
            "Agent storage migration destination is not writable",
        )
        .with_detail(validation.error.unwrap_or_default()));
    }
    if validation.on_system_drive && !allow_system_drive {
        if !destination_existed {
            remove_empty_directory(&validation.absolute_path);
        }
        return Err(AppCommandError::permission_denied(
            "Agent storage on the system drive requires explicit confirmation",
        ));
    }
    let destination = validation.absolute_path;
    ensure_empty_destination(&destination)?;
    let staging = migration_staging_path(&destination)?;
    let result = migrate_via_staging(&source, &destination, &staging, verifier);
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&staging);
        if !destination_existed {
            remove_empty_directory(&destination);
        }
        return Err(
            AppCommandError::task_execution_failed("Agent storage migration failed")
                .with_detail(error),
        );
    }

    rebase_profile_overrides(&mut config, &source, &destination);
    config.root = Some(destination.clone());
    config.allow_system_drive |= allow_system_drive;
    if let Err(error) = save_config(conn, &config).await {
        let _ = fs::remove_dir_all(&destination);
        if destination_existed {
            let _ = fs::create_dir_all(&destination);
        }
        return Err(AppCommandError::agent_storage_invalid(
            "Failed to persist migrated Agent storage",
        )
        .with_detail(error.to_string()));
    }
    let mut status = build_status(Some(&config), Some(destination), None, true, true);
    status.previous_root = Some(source);
    Ok(status)
}

fn migrate_via_staging(
    source: &Path,
    destination: &Path,
    staging: &Path,
    verifier: impl FnOnce(&AgentStoragePaths) -> Result<(), String>,
) -> Result<(), String> {
    fs::create_dir_all(staging).map_err(|error| error.to_string())?;
    for name in ["runtime", "config", "downloads"] {
        let source_entry = source.join(name);
        if source_entry.exists() {
            copy_tree(&source_entry, &staging.join(name))?;
        }
    }
    fs::create_dir_all(staging.join("staging")).map_err(|error| error.to_string())?;
    fs::create_dir_all(staging.join("trash")).map_err(|error| error.to_string())?;
    verifier(&AgentStoragePaths::new(staging.to_path_buf()))?;
    fs::remove_dir(destination).map_err(|error| error.to_string())?;
    fs::rename(staging, destination).map_err(|error| error.to_string())
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(source).map_err(|error| error.to_string())?;
    if is_unsafe_link(&metadata) {
        return Ok(());
    }
    if metadata.is_file() {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::copy(source, destination).map_err(|error| error.to_string())?;
        fs::set_permissions(destination, metadata.permissions())
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        copy_tree(&entry.path(), &destination.join(entry.file_name()))?;
    }
    Ok(())
}

fn verify_migrated_storage(
    paths: &AgentStoragePaths,
    installed: &[(AgentType, String)],
) -> Result<(), String> {
    for agent in registry::all_acp_agents() {
        if !paths.profile(agent).root.is_dir() {
            return Err(format!("missing migrated profile for {agent:?}"));
        }
    }
    for (agent, version) in installed {
        let meta = registry::get_agent_meta(*agent);
        let valid = match meta.distribution {
            AgentDistribution::Npx { cmd, .. } => {
                crate::acp::npm_runtime::resolve_private_npm_command(paths, *agent, version, cmd)
                    .is_some()
            }
            AgentDistribution::Binary { cmd, .. } => {
                crate::acp::binary_cache::find_cached_binary_for_agent(paths, *agent, version, cmd)
                    .map_err(|error| error.to_string())?
                    .is_some()
            }
            AgentDistribution::Uvx { .. } => {
                crate::acp::binary_cache::uvx_prepared_version(paths, *agent).as_deref()
                    == Some(version.as_str())
                    && crate::acp::binary_cache::find_cached_uv_tool(paths, "uvx").is_some()
            }
        };
        if !valid {
            return Err(format!("missing migrated runtime for {agent:?} {version}"));
        }
    }
    Ok(())
}

async fn installed_agents(
    conn: &DatabaseConnection,
) -> Result<Vec<(AgentType, String)>, AppCommandError> {
    let rows = agent_setting_service::list(conn)
        .await
        .map_err(|error| AppCommandError::agent_storage_invalid(error.to_string()))?;
    rows.into_iter()
        .filter_map(|row| {
            row.installed_version
                .map(|version| (row.agent_type, version))
        })
        .map(|(raw_agent, version)| {
            serde_json::from_str::<AgentType>(&raw_agent)
                .map(|agent| (agent, version))
                .map_err(|error| AppCommandError::agent_storage_invalid(error.to_string()))
        })
        .collect()
}

fn validate_migration_paths(source: &Path, destination: &Path) -> Result<(), AppCommandError> {
    if same_path(source, destination)
        || path_starts_with(source, destination)
        || path_starts_with(destination, source)
    {
        return Err(AppCommandError::invalid_input(
            "Migration destination must not overlap the current Agent storage root",
        ));
    }
    Ok(())
}

fn ensure_empty_destination(path: &Path) -> Result<(), AppCommandError> {
    let mut entries = fs::read_dir(path).map_err(AppCommandError::io)?;
    if entries.next().is_some() {
        return Err(AppCommandError::invalid_input(
            "Migration destination must be empty",
        ));
    }
    Ok(())
}

fn migration_staging_path(destination: &Path) -> Result<PathBuf, AppCommandError> {
    let parent = destination
        .parent()
        .ok_or_else(|| AppCommandError::invalid_input("Migration destination has no parent"))?;
    let name = destination
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("agent-storage");
    Ok(parent.join(format!(
        ".{name}.iyw-claw-migration.{}",
        uuid::Uuid::new_v4()
    )))
}

fn rebase_profile_overrides(config: &mut AgentStorageConfig, source: &Path, destination: &Path) {
    for path in config.profile_overrides.values_mut() {
        if let Ok(relative) = path.strip_prefix(source) {
            *path = destination.join(relative);
        }
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    comparable_path(left) == comparable_path(right)
}

fn path_starts_with(path: &Path, base: &Path) -> bool {
    comparable_path(path).starts_with(&(comparable_path(base) + "/"))
}

fn comparable_path(path: &Path) -> String {
    let value = path
        .to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();
    if cfg!(windows) {
        value.to_lowercase()
    } else {
        value
    }
}

fn remove_empty_directory(path: &Path) {
    if path.is_dir()
        && fs::read_dir(path)
            .ok()
            .is_some_and(|mut entries| entries.next().is_none())
    {
        let _ = fs::remove_dir(path);
    }
}

#[cfg(windows)]
fn is_unsafe_link(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_type().is_symlink() || metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_unsafe_link(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}
