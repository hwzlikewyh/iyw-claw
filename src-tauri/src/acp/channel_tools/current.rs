use super::{ChannelCaller, ChannelToolService};
use crate::chat_channel::task_context::TaskContext;
use crate::chat_channel::{task_context, task_progress};
use serde_json::{json, Value};

impl ChannelToolService {
    pub(crate) async fn execute_current(
        &self,
        caller: ChannelCaller,
        task: TaskContext,
        request: (&str, Value),
    ) -> Value {
        let result = self.current_inner(caller, &task, request).await;
        result.unwrap_or_else(|error| json!({"error":error}))
    }

    async fn current_inner(
        &self,
        caller: ChannelCaller,
        task: &TaskContext,
        request: (&str, Value),
    ) -> Result<Value, String> {
        let (operation, input) = request;
        self.validate_current(task).await?;
        match operation {
            "context" => Ok(json!({
                "context_id":task.id, "channel_type":task.origin.channel_type,
                "capabilities":crate::chat_channel::media_capabilities::view(&task.origin.channel_type.to_string()),
                "progress":task_progress::view(task).await,
                "default_recipient":"current_conversation", "credentials":"host_managed",
                "native_buttons":matches!(task.origin.channel_type,
                    crate::chat_channel::types::ChannelType::Lark | crate::chat_channel::types::ChannelType::WecomAiBot),
                "streaming":task.origin.channel_type == crate::chat_channel::types::ChannelType::WecomAiBot,
            })),
            "progress" => Ok(task_progress::view(task).await),
            "reporting" => {
                task_context::configure(
                    &task.id,
                    input["interval_seconds"].as_u64(),
                    input["enabled"].as_bool(),
                )?;
                Ok(
                    json!({"status":"updated", "progress":task_progress::view(&task_context::current(&task.connection_id).ok_or("CHANNEL_CONTEXT_STALE")?).await}),
                )
            }
            "milestone" => {
                task_progress::record_summary(
                    &task.connection_id,
                    input["summary"].as_str().ok_or("SUMMARY_REQUIRED")?,
                )?;
                Ok(json!({"status":"recorded","source":"agent_reported"}))
            }
            "send" => self.send_current(caller, task, input).await,
            "card" => self.send_task_card(caller, task, input).await,
            "stream" => self.send_stream(caller, task, input).await,
            "business" => self.wecom_business(&caller, task, input).await,
            _ => Err("UNKNOWN_CURRENT_CHANNEL_OPERATION".into()),
        }
    }

    async fn send_stream(
        &self,
        caller: ChannelCaller,
        task: &TaskContext,
        input: Value,
    ) -> Result<Value, String> {
        let request_id = input["request_id"].as_str().ok_or("REQUEST_ID_REQUIRED")?;
        let start = self
            .begin_mutation(&caller, "current_channel_stream", request_id, input.clone())
            .await?;
        let model = match start {
            super::service::MutationStart::Return(value) => return Ok(value),
            super::service::MutationStart::Started(model) => model,
        };
        let content = input["content"].as_str().ok_or("STREAM_CONTENT_REQUIRED")?;
        let result = crate::chat_channel::streaming::update(
            &self.manager,
            task,
            (content, input["finished"].as_bool().unwrap_or(false)),
        )
        .await;
        let result = match result {
            Ok(receipt) => json!({"status":"sent","delivery_receipt":receipt.0}),
            Err(error) => json!({"error":error,"status":"unknown"}),
        };
        self.finish_mutation(
            &caller,
            "current_channel_stream",
            request_id,
            model,
            result,
            Some(task.origin.route.channel_id),
        )
        .await
    }

    async fn send_current(
        &self,
        caller: ChannelCaller,
        task: &TaskContext,
        input: Value,
    ) -> Result<Value, String> {
        let arguments = json!({"request_id":input["request_id"], "items":[{
            "channel_id":task.origin.route.channel_id,"target_id":task.origin.route.target_id,
            "text":input.get("text"), "files":input.get("files").cloned().unwrap_or_else(|| json!([]))
        }]});
        self.send_messages(
            caller,
            serde_json::from_value(arguments).map_err(|_| "INVALID_INPUT".to_string()),
        )
        .await
    }

    async fn send_task_card(
        &self,
        caller: ChannelCaller,
        task: &TaskContext,
        input: Value,
    ) -> Result<Value, String> {
        let request_id = input["request_id"].as_str().ok_or("REQUEST_ID_REQUIRED")?;
        let mutation = self
            .begin_mutation(&caller, "current_channel_card", request_id, input.clone())
            .await?;
        let model = match mutation {
            super::service::MutationStart::Started(model) => model,
            super::service::MutationStart::Return(value) => return Ok(value),
        };
        let card = crate::chat_channel::interactions::with_text_actions(
            task,
            crate::chat_channel::interactions::task_card(
                task,
                input["title"].as_str().unwrap_or("任务"),
                input["body"].as_str().unwrap_or_default(),
            ),
        );
        let outcome = match self.validate_current(task).await {
            Ok(()) => self
                .manager
                .send_interactive_to_target(&task.origin.route.target, &card)
                .await
                .map_err(|error| error.category().to_string()),
            Err(error) => Err(error),
        };
        let result = match outcome {
            Ok(receipt) => json!({"status":"sent","delivery_receipt":receipt.0}),
            Err(error) => json!({"error":error,"status":"unknown"}),
        };
        self.finish_mutation(
            &caller,
            "current_channel_card",
            request_id,
            model,
            result,
            Some(task.origin.route.channel_id),
        )
        .await
    }

    async fn validate_current(&self, task: &TaskContext) -> Result<(), String> {
        task_progress::validate_execution(task).await?;
        let channel = crate::db::service::chat_channel_service::get_by_id(
            &self.db.conn,
            task.origin.route.channel_id,
        )
        .await
        .map_err(|_| "CHANNEL_QUERY_FAILED")?
        .ok_or("CHANNEL_NOT_FOUND")?;
        if !channel.enabled || channel.channel_type != task.origin.channel_type.to_string() {
            return Err("CHANNEL_CONTEXT_STALE".into());
        }
        let (_, target) = crate::db::service::chat_channel_target_service::resolve(
            &self.db.conn,
            task.origin.route.channel_id,
            &task.origin.route.target_id,
        )
        .await
        .map_err(|_| "TARGET_NOT_FOUND")?;
        if !target.matches_thread(&task.origin.route.target) {
            return Err("TARGET_CONTEXT_CHANGED".into());
        }
        task_progress::validate_execution(task).await
    }
}
