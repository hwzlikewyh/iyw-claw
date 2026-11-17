use serde_json::Value;

use super::service::ChannelToolService;
use super::types::ChannelCaller;
use crate::chat_channel::reply_routes::ReplySource;
use crate::chat_channel::types::ChannelMessageTarget;
use crate::db::service::chat_channel_service;

impl ChannelToolService {
    pub(crate) async fn execute_from_connection(
        &self,
        caller: ChannelCaller,
        request: (&str, Value),
        source: Option<ReplySource>,
    ) -> Value {
        let (tool, input) = request;
        if tool != "send_channel_messages" {
            return self.execute(caller, tool, input).await;
        }
        let input = serde_json::from_value(input).map_err(|_| "INVALID_INPUT".to_string());
        self.send_messages_with_source(caller, input, source)
            .await
            .unwrap_or_else(super::service::error_value)
    }

    pub(super) async fn prepare_reply_route(
        &self,
        source: Option<&ReplySource>,
        target: (&str, &ChannelMessageTarget),
    ) -> Result<Option<String>, String> {
        let Some(source) = source else {
            return Ok(None);
        };
        let channel = chat_channel_service::get_by_id(&self.db.conn, target.1.channel_id)
            .await
            .map_err(|error| {
                tracing::error!(channel_id = target.1.channel_id, error = %error,
                    "[channel-reply] failed to load reply channel");
                "CHANNEL_QUERY_FAILED".to_string()
            })?
            .ok_or_else(|| "CHANNEL_NOT_FOUND".to_string())?;
        // 微信私聊的接收人就是后续回答者；群聊不能据此推断回答者身份。
        if channel.channel_type != "weixin" {
            return Ok(None);
        }
        if !channel.enabled {
            return Err("CHANNEL_DISABLED".to_string());
        }
        self.manager
            .reply_routes()
            .register(source, target)
            .map(Some)
            .ok_or_else(|| "CHANNEL_REPLY_UNAVAILABLE".to_string())
    }
}
