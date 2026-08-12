use serde_json::{json, Value};

use crate::db::entities::automation::TriggerKind;
use crate::db::entities::folder::FolderKind;
use crate::models::agent::AgentType;
use crate::models::{AutomationConfig, AutomationDraft, AutomationInfo, FolderDetail};

use super::types::{ScheduledTaskPatch, ScheduledTaskView};

pub(super) fn merge_patch(
    folders: &[FolderDetail],
    current: &AutomationInfo,
    patch: ScheduledTaskPatch,
) -> Result<AutomationDraft, String> {
    let project_changed = patch.project.is_some() || patch.project_id.is_some();
    validate_patch(&patch)?;
    let selected_project =
        resolve_project_selection(folders, patch.project.as_deref(), patch.project_id)?;
    let root_folder_id = selected_project.or(current.root_folder_id);
    let config = match patch.prompt {
        Some(prompt) => prompt_config(prompt)?,
        None => current.config.clone(),
    };
    Ok(AutomationDraft {
        name: patch.name.unwrap_or_else(|| current.name.clone()),
        enabled: patch.enabled.unwrap_or(current.enabled),
        trigger_kind: current.trigger_kind.clone(),
        cron: patch.cron.map(Some).unwrap_or_else(|| current.cron.clone()),
        timezone: patch.timezone.unwrap_or_else(|| current.timezone.clone()),
        agent_type: patch
            .agent_type
            .as_deref()
            .map(normalize_agent_type)
            .transpose()?
            .unwrap_or_else(|| current.agent_type.clone()),
        root_folder_id,
        isolation: current.isolation.clone(),
        branch: (!project_changed).then(|| current.branch.clone()).flatten(),
        is_remote_branch: !project_changed && current.is_remote_branch,
        config,
    })
}

fn validate_patch(patch: &ScheduledTaskPatch) -> Result<(), String> {
    for (field, value) in [
        ("name", patch.name.as_deref()),
        ("prompt", patch.prompt.as_deref()),
        ("cron", patch.cron.as_deref()),
        ("timezone", patch.timezone.as_deref()),
    ] {
        if let Some(value) = value {
            require_text(value, field)?;
        }
    }
    Ok(())
}

pub(super) fn resolve_project(folders: &[FolderDetail], query: &str) -> Result<i32, String> {
    let query = query.trim();
    require_text(query, "project")?;
    let regular = folders
        .iter()
        .filter(|folder| folder.kind == FolderKind::Regular);
    if let Some(folder) = regular
        .clone()
        .find(|folder| paths_equal(&folder.path, query))
    {
        return Ok(folder.id);
    }
    let matches = regular
        .filter(|folder| folder.name.to_lowercase() == query.to_lowercase())
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [folder] => Ok(folder.id),
        [] => Err(project_not_found()),
        _ => Err(
            "project name is ambiguous; call list_scheduled_task_projects and use project_id"
                .to_string(),
        ),
    }
}

pub(super) fn resolve_project_selection(
    folders: &[FolderDetail],
    project: Option<&str>,
    project_id: Option<i32>,
) -> Result<Option<i32>, String> {
    match (project, project_id) {
        (Some(_), Some(_)) => Err("project and project_id cannot be used together".to_string()),
        (Some(query), None) => resolve_project(folders, query).map(Some),
        (None, Some(id)) => resolve_project_id(folders, id).map(Some),
        (None, None) => Ok(None),
    }
}

fn resolve_project_id(folders: &[FolderDetail], project_id: i32) -> Result<i32, String> {
    folders
        .iter()
        .find(|folder| folder.id == project_id && folder.kind == FolderKind::Regular)
        .map(|folder| folder.id)
        .ok_or_else(project_not_found)
}

fn project_not_found() -> String {
    "project not found; call list_scheduled_task_projects and use project_id, or omit the project when creating to use a dedicated folder".to_string()
}

pub(super) fn task_view(task: AutomationInfo, folders: &[FolderDetail]) -> ScheduledTaskView {
    let folder = task
        .root_folder_id
        .and_then(|id| folders.iter().find(|folder| folder.id == id));
    let prompt = serde_json::from_value::<AutomationConfig>(task.config.clone())
        .map(prompt_text)
        .unwrap_or_default();
    ScheduledTaskView {
        id: task.id,
        name: task.name,
        enabled: task.enabled,
        cron: task.cron,
        timezone: task.timezone,
        next_run_at: task.next_run_at,
        agent_type: task.agent_type,
        project_id: task.root_folder_id,
        project_name: folder.map(|item| item.name.clone()),
        prompt,
        last_run_at: task.last_run_at,
        last_run_status: task.last_run_status,
        created_at: task.created_at,
        updated_at: task.updated_at,
    }
}

fn prompt_text(config: AutomationConfig) -> String {
    if !config.display_text.trim().is_empty() {
        return config.display_text;
    }
    config
        .prompt_blocks
        .iter()
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn prompt_config(prompt: String) -> Result<Value, String> {
    let config = AutomationConfig {
        prompt_blocks: vec![json!({ "type": "text", "text": prompt })],
        display_text: prompt,
        ..AutomationConfig::default()
    };
    serde_json::to_value(config).map_err(|error| format!("serialize prompt config: {error}"))
}

pub(super) fn normalize_agent_type(raw: &str) -> Result<String, String> {
    let agent = serde_json::from_value::<AgentType>(Value::String(raw.trim().to_string()))
        .map_err(|_| format!("unsupported agent_type: {raw}"))?;
    serde_json::to_value(agent)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .ok_or_else(|| format!("unsupported agent_type: {raw}"))
}

pub(super) fn require_text(value: &str, field: &str) -> Result<(), String> {
    (!value.trim().is_empty())
        .then_some(())
        .ok_or_else(|| format!("{field} is required"))
}

pub(super) fn ensure_scheduled(task: &AutomationInfo) -> Result<(), String> {
    (task.trigger_kind == TriggerKind::Schedule)
        .then_some(())
        .ok_or_else(|| format!("scheduled task not found: {}", task.id))
}

fn paths_equal(left: &str, right: &str) -> bool {
    if cfg!(windows) {
        trim_path(left).eq_ignore_ascii_case(trim_path(right))
    } else {
        trim_path(left) == trim_path(right)
    }
}

fn trim_path(value: &str) -> &str {
    value.trim().trim_end_matches(['/', '\\'])
}

pub(super) fn db_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
