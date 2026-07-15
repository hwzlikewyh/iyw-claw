use std::fs;
use std::time::Duration;

use crate::acp::agent_storage::AgentStoragePaths;
use crate::commands::experts::central_experts_dir;

use super::*;

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn internet_tools_detect() -> Result<Vec<InternetToolInfo>, String> {
    let paths = active_paths()?;
    let (agent_reach, opencli) = tokio::join!(
        detect_tool(&paths, InternetToolId::AgentReach),
        detect_tool(&paths, InternetToolId::Opencli)
    );
    Ok(vec![agent_reach, opencli])
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn internet_tool_install(tool: InternetToolId) -> Result<InternetToolInfo, String> {
    let _guard = bootstrap_lock().lock().await;
    let paths = active_paths()?;
    match tool {
        InternetToolId::AgentReach => install_agent_reach(&paths).await?,
        InternetToolId::Opencli => install_opencli(&paths).await?,
    }
    sync_installed_skills(&paths)?;
    Ok(detect_tool(&paths, tool).await)
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn internet_tool_uninstall(
    tool: InternetToolId,
    remove_config: bool,
) -> Result<InternetToolInfo, String> {
    let _guard = bootstrap_lock().lock().await;
    let paths = active_paths()?;
    remove_agent_skill_links(tool).await?;
    let config = agent_reach_config_dir().unwrap_or_else(|| paths.config_dir().join("agent-reach"));
    for target in uninstall_targets(&paths, tool, remove_config, &config) {
        if target.exists() || fs::symlink_metadata(&target).is_ok() {
            crate::commands::acp::remove_skill_entry(&target).map_err(|error| error.to_string())?;
        }
    }
    remove_tool_assets(&paths, tool)?;
    let _ = fs::remove_file(paths.root().join(BOOTSTRAP_MARKER));
    Ok(detect_tool(&paths, tool).await)
}

async fn remove_agent_skill_links(tool: InternetToolId) -> Result<(), String> {
    let skill_ids = list_internet_skills_from(&central_experts_dir())
        .into_iter()
        .filter(|skill| skill.source == tool)
        .map(|skill| skill.id)
        .collect::<Vec<_>>();
    let targets = crate::commands::managed_skills::supported_skill_agent_types()
        .into_iter()
        .flat_map(|agent_type| {
            skill_ids
                .iter()
                .cloned()
                .map(move |skill_id| (agent_type, skill_id, false))
        })
        .collect::<Vec<_>>();
    let failures = reconcile_managed_internet_tools(&targets)
        .await
        .into_iter()
        .filter(|result| !result.ok)
        .filter_map(|result| result.error)
        .collect::<Vec<_>>();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Failed to remove managed skill links: {}",
            failures.join("; ")
        ))
    }
}

fn remove_tool_assets(paths: &AgentStoragePaths, tool: InternetToolId) -> Result<(), String> {
    match tool {
        InternetToolId::AgentReach => {
            let binary = agent_reach_command_path(paths);
            if binary.is_file() {
                fs::remove_file(binary).map_err(|error| error.to_string())?;
            }
            remove_managed_skill("agent-reach")
        }
        InternetToolId::Opencli => {
            for skill in list_internet_skills_from(&central_experts_dir())
                .into_iter()
                .filter(|skill| skill.source == InternetToolId::Opencli)
            {
                remove_managed_skill(&skill.id)?;
            }
            Ok(())
        }
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn internet_tools_sync_skills() -> Result<InternetSkillSyncReport, String> {
    let _guard = bootstrap_lock().lock().await;
    let paths = active_paths()?;
    let skill_ids = sync_installed_skills(&paths)?;
    Ok(InternetSkillSyncReport {
        synced: skill_ids.len(),
        skill_ids,
        errors: Vec::new(),
    })
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn internet_tools_list_skills() -> Vec<InternetToolSkill> {
    list_internet_skills_from(&central_experts_dir())
}

pub(crate) fn managed_internet_skill_ids() -> Vec<String> {
    list_internet_skills_from(&central_experts_dir())
        .into_iter()
        .map(|skill| skill.id)
        .collect()
}

pub(crate) fn managed_ready_internet_skill_ids() -> Vec<String> {
    managed_internet_skill_ids()
}

pub(crate) fn managed_internet_skill_has_owned_link(
    skill_id: &str,
    agents: &[crate::models::agent::AgentType],
) -> bool {
    crate::commands::experts::managed_expert_has_owned_link(skill_id, agents)
}

pub(crate) async fn reconcile_managed_internet_tools(
    targets: &[(crate::models::agent::AgentType, String, bool)],
) -> Vec<crate::commands::experts::LinkOpResult> {
    crate::commands::experts::reconcile_managed_experts(targets).await
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn internet_tools_read_skill(skill_id: String) -> Result<String, String> {
    let skill_id =
        crate::commands::acp::validate_skill_id(&skill_id).map_err(|error| error.to_string())?;
    if skill_id != "agent-reach" && !skill_id.starts_with("opencli-") {
        return Err("Unknown internet tool skill".to_string());
    }
    fs::read_to_string(central_experts_dir().join(skill_id).join("SKILL.md"))
        .map_err(|error| error.to_string())
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn internet_tools_agent_reach_doctor() -> Result<Vec<InternetChannelStatus>, String> {
    let paths = active_paths()?;
    let mut command = agent_reach_command(&paths)?;
    command.args(["doctor", "--json"]);
    let output = run_tool_output(command, "Agent Reach doctor", Duration::from_secs(90)).await?;
    if !output.status.success() {
        return Err(format!(
            "Agent Reach doctor failed: {}",
            output_text(&output)
        ));
    }
    parse_agent_reach_doctor_json(&output_text(&output))
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn internet_tools_opencli_doctor() -> Result<OpencliDoctorResult, String> {
    let paths = active_paths()?;
    let path = opencli_command_path(&paths);
    if !path.is_file() {
        return Err("OpenCLI is not installed".to_string());
    }
    let mut command = crate::process::tokio_command(path);
    command
        .arg("doctor")
        .envs(private_tool_environment_for(&paths));
    let output = run_tool_output(command, "OpenCLI doctor", Duration::from_secs(60)).await?;
    Ok(OpencliDoctorResult {
        ok: output.status.success(),
        message: output_text(&output),
    })
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn internet_tools_configure_agent_reach(
    key: AgentReachConfigKey,
    value: String,
) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("Configuration value cannot be empty".to_string());
    }
    let paths = active_paths()?;
    let mut command = agent_reach_command(&paths)?;
    command.args(["configure", key.cli_value(), value]);
    let output = run_tool_output(
        command,
        "Agent Reach configuration",
        Duration::from_secs(60),
    )
    .await?;
    if output.status.success() {
        Ok(())
    } else {
        Err("Agent Reach configuration failed; check the supplied value".to_string())
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn internet_tools_import_browser(browser: SupportedBrowser) -> Result<(), String> {
    let paths = active_paths()?;
    let mut command = agent_reach_command(&paths)?;
    command.args(["configure", "--from-browser", browser.cli_value()]);
    let output = run_tool_output(
        command,
        "browser credential import",
        Duration::from_secs(120),
    )
    .await?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!("Browser import failed: {}", output_text(&output)))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn internet_tools_install_channels(
    channels: Vec<AgentReachChannel>,
) -> Result<Vec<InternetChannelStatus>, String> {
    if channels.is_empty() {
        return Err("Select at least one channel".to_string());
    }
    let paths = active_paths()?;
    let channel_list = channels
        .iter()
        .map(|channel| channel.cli_value())
        .collect::<Vec<_>>()
        .join(",");
    let mut command = agent_reach_command(&paths)?;
    command.args(["install", "--env=auto", "--channels", &channel_list]);
    run_install_command(command, "Agent Reach channels").await?;
    internet_tools_agent_reach_doctor().await
}

fn active_paths() -> Result<AgentStoragePaths, String> {
    AgentStoragePaths::active().ok_or_else(|| "Agent storage is not active".to_string())
}

fn agent_reach_command(paths: &AgentStoragePaths) -> Result<tokio::process::Command, String> {
    let path = agent_reach_command_path(paths);
    if !path.is_file() {
        return Err("Agent Reach is not installed".to_string());
    }
    let mut command = crate::process::tokio_command(path);
    command.envs(private_tool_environment_for(paths));
    Ok(command)
}
