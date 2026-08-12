use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use super::authorization::AuthorizationRegistry;
use super::confirmation::ChannelConfirmationSpec;
use super::idempotency::{self, BeginOutcome};
use super::types::*;
use crate::chat_channel::manager::ChatChannelManager;
use crate::db::entities::chat_channel_tool_request;
use crate::db::service::chat_channel_tool_audit_service::{self, AuditRecord};
use crate::db::AppDatabase;

pub struct ChannelToolService {
    pub(super) db: Arc<AppDatabase>,
    pub(super) manager: ChatChannelManager,
    pub(super) authorizations: AuthorizationRegistry,
}

pub(super) enum MutationStart {
    Started(chat_channel_tool_request::Model),
    Return(Value),
}

impl ChannelToolService {
    pub fn new(db: Arc<AppDatabase>, manager: ChatChannelManager) -> Self {
        Self {
            db,
            manager,
            authorizations: AuthorizationRegistry::default(),
        }
    }

    pub async fn execute(&self, caller: ChannelCaller, tool: &str, input: Value) -> Value {
        let result = match tool {
            "list_message_channels" => self.list_channels(parse(input)).await,
            "save_message_channel" => self.save_channel(&caller, parse(input)).await,
            "manage_channel_credential" => self.manage_credential(&caller, parse(input)).await,
            "operate_message_channel" => self.operate_channel(&caller, parse(input)).await,
            "list_channel_targets" => self.list_targets(parse(input)).await,
            "list_channel_messages" => self.list_messages(parse(input)).await,
            "send_channel_messages" => self.send_messages(caller, parse(input)).await,
            "manage_channel_settings" => self.manage_settings(&caller, parse(input)).await,
            "delete_message_channel" => Err("CONFIRMATION_REQUIRED".to_string()),
            _ => Err("UNKNOWN_CHANNEL_TOOL".to_string()),
        };
        result.unwrap_or_else(error_value)
    }

    pub async fn prepare_confirmation(
        &self,
        tool: &str,
        input: &Value,
    ) -> Result<ChannelConfirmationSpec, String> {
        super::confirmation_prepare::prepare_confirmation(&self.db, tool, input).await
    }

    pub async fn execute_confirmed(
        &self,
        caller: ChannelCaller,
        tool: &str,
        input: Value,
        expected_version: &str,
    ) -> Value {
        let current = self.prepare_confirmation(tool, &input).await;
        if current.as_ref().map(|spec| spec.resource_version.as_str()) != Ok(expected_version) {
            return error_value("CONFIRMATION_STALE");
        }
        let result = match tool {
            "delete_message_channel" => self.delete_channel(&caller, parse(input)).await,
            "manage_channel_credential" => {
                self.manage_credential_confirmed(&caller, parse(input))
                    .await
            }
            _ => Err("CONFIRMATION_NOT_APPLICABLE".to_string()),
        };
        result.unwrap_or_else(error_value)
    }

    pub async fn cancel_request(&self, caller: &ChannelCaller, tool: &str, input: &Value) {
        let Some(request_id) = input.get("request_id").and_then(Value::as_str) else {
            return;
        };
        let canceled = idempotency::cancel(&self.db, &caller.caller_scope, tool, request_id)
            .await
            .unwrap_or(false);
        if !canceled {
            return;
        }
        let record = AuditRecord {
            agent_type: &caller.agent_type,
            session_ref: &caller.session_ref,
            operation: tool,
            channel_id: input
                .get("channel_id")
                .and_then(Value::as_i64)
                .and_then(|value| i32::try_from(value).ok()),
            target_id: None,
            target_label: None,
            file_summary_json: None,
            status: "failed",
            error_code: Some("REQUEST_CANCELED"),
            request_id,
        };
        let _ = chat_channel_tool_audit_service::create(&self.db.conn, record).await;
    }

    pub fn requires_confirmation(tool: &str, input: &Value) -> bool {
        if tool == "delete_message_channel" {
            return true;
        }
        tool == "manage_channel_credential"
            && input.get("operation").and_then(Value::as_str) == Some("delete")
    }

    pub(super) async fn begin_mutation(
        &self,
        caller: &ChannelCaller,
        operation: &str,
        request_id: &str,
        safe_digest: Value,
    ) -> Result<MutationStart, String> {
        match idempotency::begin(
            &self.db,
            &caller.caller_scope,
            operation,
            request_id,
            &safe_digest,
        )
        .await?
        {
            BeginOutcome::Started(model) => Ok(MutationStart::Started(model)),
            BeginOutcome::Cached(value) => Ok(MutationStart::Return(value)),
            BeginOutcome::Processing => Ok(MutationStart::Return(json!({
                "status": "processing",
                "request_id": request_id,
            }))),
        }
    }

    pub(super) async fn finish_mutation(
        &self,
        caller: &ChannelCaller,
        operation: &str,
        request_id: &str,
        model: chat_channel_tool_request::Model,
        mut result: Value,
        channel_id: Option<i32>,
    ) -> Result<Value, String> {
        idempotency::finish(&self.db, model, &result).await?;
        let error = result.get("error").and_then(Value::as_str);
        let record = AuditRecord {
            agent_type: &caller.agent_type,
            session_ref: &caller.session_ref,
            operation,
            channel_id,
            target_id: None,
            target_label: None,
            file_summary_json: None,
            status: if error.is_some() {
                "failed"
            } else {
                "completed"
            },
            error_code: error,
            request_id,
        };
        if chat_channel_tool_audit_service::create(&self.db.conn, record)
            .await
            .is_err()
        {
            tracing::error!(operation, request_id, "channel tool audit write failed");
            if let Some(object) = result.as_object_mut() {
                object.insert("audit_error".to_string(), json!("AUDIT_UNAVAILABLE"));
            }
        }
        Ok(result)
    }
}

fn parse<T: DeserializeOwned>(input: Value) -> Result<T, String> {
    serde_json::from_value(input).map_err(|_| "INVALID_INPUT".to_string())
}

pub(super) fn error_value(code: impl Into<String>) -> Value {
    json!({ "error": code.into() })
}
