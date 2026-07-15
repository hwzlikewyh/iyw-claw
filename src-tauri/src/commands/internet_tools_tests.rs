use std::fs;

use super::*;
use serde_json::json;

#[test]
fn install_specs_pin_agent_reach_and_opencli() {
    assert_eq!(
        agent_reach_package_spec(),
        "https://github.com/Panniantong/Agent-Reach/archive/refs/tags/v1.5.0.zip"
    );
    assert_eq!(opencli_package_spec(), "@jackwener/opencli@1.8.6");
    assert_eq!(mcporter_package_spec(), "mcporter@0.9.0");
}

#[test]
fn sync_packaged_skills_copies_agent_reach_and_opencli_skills() {
    let temp = tempfile::tempdir().expect("tempdir");
    let agent_reach = temp.path().join("agent-reach-skill");
    let opencli = temp.path().join("opencli-skills");
    let central = temp.path().join("central");
    fs::create_dir_all(&agent_reach).unwrap();
    fs::write(agent_reach.join("SKILL.md"), "# Agent Reach").unwrap();
    fs::create_dir_all(opencli.join("opencli-usage")).unwrap();
    fs::write(opencli.join("opencli-usage/SKILL.md"), "# OpenCLI").unwrap();
    fs::create_dir_all(opencli.join("not-a-skill")).unwrap();

    let synced = sync_packaged_skills(&agent_reach, &opencli, &central).unwrap();

    assert_eq!(synced, vec!["agent-reach", "opencli-usage"]);
    assert!(central.join("agent-reach/SKILL.md").is_file());
    assert!(central.join("opencli-usage/SKILL.md").is_file());
    assert!(!central.join("not-a-skill").exists());
}

#[test]
fn private_tool_paths_are_stable_before_installation() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = AgentStoragePaths::new(temp.path().join("storage"));

    assert_eq!(uv_tool_bin_dir(&paths), paths.uv_runtime_dir().join("bin"));
    assert_eq!(
        npm_runtime::npm_prefix_bin_dir(&opencli_prefix(&paths)),
        opencli_prefix(&paths).join(if cfg!(windows) { "" } else { "bin" })
    );
}

#[test]
fn private_runtime_exposes_uv_commands_and_environment() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = AgentStoragePaths::new(temp.path().join("storage"));

    assert_eq!(
        private_tool_bin_dirs_for(&paths),
        vec![
            binary_cache::uv_tool_dir_for(&paths),
            uv_tool_bin_dir(&paths),
            npm_runtime::npm_prefix_bin_dir(&opencli_prefix(&paths)),
        ]
    );
    assert_eq!(
        private_tool_environment_for(&paths),
        vec![
            ("UV_CACHE_DIR", paths.uv_cache_dir()),
            ("UV_TOOL_BIN_DIR", uv_tool_bin_dir(&paths)),
            ("UV_TOOL_DIR", paths.uv_runtime_dir().join("tools")),
            ("MCPORTER_CONFIG", mcporter_config_path(&paths)),
        ]
    );
}

#[test]
fn agent_reach_doctor_json_preserves_channel_diagnostics() {
    let channels = parse_agent_reach_doctor_json(
        r#"{
          "github": {
            "status": "ok",
            "name": "GitHub repositories",
            "message": "Ready",
            "tier": 0,
            "backends": ["gh CLI"],
            "active_backend": "gh CLI"
          },
          "reddit": {
            "status": "off",
            "name": "Reddit",
            "message": "Login required",
            "tier": 1,
            "backends": ["OpenCLI", "rdt-cli"],
            "active_backend": null
          }
        }"#,
    )
    .expect("doctor JSON");

    assert_eq!(channels.len(), 2);
    assert_eq!(channels[0].id, "github");
    assert_eq!(channels[0].status, InternetChannelHealth::Ok);
    assert_eq!(channels[0].active_backend.as_deref(), Some("gh CLI"));
    assert_eq!(channels[1].id, "reddit");
    assert_eq!(channels[1].status, InternetChannelHealth::Off);
    assert_eq!(channels[1].backends, vec!["OpenCLI", "rdt-cli"]);
}

#[test]
fn internet_tool_wire_types_use_stable_case_conventions() {
    assert_eq!(
        serde_json::to_value(InternetToolId::AgentReach).unwrap(),
        json!("agent_reach")
    );
    assert_eq!(
        serde_json::to_value(InternetToolId::Opencli).unwrap(),
        json!("opencli")
    );
    assert_eq!(
        serde_json::to_value(InternetToolStatus::UpdateAvailable).unwrap(),
        json!("update_available")
    );
}

#[test]
fn version_state_distinguishes_current_update_and_unhealthy_tools() {
    assert_eq!(
        tool_status(true, Some("1.5.0"), "1.5.0", None),
        InternetToolStatus::Installed
    );
    assert_eq!(
        tool_status(true, Some("1.4.0"), "1.5.0", None),
        InternetToolStatus::UpdateAvailable
    );
    assert_eq!(
        tool_status(true, None, "1.5.0", Some("failed to run")),
        InternetToolStatus::NotRunnable
    );
    assert_eq!(
        tool_status(false, None, "1.5.0", None),
        InternetToolStatus::NotInstalled
    );
}

#[test]
fn configuration_and_channel_values_are_closed_enums() {
    assert!(serde_json::from_value::<AgentReachConfigKey>(json!("github_token")).is_ok());
    assert!(serde_json::from_value::<AgentReachConfigKey>(json!("arbitrary")).is_err());
    assert!(serde_json::from_value::<SupportedBrowser>(json!("chrome")).is_ok());
    assert!(serde_json::from_value::<SupportedBrowser>(json!("custom-browser")).is_err());
    assert!(serde_json::from_value::<AgentReachChannel>(json!("xiaohongshu")).is_ok());
    assert!(serde_json::from_value::<AgentReachChannel>(json!("shell-command")).is_err());
}

#[test]
fn internet_skill_list_only_includes_managed_sources() {
    let temp = tempfile::tempdir().expect("tempdir");
    let central = temp.path().join("central");
    fs::create_dir_all(central.join("agent-reach")).unwrap();
    fs::write(central.join("agent-reach/SKILL.md"), "# Agent Reach").unwrap();
    fs::create_dir_all(central.join("opencli-browser")).unwrap();
    fs::write(central.join("opencli-browser/SKILL.md"), "# Browser").unwrap();
    fs::create_dir_all(central.join("unrelated")).unwrap();
    fs::write(central.join("unrelated/SKILL.md"), "# Ignore").unwrap();

    let skills = list_internet_skills_from(&central);

    assert_eq!(
        skills
            .iter()
            .map(|skill| skill.id.as_str())
            .collect::<Vec<_>>(),
        vec!["agent-reach", "opencli-browser"]
    );
    assert!(skills.iter().all(|skill| skill.installed_centrally));
}

#[test]
fn default_uninstall_plan_preserves_user_configuration() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = AgentStoragePaths::new(temp.path().join("storage"));
    let home_config = temp.path().join("home-agent-reach");

    let plan = uninstall_targets(&paths, InternetToolId::AgentReach, false, &home_config);
    assert!(!plan.contains(&home_config));

    let destructive = uninstall_targets(&paths, InternetToolId::AgentReach, true, &home_config);
    assert!(destructive.contains(&home_config));
}
