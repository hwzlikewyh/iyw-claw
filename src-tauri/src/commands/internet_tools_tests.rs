use std::fs;

use super::*;

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
