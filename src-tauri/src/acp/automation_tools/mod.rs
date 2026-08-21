mod helpers;
mod types;

pub const LIST_SCHEDULED_TASK_PROJECTS_TOOL: &str = "list_scheduled_task_projects";

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::commands::automation::{
    automation_create_core, automation_delete_core, automation_get_core, automation_list_core,
    automation_update_core,
};
use crate::db::entities::automation::{IsolationMode, TriggerKind};
use crate::db::entities::folder::FolderKind;
use crate::db::service::folder_service;
use crate::db::AppDatabase;
use crate::models::{AutomationDraft, FolderDetail};
use crate::web::event_bridge::EventEmitter;

use helpers::*;

use types::{
    CreateInput, DeleteInput, ListInput, ListProjectsInput, ScheduledTaskProjectView, UpdateInput,
};
pub use types::{ScheduledTaskOperation, ScheduledTaskRequest};

pub struct AutomationAgentService {
    db: Arc<AppDatabase>,
    emitter: EventEmitter,
    data_dir: PathBuf,
}

impl AutomationAgentService {
    pub fn new(db: Arc<AppDatabase>, emitter: EventEmitter, data_dir: PathBuf) -> Self {
        Self {
            db,
            emitter,
            data_dir,
        }
    }

    pub async fn execute(&self, request: ScheduledTaskRequest) -> Result<Value, String> {
        let request_id = uuid::Uuid::new_v4();
        let started_at = Instant::now();
        let operation = request.operation;
        tracing::info!(
            request_id = %request_id,
            operation = ?operation,
            caller_agent_type = ?request.caller_agent_type,
            "[automation-tool] request received"
        );
        let result = self.dispatch(request).await;
        let elapsed_ms = started_at.elapsed().as_millis();
        match &result {
            Ok(_) => tracing::info!(
                request_id = %request_id,
                operation = ?operation,
                elapsed_ms,
                "[automation-tool] request completed"
            ),
            Err(error) => tracing::warn!(
                request_id = %request_id,
                operation = ?operation,
                elapsed_ms,
                error = %error,
                "[automation-tool] request failed"
            ),
        }
        result
    }

    async fn dispatch(&self, request: ScheduledTaskRequest) -> Result<Value, String> {
        match request.operation {
            ScheduledTaskOperation::ListProjects => {
                self.list_projects(parse_empty(request.input)?).await
            }
            ScheduledTaskOperation::List => self.list(parse(request.input)?).await,
            ScheduledTaskOperation::Create => {
                self.create(parse(request.input)?, request.caller_agent_type)
                    .await
            }
            ScheduledTaskOperation::Update => self.update(parse(request.input)?).await,
            ScheduledTaskOperation::Delete => self.delete(parse(request.input)?).await,
        }
    }

    async fn list_projects(&self, _input: ListProjectsInput) -> Result<Value, String> {
        let mut projects = self
            .folders()
            .await?
            .into_iter()
            .filter(|folder| folder.kind == FolderKind::Regular)
            .map(|folder| ScheduledTaskProjectView {
                project_id: folder.id,
                name: folder.name,
            })
            .collect::<Vec<_>>();
        projects.sort_by(|left, right| {
            left.name
                .to_lowercase()
                .cmp(&right.name.to_lowercase())
                .then(left.project_id.cmp(&right.project_id))
        });
        tracing::info!(count = projects.len(), "[automation-tool] projects listed");
        Ok(json!({ "projects": projects }))
    }

    async fn list(&self, input: ListInput) -> Result<Value, String> {
        let folders = self.folders().await?;
        let project_id =
            resolve_project_selection(&folders, input.project.as_deref(), input.project_id)?;
        let agent_type = input
            .agent_type
            .as_deref()
            .map(normalize_agent_type)
            .transpose()?;
        let rows = if let Some(id) = input.task_id {
            vec![automation_get_core(&self.db, id).await.map_err(db_error)?]
        } else {
            automation_list_core(&self.db).await.map_err(db_error)?
        };
        let tasks = rows
            .into_iter()
            .filter(|task| task.trigger_kind == TriggerKind::Schedule)
            .filter(|task| input.enabled.is_none_or(|enabled| task.enabled == enabled))
            .filter(|task| project_id.is_none_or(|id| task.root_folder_id == Some(id)))
            .filter(|task| {
                agent_type
                    .as_ref()
                    .is_none_or(|kind| &task.agent_type == kind)
            })
            .map(|task| task_view(task, &folders))
            .collect::<Vec<_>>();
        tracing::info!(
            count = tasks.len(),
            task_id = ?input.task_id,
            enabled = ?input.enabled,
            has_project_filter = input.project.is_some() || input.project_id.is_some(),
            agent_type = ?agent_type,
            "[automation-tool] list completed"
        );
        Ok(json!({ "tasks": tasks }))
    }

    async fn create(
        &self,
        input: CreateInput,
        caller_agent_type: Option<String>,
    ) -> Result<Value, String> {
        require_text(&input.name, "name")?;
        require_text(&input.prompt, "prompt")?;
        require_text(&input.cron, "cron")?;
        let folders = self.folders().await?;
        let folder_id =
            resolve_project_selection(&folders, input.project.as_deref(), input.project_id)?;
        let raw_agent = input.agent_type.or(caller_agent_type).ok_or(
            "agent_type is required when the current Agent identity is unavailable".to_string(),
        )?;
        let draft = AutomationDraft {
            name: input.name,
            enabled: input.enabled,
            trigger_kind: TriggerKind::Schedule,
            cron: Some(input.cron),
            timezone: input.timezone,
            agent_type: normalize_agent_type(&raw_agent)?,
            root_folder_id: folder_id,
            isolation: IsolationMode::SharedInRoot,
            branch: None,
            is_remote_branch: false,
            config: prompt_config(input.prompt)?,
        };
        let task = automation_create_core(&self.emitter, &self.db, &self.data_dir, draft)
            .await
            .map_err(db_error)?;
        tracing::info!(
            task_id = task.id,
            folder_id = ?task.root_folder_id,
            agent_type = %task.agent_type,
            enabled = task.enabled,
            "[automation-tool] task created"
        );
        let result_folders = match self.folders().await {
            Ok(result_folders) => result_folders,
            Err(error) => {
                tracing::warn!(
                    task_id = task.id,
                    error = %error,
                    "[automation-tool] task project metadata refresh failed"
                );
                folders
            }
        };
        Ok(json!({ "task": task_view(task, &result_folders) }))
    }

    async fn update(&self, input: UpdateInput) -> Result<Value, String> {
        if input.patch.is_empty() {
            return Err("patch must include at least one field".to_string());
        }
        let current = automation_get_core(&self.db, input.task_id)
            .await
            .map_err(db_error)?;
        ensure_scheduled(&current)?;
        let folders = self.folders().await?;
        let draft = merge_patch(&folders, &current, input.patch)?;
        let task = automation_update_core(
            &self.emitter,
            &self.db,
            &self.data_dir,
            input.task_id,
            draft,
        )
        .await
        .map_err(db_error)?;
        tracing::info!(
            task_id = task.id,
            folder_id = ?task.root_folder_id,
            agent_type = %task.agent_type,
            enabled = task.enabled,
            "[automation-tool] task updated"
        );
        Ok(json!({ "task": task_view(task, &folders) }))
    }

    async fn delete(&self, input: DeleteInput) -> Result<Value, String> {
        let folders = self.folders().await?;
        let task = automation_get_core(&self.db, input.task_id)
            .await
            .map_err(db_error)?;
        ensure_scheduled(&task)?;
        let folder_id = task.root_folder_id;
        let deleted = task_view(task, &folders);
        automation_delete_core(&self.emitter, &self.db, input.task_id)
            .await
            .map_err(db_error)?;
        tracing::info!(
            task_id = input.task_id,
            folder_id = ?folder_id,
            "[automation-tool] task deleted"
        );
        Ok(json!({ "deleted": deleted }))
    }

    async fn folders(&self) -> Result<Vec<FolderDetail>, String> {
        folder_service::list_all_folder_details(&self.db.conn)
            .await
            .map_err(db_error)
    }
}

fn parse<T: DeserializeOwned>(input: Value) -> Result<T, String> {
    serde_json::from_value(input)
        .map_err(|_| "invalid input: request does not match the tool schema".to_string())
}

fn parse_empty<T: DeserializeOwned>(input: Value) -> Result<T, String> {
    parse(if input.is_null() { json!({}) } else { input })
}
