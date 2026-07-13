use std::ffi::OsString;
use std::path::PathBuf;

use super::{
    effective_startup_config, startup_profile_env, startup_profile_env_is_complete,
    startup_profile_env_matches, AgentStorageConfig, AgentStoragePaths,
};

#[test]
fn startup_profile_env_uses_persisted_codex_override() {
    let paths = AgentStoragePaths::new(PathBuf::from("D:/iyw-claw-data"));
    let mut config = AgentStorageConfig::confirmed(PathBuf::from("D:/iyw-claw-data"));
    config
        .profile_overrides
        .insert("codex-acp".to_string(), PathBuf::from("E:/profiles/codex"));

    let env = startup_profile_env(&paths, &config);

    assert_eq!(
        env.get("CODEX_HOME").map(PathBuf::from),
        Some(PathBuf::from("E:/profiles/codex"))
    );
    assert_eq!(
        env.get("CLAUDE_CONFIG_DIR").map(PathBuf::from),
        Some(paths.root().join("config/claude"))
    );
    assert!(env.contains_key("GEMINI_CLI_HOME"));
    assert!(env.contains_key("XDG_CONFIG_HOME"));
    assert!(env.contains_key("HERMES_HOME"));
    assert!(env.contains_key("PI_CODING_AGENT_DIR"));
}

#[test]
fn startup_profile_env_match_detects_profile_override_pending_restart() {
    let paths = AgentStoragePaths::new(PathBuf::from("D:/iyw-claw-data"));
    let mut config = AgentStorageConfig::confirmed(PathBuf::from("D:/iyw-claw-data"));
    let active = startup_profile_env(&paths, &config);
    assert!(startup_profile_env_matches(&paths, &config, |key| {
        active.get(key).cloned()
    }));

    config
        .profile_overrides
        .insert("codex-acp".to_string(), PathBuf::from("E:/profiles/codex"));
    assert!(!startup_profile_env_matches(&paths, &config, |key| {
        active.get(key).cloned()
    }));
}

#[test]
fn profile_write_environment_requires_every_absolute_profile_path() {
    let paths = AgentStoragePaths::new(PathBuf::from("D:/iyw-claw-data"));
    let config = AgentStorageConfig::confirmed(PathBuf::from("D:/iyw-claw-data"));
    let mut active = startup_profile_env(&paths, &config);

    assert!(startup_profile_env_is_complete(&paths, |key| active
        .get(key)
        .cloned()));

    active.remove("CODEX_HOME");
    assert!(!startup_profile_env_is_complete(&paths, |key| active
        .get(key)
        .cloned()));

    active.insert("CODEX_HOME".to_string(), OsString::from("relative/codex"));
    assert!(!startup_profile_env_is_complete(&paths, |key| active
        .get(key)
        .cloned()));
}

#[test]
fn startup_env_root_override_preserves_profile_overrides() {
    let mut persisted = AgentStorageConfig::confirmed(PathBuf::from("D:/persisted"));
    persisted
        .profile_overrides
        .insert("codex-acp".to_string(), PathBuf::from("F:/profiles/codex"));

    let (paths, effective) = effective_startup_config(
        Some(OsString::from("E:/runtime-root")),
        Some(&persisted),
        Some(PathBuf::from("G:/server")),
    )
    .expect("effective startup config");

    assert_eq!(paths.root(), &PathBuf::from("E:/runtime-root"));
    assert_eq!(effective.root, Some(PathBuf::from("E:/runtime-root")));
    assert_eq!(
        effective.profile_overrides.get("codex-acp"),
        Some(&PathBuf::from("F:/profiles/codex"))
    );
}
