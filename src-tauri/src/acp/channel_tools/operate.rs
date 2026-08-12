use serde_json::{json, Value};

use super::service::{ChannelToolService, MutationStart};
use super::types::{ChannelCaller, ChannelOperation, OperateChannelInput};
use crate::commands::chat_channel;

impl ChannelToolService {
    pub(super) async fn operate_channel(
        &self,
        caller: &ChannelCaller,
        input: Result<OperateChannelInput, String>,
    ) -> Result<Value, String> {
        let input = input?;
        let digest = json!({ "channel_id": input.channel_id, "operation": input.operation });
        let start = self
            .begin_mutation(caller, "operate_message_channel", &input.request_id, digest)
            .await?;
        let MutationStart::Started(model) = start else {
            return match start {
                MutationStart::Return(value) => Ok(value),
                _ => unreachable!(),
            };
        };
        let result = self
            .operate_inner(&input)
            .await
            .unwrap_or_else(super::service::error_value);
        self.finish_mutation(
            caller,
            "operate_message_channel",
            &input.request_id,
            model,
            result,
            Some(input.channel_id),
        )
        .await
    }

    async fn operate_inner(&self, input: &OperateChannelInput) -> Result<Value, String> {
        match input.operation {
            ChannelOperation::Connect => {
                chat_channel::connect_chat_channel_core(&self.db, &self.manager, input.channel_id)
                    .await
                    .map_err(|_| "CHANNEL_CONNECT_FAILED".to_string())?;
                Ok(json!({ "status": "connected", "channel_id": input.channel_id }))
            }
            ChannelOperation::Disconnect => {
                chat_channel::disconnect_chat_channel_core(
                    &self.db,
                    &self.manager,
                    input.channel_id,
                )
                .await
                .map_err(|_| "CHANNEL_DISCONNECT_FAILED".to_string())?;
                Ok(json!({ "status": "disconnected", "channel_id": input.channel_id }))
            }
            ChannelOperation::QuickCheck => diagnostic(
                chat_channel::quick_check_chat_channel_core(
                    &self.db,
                    &self.manager,
                    input.channel_id,
                )
                .await
                .map_err(|_| "CHANNEL_DIAGNOSTIC_FAILED".to_string())?,
            ),
            ChannelOperation::FullLoop => diagnostic(
                chat_channel::full_loop_chat_channel_core(
                    &self.db,
                    &self.manager,
                    input.channel_id,
                )
                .await
                .map_err(|_| "CHANNEL_DIAGNOSTIC_FAILED".to_string())?,
            ),
        }
    }
}

fn diagnostic(value: crate::chat_channel::diagnostics::ChannelDiagnostic) -> Result<Value, String> {
    Ok(json!({
        "status": "completed",
        "channel_id": value.channel_id,
        "kind": value.kind,
        "readiness": value.readiness,
        "roundtrip_verified": value.roundtrip.as_ref().is_some_and(|item| item.verified),
    }))
}
