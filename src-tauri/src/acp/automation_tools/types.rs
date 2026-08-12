use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduledTaskOperation {
    ListProjects,
    List,
    Create,
    Update,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduledTaskRequest {
    pub operation: ScheduledTaskOperation,
    #[serde(default)]
    pub input: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller_agent_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListProjectsInput {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListInput {
    pub task_id: Option<i32>,
    pub project: Option<String>,
    pub project_id: Option<i32>,
    pub enabled: Option<bool>,
    pub agent_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateInput {
    pub name: String,
    pub project: Option<String>,
    pub project_id: Option<i32>,
    pub prompt: String,
    pub cron: String,
    #[serde(default = "default_timezone")]
    pub timezone: String,
    pub agent_type: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateInput {
    pub task_id: i32,
    pub patch: ScheduledTaskPatch,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeleteInput {
    pub task_id: i32,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScheduledTaskPatch {
    pub name: Option<String>,
    pub project: Option<String>,
    pub project_id: Option<i32>,
    pub prompt: Option<String>,
    pub cron: Option<String>,
    pub timezone: Option<String>,
    pub agent_type: Option<String>,
    pub enabled: Option<bool>,
}

impl ScheduledTaskPatch {
    pub(crate) fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.project.is_none()
            && self.project_id.is_none()
            && self.prompt.is_none()
            && self.cron.is_none()
            && self.timezone.is_none()
            && self.agent_type.is_none()
            && self.enabled.is_none()
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct ScheduledTaskView {
    pub id: i32,
    pub name: String,
    pub enabled: bool,
    pub cron: Option<String>,
    pub timezone: String,
    pub next_run_at: Option<DateTime<Utc>>,
    pub agent_type: String,
    pub project_id: Option<i32>,
    pub project_name: Option<String>,
    pub prompt: String,
    pub last_run_at: Option<DateTime<Utc>>,
    pub last_run_status: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ScheduledTaskProjectView {
    pub project_id: i32,
    pub name: String,
}

fn default_timezone() -> String {
    "UTC".to_string()
}

const fn default_enabled() -> bool {
    true
}
