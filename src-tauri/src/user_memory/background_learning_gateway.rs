use sea_orm::{ConnectionTrait, DbBackend, Statement};

use super::UserMemoryService;
use crate::acp::model_gateway_chat::ModelGatewayChatConfig;
use crate::acp::provider_overlay::model_gateway_base_url_for;
use crate::app_error::AppCommandError;

const MODEL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

impl UserMemoryService {
    pub(super) async fn learning_gateway(
        &self,
        model: &str,
    ) -> Result<ModelGatewayChatConfig, AppCommandError> {
        if !supports_structured_output(model) {
            return Err(AppCommandError::configuration_invalid(
                "Memory learning requires an available model with structured output support",
            ));
        }
        let token = crate::commands::iyw_account::iyw_account_access_token_core(&self.db)
            .await?
            .ok_or_else(|| {
                AppCommandError::configuration_missing("Sign in to use background memory learning")
            })?;
        let base = model_gateway_base_url_for(crate::models::agent::AgentType::Codex);
        let api_url =
            crate::chat_channel::natural_router_config::normalize_chat_completions_url(&base)?;
        Ok(ModelGatewayChatConfig {
            api_url,
            api_key: token.expose().to_string(),
            model: model.to_string(),
            timeout: MODEL_TIMEOUT,
        })
    }

    pub(super) async fn is_user_conversation(
        &self,
        conversation: &str,
    ) -> Result<bool, AppCommandError> {
        let Ok(id) = conversation.parse::<i32>() else {
            return Ok(false);
        };
        let row = self
            .db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "SELECT c.id FROM conversation c WHERE c.id = ? AND c.parent_id IS NULL AND c.deleted_at IS NULL AND NOT EXISTS (SELECT 1 FROM automation_run a WHERE a.conversation_id = c.id)",
                [id.into()],
            ))
            .await
            .map_err(super::index_checkpoint::database_error)?;
        Ok(row.is_some())
    }
}

pub(super) fn supports_structured_output(model: &str) -> bool {
    crate::acp::model_catalog::model_capabilities(model)
        .is_some_and(|value| value.capabilities.structured_output)
}

pub(super) fn structured_model_options() -> Vec<crate::acp::model_catalog::ModelOption> {
    crate::acp::model_catalog::all_model_options()
        .into_iter()
        .filter(|model| supports_structured_output(&model.id))
        .collect()
}

pub(super) fn resolve_learning_model(configured: &str) -> Option<String> {
    let models = structured_model_options();
    models
        .iter()
        .find(|model| model.id == configured.trim())
        .or_else(|| models.first())
        .map(|model| model.id.clone())
}

pub(super) fn preserve_provider_error(message: &str, error: AppCommandError) -> AppCommandError {
    let code = error.code;
    let detail = error.detail.unwrap_or(error.message);
    AppCommandError::new(code, message)
        .with_detail(format!("provider_error_code={code:?}; {detail}"))
}
