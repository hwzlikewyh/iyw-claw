mod helpers;
mod runtime;
mod types;

use std::path::PathBuf;
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::commands::automation::{
    automation_create_core, automation_delete_core, automation_get_core, automation_list_core,
    automation_update_core,
};
use crate::db::entities::automation::{IsolationMode, TriggerKind};
use crate::db::service::folder_service;
use crate::db::AppDatabase;
use crate::models::{AutomationDraft, FolderDetail};
use crate::web::event_bridge::EventEmitter;

use helpers::*;

pub use runtime::{
    inject_scheduled_task_env, install_scheduled_task_runtime, scheduled_task_cli_path,
};
use types::{CreateInput, DeleteInput, ListInput, UpdateInput};
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
        tracing::info!(
            operation = ?request.operation,
            caller_agent_type = ?request.caller_agent_type,
            "[automation-tool] request received"
        );
        let result = match request.operation {
            ScheduledTaskOperation::List => self.list(parse(request.input)?).await,
            ScheduledTaskOperation::Create => {
                self.create(parse(request.input)?, request.caller_agent_type)
                    .await
            }
            ScheduledTaskOperation::Update => self.update(parse(request.input)?).await,
            ScheduledTaskOperation::Delete => self.delete(parse(request.input)?).await,
        };
        if let Err(error) = &result {
            tracing::warn!(operation = ?request.operation, error = %error, "[automation-tool] request failed");
        }
        result
    }

    async fn list(&self, input: ListInput) -> Result<Value, String> {
        let folders = self.folders().await?;
        let project_id = input
            .project
            .as_deref()
            .map(|project| resolve_project(&folders, project))
            .transpose()?;
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
            has_project_filter = input.project.is_some(),
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
        let folder_id = resolve_project(&folders, &input.project)?;
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
            root_folder_id: Some(folder_id),
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
            folder_id,
            agent_type = %task.agent_type,
            enabled = task.enabled,
            "[automation-tool] task created"
        );
        Ok(json!({ "task": task_view(task, &folders) }))
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
    serde_json::from_value(input).map_err(|error| format!("invalid input: {error}"))
}
