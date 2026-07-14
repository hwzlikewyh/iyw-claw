use std::ffi::OsString;
use std::path::PathBuf;

use crate::acp::registry;
use crate::models::agent::AgentType;

use super::{
    is_windows_system_drive, load_config, resolve_root, save_config, suggest_desktop_root,
    validate_root, AgentStorageConfig, AgentStoragePaths, STORAGE_METADATA_KEY,
};

#[test]
fn env_override_wins_over_persisted_root() {
    let persisted = AgentStorageConfig::confirmed(PathBuf::from("D:/persisted"));
    let resolved = resolve_root(
        Some(OsString::from("E:/env")),
        Some(&persisted),
        Some(PathBuf::from("F:/server")),
    );
    assert_eq!(resolved, Some(PathBuf::from("E:/env")));
}

#[test]
fn confirmed_persisted_root_wins_over_server_fallback() {
    let persisted = AgentStorageConfig::confirmed(PathBuf::from("D:/persisted"));
    let resolved = resolve_root(None, Some(&persisted), Some(PathBuf::from("F:/server")));
    assert_eq!(resolved, Some(PathBuf::from("D:/persisted")));
}

#[test]
fn server_fallback_is_used_without_env_or_persisted_root() {
    let fallback = PathBuf::from("D:/server-data/agents");

    assert_eq!(
        resolve_root(None, None, Some(fallback.clone())),
        Some(fallback)
    );
}

#[test]
fn uninitialized_persisted_root_is_not_active() {
    let persisted = AgentStorageConfig {
        root: Some(PathBuf::from("D:/unconfirmed")),
        initialized: false,
        allow_system_drive: false,
        import_version: 0,
        profile_overrides: Default::default(),
    };
    let resolved = resolve_root(None, Some(&persisted), Some(PathBuf::from("F:/server")));
    assert_eq!(resolved, Some(PathBuf::from("F:/server")));
}

#[test]
fn storage_layout_keeps_owned_directories_below_root() {
    let paths = AgentStoragePaths::new(PathBuf::from("D:/iyw-claw-data"));
    for owned in [
        paths.runtime_dir(),
        paths.config_dir(),
        paths.downloads_dir(),
        paths.staging_dir(),
        paths.trash_dir(),
    ] {
        assert!(owned.starts_with(paths.root()), "{owned:?} escaped root");
    }
}

#[test]
fn runtime_layout_separates_binary_and_uv_state() {
    let root = PathBuf::from("D:/iyw-claw-data");
    let paths = AgentStoragePaths::new(root.clone());

    assert_eq!(
        paths.binary_runtime_dir(),
        root.join("runtime").join("binary")
    );
    assert_eq!(paths.npm_runtime_dir(), root.join("runtime").join("npm"));
    assert_eq!(
        paths.npm_cache_dir(),
        root.join("runtime").join("npm").join("cache")
    );
    assert_eq!(paths.uv_runtime_dir(), root.join("runtime").join("uv"));
    assert_eq!(
        paths.uv_cache_dir(),
        root.join("runtime").join("uv").join("cache")
    );
}

#[test]
fn private_profiles_cover_every_agent() {
    let paths = AgentStoragePaths::new(PathBuf::from("D:/iyw-claw-data"));
    for agent in registry::all_acp_agents() {
        let profile = paths.profile(agent);
        assert!(
            profile.root.starts_with(paths.config_dir()),
            "missing private profile for {agent:?}: {:?}",
            profile.root
        );
        assert!(!profile.env.is_empty(), "missing path env for {agent:?}");
    }
}

#[test]
fn profile_environment_matches_agent_contracts() {
    let paths = AgentStoragePaths::new(PathBuf::from("D:/iyw-claw-data"));
    let cases = [
        (AgentType::ClaudeCode, "CLAUDE_CONFIG_DIR", "config/claude"),
        (AgentType::Codex, "CODEX_HOME", "config/codex"),
        (AgentType::Gemini, "GEMINI_CLI_HOME", "config/gemini-home"),
        (AgentType::OpenClaw, "OPENCLAW_STATE_DIR", "config/openclaw"),
        (
            AgentType::OpenCode,
            "XDG_CONFIG_HOME",
            "config/opencode/config",
        ),
        (AgentType::Cline, "CLINE_DIR", "config/cline"),
        (AgentType::Hermes, "HERMES_HOME", "config/hermes"),
        (
            AgentType::CodeBuddy,
            "CODEBUDDY_CONFIG_DIR",
            "config/codebuddy",
        ),
        (AgentType::KimiCode, "KIMI_CODE_HOME", "config/kimi-code"),
        (AgentType::Pi, "PI_CODING_AGENT_DIR", "config/pi"),
    ];
    for (agent, key, relative) in cases {
        let profile = paths.profile(agent);
        assert_eq!(
            profile.env.get(key),
            Some(&paths.root().join(relative)),
            "wrong {key} for {agent:?}"
        );
    }
}

#[test]
fn validation_rejects_relative_paths_without_creating_them() {
    let relative = PathBuf::from("relative-agent-storage");
    let result = validate_root(&relative, Some("C:"));
    assert!(!result.writable);
    assert!(result
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("absolute"));
    assert!(!relative.exists());
}

#[test]
fn validation_rejects_an_existing_file() {
    let file = tempfile::NamedTempFile::new().expect("create file");
    let result = validate_root(file.path(), Some("C:"));
    assert!(!result.writable);
    assert!(result
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("directory"));
}

#[test]
fn validation_creates_writable_directory_and_removes_probe() {
    let parent = tempfile::tempdir().expect("create temp dir");
    let target = parent.path().join("nested").join("agent-storage");
    let result = validate_root(&target, None);
    assert!(result.writable, "validation failed: {:?}", result.error);
    assert!(target.is_dir());
    assert_eq!(
        std::fs::read_dir(&target).expect("read target").count(),
        0,
        "probe file must be removed"
    );
}

#[test]
fn windows_system_drive_detection_is_case_insensitive() {
    assert!(is_windows_system_drive(
        PathBuf::from("c:/iyw-claw-data").as_path(),
        Some("C:")
    ));
    assert!(!is_windows_system_drive(
        PathBuf::from("D:/iyw-claw-data").as_path(),
        Some("C:")
    ));
    assert!(!is_windows_system_drive(
        PathBuf::from("/srv/iyw-claw-data").as_path(),
        Some("C:")
    ));
}

#[tokio::test]
async fn storage_config_round_trips_through_app_metadata() {
    let db = crate::db::test_helpers::fresh_in_memory_db().await;
    let mut config = AgentStorageConfig::confirmed(PathBuf::from("D:/iyw-claw-data"));
    config.allow_system_drive = true;
    config.import_version = 2;
    config
        .profile_overrides
        .insert("codex-acp".to_string(), PathBuf::from("E:/profiles/codex"));

    save_config(&db.conn, &config).await.expect("save config");
    let loaded = load_config(&db.conn).await.expect("load config");

    assert_eq!(loaded, Some(config));
}

#[tokio::test]
async fn malformed_storage_metadata_is_an_error() {
    let db = crate::db::test_helpers::fresh_in_memory_db().await;
    crate::db::service::app_metadata_service::upsert_value(
        &db.conn,
        STORAGE_METADATA_KEY,
        "{not-json",
    )
    .await
    .expect("seed malformed config");

    let error = load_config(&db.conn)
        .await
        .expect_err("malformed config must not be treated as uninitialized");

    assert!(error.to_string().contains("invalid agent storage config"));
}

#[test]
fn desktop_suggestion_uses_the_product_root_above_app() {
    let executable = PathBuf::from("D:/Apps/iyw-claw/app/iyw-claw.exe");

    let suggested = suggest_desktop_root(&executable);

    assert_eq!(suggested, Some(PathBuf::from("D:/Apps/iyw-claw")));
}

#[test]
fn desktop_suggestion_appends_product_name_at_drive_root() {
    let executable = PathBuf::from("D:/iyw-claw.exe");

    let suggested = suggest_desktop_root(&executable);

    assert_eq!(suggested, Some(PathBuf::from("D:/iyw-claw")));
}
