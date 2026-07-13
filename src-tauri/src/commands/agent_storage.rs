use std::ffi::OsString;
use std::path::{Path, PathBuf};

use sea_orm::DatabaseConnection;
use serde::Serialize;

use crate::acp::agent_storage::{
    load_config, resolve_root, save_config, startup_profile_env_matches, suggest_desktop_root,
    validate_root, AgentStorageConfig, AgentStorageError, AgentStoragePaths,
};
use crate::acp::profile_import::{
    import_existing_profiles, ProfileImportError, ProfileSourceRoots, PROFILE_IMPORT_VERSION,
};
use crate::acp::registry;
use crate::app_error::AppCommandError;
use crate::models::agent::AgentType;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentProfileStatus {
    pub agent_type: AgentType,
    pub path: PathBuf,
    pub overridden: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentStorageStatus {
    pub initialized: bool,
    pub active_root: Option<PathBuf>,
    pub suggested_root: Option<PathBuf>,
    pub allow_system_drive: bool,
    pub restart_required: bool,
    pub profile_paths: Vec<AgentProfileStatus>,
    pub previous_root: Option<PathBuf>,
}

pub async fn get_agent_storage_status_core(
    conn: &DatabaseConnection,
    executable_path: &Path,
    env_override: Option<OsString>,
    server_fallback: Option<PathBuf>,
) -> Result<AgentStorageStatus, AppCommandError> {
    let persisted = load_config(conn).await.map_err(map_storage_error)?;
    let env_root = env_override
        .as_ref()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let active_root = resolve_root(env_override.clone(), persisted.as_ref(), server_fallback);
    let initialized = env_override.is_some()
        || persisted
            .as_ref()
            .is_some_and(|config| config.initialized && config.root.is_some());
    let restart_required = persisted.as_ref().is_some_and(|config| {
        if !config.initialized || config.root.as_ref() != env_root.as_ref() {
            return config.initialized;
        }
        let Some(root) = active_root.as_ref() else {
            return true;
        };
        !startup_profile_env_matches(&AgentStoragePaths::new(root.clone()), config, |key| {
            std::env::var_os(key)
        })
    });
    Ok(build_status(
        persisted.as_ref(),
        active_root,
        suggest_desktop_root(executable_path),
        initialized,
        restart_required,
    ))
}

pub async fn initialize_agent_storage_core(
    conn: &DatabaseConnection,
    root: PathBuf,
    allow_system_drive: bool,
    system_drive: Option<&str>,
    import_existing_settings: bool,
) -> Result<AgentStorageStatus, AppCommandError> {
    let sources = if import_existing_settings {
        Some(ProfileSourceRoots::discover().map_err(map_profile_import_error)?)
    } else {
        None
    };
    initialize_agent_storage_with_sources_core(
        conn,
        root,
        allow_system_drive,
        system_drive,
        sources.as_ref(),
    )
    .await
}

pub(crate) async fn initialize_agent_storage_with_sources_core(
    conn: &DatabaseConnection,
    root: PathBuf,
    allow_system_drive: bool,
    system_drive: Option<&str>,
    sources: Option<&ProfileSourceRoots>,
) -> Result<AgentStorageStatus, AppCommandError> {
    let validation = validate_root(&root, system_drive);
    if !validation.writable {
        return Err(AppCommandError::agent_storage_invalid(
            "Agent storage directory is not writable",
        )
        .with_detail(validation.error.unwrap_or_default()));
    }
    if validation.on_system_drive && !allow_system_drive {
        return Err(AppCommandError::permission_denied(
            "Agent storage on the system drive requires explicit confirmation",
        ));
    }
    let paths = AgentStoragePaths::new(validation.absolute_path.clone());
    create_storage_layout(&paths)?;
    let previous = load_config(conn).await.map_err(map_storage_error)?;
    let mut config = previous
        .clone()
        .unwrap_or_else(|| AgentStorageConfig::confirmed(validation.absolute_path.clone()));
    config.root = Some(validation.absolute_path.clone());
    config.initialized = true;
    config.allow_system_drive |= allow_system_drive;
    if let Some(sources) = sources.filter(|_| config.import_version < PROFILE_IMPORT_VERSION) {
        import_existing_profiles(&paths, sources).map_err(map_profile_import_error)?;
        config.import_version = PROFILE_IMPORT_VERSION;
    }
    save_config(conn, &config)
        .await
        .map_err(map_storage_error)?;
    Ok(build_status(
        Some(&config),
        Some(validation.absolute_path),
        None,
        true,
        true,
    ))
}

pub async fn update_agent_profile_override_core(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    override_path: Option<PathBuf>,
    allow_system_drive: bool,
    allow_user_global_profile: bool,
    system_drive: Option<&str>,
) -> Result<AgentStorageStatus, AppCommandError> {
    let mut config = load_config(conn)
        .await
        .map_err(map_storage_error)?
        .ok_or_else(|| {
            AppCommandError::agent_storage_not_initialized("Agent storage is not initialized")
        })?;
    let root = config
        .root
        .clone()
        .ok_or_else(|| AppCommandError::agent_storage_invalid("Agent storage root is missing"))?;

    if let Some(path) = override_path {
        let home = dirs::home_dir().unwrap_or_default();
        let xdg_config = home.join(".config");
        if !allow_user_global_profile
            && is_user_global_profile_path(agent_type, &path, &home, &xdg_config)
        {
            return Err(AppCommandError::permission_denied(
                "Using the existing user-global Agent profile requires explicit confirmation",
            ));
        }
        let validation = validate_root(&path, system_drive);
        if !validation.writable {
            return Err(AppCommandError::agent_storage_invalid(
                "Agent profile directory is not writable",
            )
            .with_detail(validation.error.unwrap_or_default()));
        }
        if validation.on_system_drive && !allow_system_drive {
            return Err(AppCommandError::permission_denied(
                "Agent profile on the system drive requires explicit confirmation",
            ));
        }
        config.profile_overrides.insert(
            registry::registry_id_for(agent_type).to_string(),
            validation.absolute_path,
        );
    } else {
        config
            .profile_overrides
            .remove(registry::registry_id_for(agent_type));
    }
    config.allow_system_drive |= allow_system_drive;
    save_config(conn, &config)
        .await
        .map_err(map_storage_error)?;
    Ok(build_status(Some(&config), Some(root), None, true, true))
}

fn create_storage_layout(paths: &AgentStoragePaths) -> Result<(), AppCommandError> {
    for dir in [
        paths.runtime_dir(),
        paths.config_dir(),
        paths.downloads_dir(),
        paths.staging_dir(),
        paths.trash_dir(),
    ] {
        std::fs::create_dir_all(&dir).map_err(AppCommandError::io)?;
    }
    for agent_type in registry::all_acp_agents() {
        let profile = paths.profile(agent_type);
        std::fs::create_dir_all(&profile.root).map_err(AppCommandError::io)?;
        for dir in profile.env.values() {
            std::fs::create_dir_all(dir).map_err(AppCommandError::io)?;
        }
    }
    Ok(())
}

pub(super) fn build_status(
    config: Option<&AgentStorageConfig>,
    active_root: Option<PathBuf>,
    suggested_root: Option<PathBuf>,
    initialized: bool,
    restart_required: bool,
) -> AgentStorageStatus {
    let profile_paths = active_root
        .as_ref()
        .map(|root| profile_statuses(config, root))
        .unwrap_or_default();
    AgentStorageStatus {
        initialized,
        active_root,
        suggested_root,
        allow_system_drive: config.is_some_and(|value| value.allow_system_drive),
        restart_required,
        profile_paths,
        previous_root: None,
    }
}

#[cfg(test)]
pub(crate) use super::agent_storage_migration::migrate_agent_storage_with_verifier_core;
pub(crate) use super::agent_storage_migration::{migrate_agent_storage_core, MigrationActivity};
pub(crate) use super::agent_storage_profile::is_user_global_profile_path;

fn profile_statuses(config: Option<&AgentStorageConfig>, root: &Path) -> Vec<AgentProfileStatus> {
    let paths = AgentStoragePaths::new(root.to_path_buf());
    registry::all_acp_agents()
        .into_iter()
        .map(|agent_type| {
            let registry_id = registry::registry_id_for(agent_type);
            let override_path = config.and_then(|value| value.profile_overrides.get(registry_id));
            AgentProfileStatus {
                agent_type,
                path: override_path
                    .cloned()
                    .unwrap_or_else(|| paths.profile(agent_type).root),
                overridden: override_path.is_some(),
            }
        })
        .collect()
}

fn map_storage_error(error: AgentStorageError) -> AppCommandError {
    AppCommandError::agent_storage_invalid("Failed to load Agent storage settings")
        .with_detail(error.to_string())
}

fn map_profile_import_error(error: ProfileImportError) -> AppCommandError {
    AppCommandError::agent_storage_invalid("Failed to import existing Agent settings")
        .with_detail(error.to_string())
}

#[cfg(feature = "tauri-runtime")]
pub use super::agent_storage_tauri::*;

#[cfg(test)]
#[path = "agent_storage_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "agent_storage_migration_tests.rs"]
mod migration_tests;
