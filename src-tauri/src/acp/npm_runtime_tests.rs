use std::path::{Path, PathBuf};

use super::*;
use crate::acp::agent_storage::AgentStoragePaths;
use crate::models::agent::AgentType;

fn command_path(prefix: &Path, command: &str) -> PathBuf {
    let bin_dir = npm_prefix_bin_dir(prefix);
    if cfg!(windows) {
        bin_dir.join(format!("{command}.cmd"))
    } else {
        bin_dir.join(command)
    }
}

fn write_command(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b"command").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions).unwrap();
    }
}

#[test]
fn private_prefix_is_versioned_by_agent_and_platform() {
    let paths = AgentStoragePaths::new(PathBuf::from("D:/iyw-claw-data"));
    assert_eq!(
        private_npm_prefix(&paths, AgentType::Codex, "v1.1.0").unwrap(),
        paths
            .npm_runtime_dir()
            .join("codex-acp")
            .join("1.1.0")
            .join(crate::acp::registry::current_platform())
    );
    assert!(private_npm_prefix(&paths, AgentType::Codex, "../outside").is_err());
}

#[test]
fn private_resolver_ignores_system_command_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let paths = AgentStoragePaths::new(temp.path().join("private"));
    let prefix = private_npm_prefix(&paths, AgentType::Gemini, "0.47.0").unwrap();
    let private_command = command_path(&prefix, "gemini");
    let system_bin = temp.path().join("system-bin");
    let system_command = command_path(&system_bin, "gemini");
    write_command(&private_command);
    write_command(&system_command);

    temp_env::with_var("PATH", Some(system_bin.as_path()), || {
        assert!(which::which("gemini").is_ok());
        assert_eq!(
            resolve_private_npm_command(&paths, AgentType::Gemini, "0.47.0", "gemini"),
            Some(private_command.clone())
        );

        std::fs::remove_file(&private_command).unwrap();
        assert_eq!(
            resolve_private_npm_command(&paths, AgentType::Gemini, "0.47.0", "gemini"),
            None
        );
    });
}

#[test]
fn pi_adapter_and_child_share_one_private_prefix() {
    let temp = tempfile::tempdir().unwrap();
    let paths = AgentStoragePaths::new(temp.path().join("private"));
    let prefix = private_npm_prefix(&paths, AgentType::Pi, "0.0.31").unwrap();
    let adapter = command_path(&prefix, "pi-acp");
    let child = command_path(&prefix, "pi");
    write_command(&adapter);
    write_command(&child);

    assert_eq!(
        resolve_private_npm_command(&paths, AgentType::Pi, "0.0.31", "pi-acp"),
        Some(adapter)
    );
    assert_eq!(
        resolve_private_npm_command(&paths, AgentType::Pi, "0.0.31", "pi"),
        Some(child)
    );
}

#[test]
fn npm_install_args_always_use_private_prefix_without_force() {
    let prefix = PathBuf::from("D:/iyw-claw-data/staging/codex");
    let cache = PathBuf::from("D:/iyw-claw-data/runtime/npm/cache");
    let args = private_npm_install_args(&prefix, &cache, &["@agentclientprotocol/codex-acp@1.1.0"]);
    let args = args
        .iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    assert_eq!(args[0..3], ["install", "--global", "--include=optional"]);
    assert!(args.contains(&format!("--prefix={}", prefix.display())));
    assert!(args.contains(&format!("--cache={}", cache.display())));
    assert!(args.contains(&"@agentclientprotocol/codex-acp@1.1.0".to_string()));
    assert!(!args.iter().any(|arg| arg == "--force"));
}

#[test]
fn failed_pi_activation_keeps_previous_private_runtime() {
    let temp = tempfile::tempdir().unwrap();
    let paths = AgentStoragePaths::new(temp.path().join("private"));
    let previous_prefix = private_npm_prefix(&paths, AgentType::Pi, "0.0.31").unwrap();
    let previous_adapter = command_path(&previous_prefix, "pi-acp");
    let previous_child = command_path(&previous_prefix, "pi");
    write_command(&previous_adapter);
    write_command(&previous_child);

    let staging = paths.staging_dir().join("pi-incomplete");
    write_command(&command_path(&staging, "pi-acp"));

    assert!(activate_private_npm_runtime(
        &paths,
        AgentType::Pi,
        "0.0.31",
        &staging,
        &["pi-acp", "pi"]
    )
    .is_err());
    assert!(previous_adapter.is_file());
    assert!(previous_child.is_file());
    assert!(!staging.exists());
}

#[test]
fn uninstall_removes_only_the_private_agent_runtime() {
    let temp = tempfile::tempdir().unwrap();
    let paths = AgentStoragePaths::new(temp.path().join("private"));
    let prefix = private_npm_prefix(&paths, AgentType::Codex, "1.1.0").unwrap();
    write_command(&command_path(&prefix, "codex-acp"));
    let unrelated = temp.path().join("system").join("codex-acp.cmd");
    write_command(&unrelated);

    uninstall_private_npm_runtime(&paths, AgentType::Codex).unwrap();

    assert!(!paths.npm_runtime_dir().join("codex-acp").exists());
    assert!(unrelated.is_file());
}

#[cfg(not(windows))]
#[test]
fn private_resolver_rejects_non_executable_command() {
    let temp = tempfile::tempdir().unwrap();
    let paths = AgentStoragePaths::new(temp.path().join("private"));
    let prefix = private_npm_prefix(&paths, AgentType::Gemini, "0.47.0").unwrap();
    let command = command_path(&prefix, "gemini");
    std::fs::create_dir_all(command.parent().unwrap()).unwrap();
    std::fs::write(&command, b"not executable").unwrap();

    assert_eq!(
        resolve_private_npm_command(&paths, AgentType::Gemini, "0.47.0", "gemini"),
        None
    );
}
