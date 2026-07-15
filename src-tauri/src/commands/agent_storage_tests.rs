use std::path::{Path, PathBuf};

use crate::acp::agent_storage::{load_config, AgentStoragePaths};
use crate::acp::profile_import::{ProfileSourceRoots, PROFILE_IMPORT_VERSION};
use crate::acp::registry;
use crate::models::agent::AgentType;

use super::{
    get_agent_storage_status_core, initialize_agent_storage_core,
    initialize_agent_storage_with_sources_core, is_user_global_profile_path,
    update_agent_profile_override_core,
};

#[test]
fn user_global_profile_detection_is_agent_specific() {
    let home = Path::new("C:/Users/demo");
    let xdg_config = Path::new("C:/Users/demo/.config");

    assert!(is_user_global_profile_path(
        AgentType::Codex,
        Path::new("c:/users/demo/.codex/"),
        home,
        xdg_config,
    ));
    assert!(is_user_global_profile_path(
        AgentType::ClaudeCode,
        Path::new("C:/Users/demo/.claude"),
        home,
        xdg_config,
    ));
    assert!(!is_user_global_profile_path(
        AgentType::Codex,
        Path::new("D:/iyw-claw-data/config/codex"),
        home,
        xdg_config,
    ));
}

#[tokio::test]
async fn uninitialized_status_returns_suggestion_without_active_root() {
    let db = crate::db::test_helpers::fresh_in_memory_db().await;
    let executable = PathBuf::from("D:/Apps/iyw-claw/iyw-claw.exe");

    let status = get_agent_storage_status_core(&db.conn, &executable, None, None)
        .await
        .expect("load status");

    assert!(!status.initialized);
    assert_eq!(status.active_root, None);
    assert_eq!(
        status.suggested_root,
        Some(PathBuf::from("D:/Apps/iyw-claw"))
    );
    assert!(!status.restart_required);
}

#[tokio::test]
async fn initialization_creates_layout_and_requires_restart() {
    let db = crate::db::test_helpers::fresh_in_memory_db().await;
    let parent = tempfile::tempdir().expect("create temp dir");
    let root = parent.path().join("iyw-claw-data");

    let status = initialize_agent_storage_core(&db.conn, root.clone(), false, None, false)
        .await
        .expect("initialize storage");

    assert!(status.initialized);
    assert_eq!(status.active_root, Some(root.clone()));
    assert!(status.restart_required);
    let paths = AgentStoragePaths::new(root.clone());
    for dir in [
        paths.runtime_dir(),
        paths.config_dir(),
        paths.downloads_dir(),
        paths.staging_dir(),
        paths.trash_dir(),
    ] {
        assert!(dir.is_dir(), "missing initialized directory: {dir:?}");
    }
    for agent in registry::all_acp_agents() {
        assert!(
            paths.profile(agent).root.is_dir(),
            "missing {agent:?} profile"
        );
    }
    let persisted = load_config(&db.conn)
        .await
        .expect("load persisted config")
        .expect("persisted config");
    assert_eq!(persisted.root, Some(root));
    assert!(persisted.initialized);
}

#[tokio::test]
async fn initialization_imports_profiles_and_persists_version() {
    let db = crate::db::test_helpers::fresh_in_memory_db().await;
    let temp = tempfile::tempdir().expect("temp dir");
    let root = temp.path().join("iyw-claw-data");
    let sources = profile_sources(temp.path());
    seed_import_sources(&sources);

    initialize_agent_storage_with_sources_core(&db.conn, root.clone(), false, None, Some(&sources))
        .await
        .expect("initialize with import");

    let config = load_config(&db.conn)
        .await
        .expect("load config")
        .expect("persisted config");
    assert_eq!(config.import_version, PROFILE_IMPORT_VERSION);
    assert!(AgentStoragePaths::new(root)
        .profile(AgentType::Codex)
        .root
        .join("auth.json")
        .is_file());
}

#[tokio::test]
async fn completed_profile_import_is_not_repeated() {
    let db = crate::db::test_helpers::fresh_in_memory_db().await;
    let temp = tempfile::tempdir().expect("temp dir");
    let root = temp.path().join("iyw-claw-data");
    let sources = profile_sources(temp.path());
    seed_import_sources(&sources);
    initialize_agent_storage_with_sources_core(&db.conn, root.clone(), false, None, Some(&sources))
        .await
        .expect("first initialization");
    let auth = AgentStoragePaths::new(root.clone())
        .profile(AgentType::Codex)
        .root
        .join("auth.json");
    std::fs::remove_file(&auth).expect("remove imported auth");

    initialize_agent_storage_with_sources_core(&db.conn, root, false, None, Some(&sources))
        .await
        .expect("second initialization");

    assert!(!auth.exists(), "one-time import must not run again");
}

#[tokio::test]
async fn initialization_without_import_keeps_version_zero() {
    let db = crate::db::test_helpers::fresh_in_memory_db().await;
    let temp = tempfile::tempdir().expect("temp dir");
    let root = temp.path().join("iyw-claw-data");

    initialize_agent_storage_with_sources_core(&db.conn, root, false, None, None)
        .await
        .expect("initialize without import");

    let config = load_config(&db.conn)
        .await
        .expect("load config")
        .expect("persisted config");
    assert_eq!(config.import_version, 0);
}

#[cfg(windows)]
#[tokio::test]
async fn system_drive_initialization_requires_explicit_approval() {
    let db = crate::db::test_helpers::fresh_in_memory_db().await;
    let parent = tempfile::tempdir().expect("create temp dir");
    let root = parent.path().join("iyw-claw-data");
    let system_drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());

    let error = initialize_agent_storage_core(&db.conn, root, false, Some(&system_drive), false)
        .await
        .expect_err("system drive must require approval");

    assert!(matches!(
        error.code,
        crate::app_error::AppErrorCode::PermissionDenied
    ));
    assert!(load_config(&db.conn).await.expect("load config").is_none());
}

#[tokio::test]
async fn profile_override_persists_and_requires_restart() {
    let db = crate::db::test_helpers::fresh_in_memory_db().await;
    let parent = tempfile::tempdir().expect("create temp dir");
    let root = parent.path().join("iyw-claw-data");
    initialize_agent_storage_core(&db.conn, root.clone(), false, None, false)
        .await
        .expect("initialize storage");
    let override_path = parent.path().join("custom-codex-profile");

    let status = update_agent_profile_override_core(
        &db.conn,
        AgentType::Codex,
        Some(override_path.clone()),
        false,
        false,
        None,
    )
    .await
    .expect("save override");

    assert!(status.restart_required);
    let codex = status
        .profile_paths
        .iter()
        .find(|profile| profile.agent_type == AgentType::Codex)
        .expect("codex profile status");
    assert_eq!(codex.path, override_path);
    assert!(codex.overridden);
    let persisted = load_config(&db.conn)
        .await
        .expect("load config")
        .expect("persisted config");
    assert_eq!(
        persisted.profile_overrides.get("codex-acp"),
        Some(&codex.path)
    );
}

#[tokio::test]
async fn persisted_root_without_active_env_still_requires_restart() {
    let db = crate::db::test_helpers::fresh_in_memory_db().await;
    let parent = tempfile::tempdir().expect("create temp dir");
    let root = parent.path().join("iyw-claw-data");
    initialize_agent_storage_core(&db.conn, root, false, None, false)
        .await
        .expect("initialize storage");

    let status = get_agent_storage_status_core(
        &db.conn,
        &PathBuf::from("D:/Apps/iyw-claw/iyw-claw.exe"),
        None,
        None,
    )
    .await
    .expect("load status");

    assert!(status.restart_required);
}

#[tokio::test]
async fn profile_override_before_initialization_has_typed_error() {
    let db = crate::db::test_helpers::fresh_in_memory_db().await;

    let error = update_agent_profile_override_core(
        &db.conn,
        AgentType::Codex,
        Some(PathBuf::from("D:/profiles/codex")),
        false,
        false,
        None,
    )
    .await
    .expect_err("uninitialized storage must reject profile override");

    assert!(matches!(
        error.code,
        crate::app_error::AppErrorCode::AgentStorageNotInitialized
    ));
}

fn profile_sources(base: &std::path::Path) -> ProfileSourceRoots {
    ProfileSourceRoots::new(
        base.join("home"),
        base.join("xdg-config"),
        base.join("xdg-data"),
    )
}

fn seed_import_sources(sources: &ProfileSourceRoots) {
    let codex = sources.home.join(".codex");
    std::fs::create_dir_all(&codex).expect("codex source");
    std::fs::write(codex.join("auth.json"), "{\"token\":\"source\"}\n").expect("codex auth source");
}
