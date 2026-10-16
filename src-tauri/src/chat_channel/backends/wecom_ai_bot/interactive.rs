use super::{protocol, WecomAiBotBackend};
use crate::chat_channel::error::ChatChannelError;
use crate::chat_channel::traits::ChatChannelBackend;
use crate::chat_channel::types::*;
use serde_json::{json, Value};

impl WecomAiBotBackend {
    pub(super) async fn send_interactive(
        &self,
        message: &InteractiveMessage,
        target: &ChannelMessageTarget,
    ) -> Result<SentMessageId, ChatChannelError> {
        if message.buttons.is_empty() {
            return self.send_rich_message_to(&message.base, target).await;
        }
        if message.base.to_plain_text().chars().count() > 112 {
            self.send_rich_message_to(&message.base, target).await?;
        }
        let mut frame = message_frame(target, message)?;
        let buttons = message.buttons.iter().take(6).map(|button| json!({
            "text":button.label,"key":button.id,"style":match button.style {ButtonStyle::Primary => 1, _ => 2}
        })).collect::<Vec<_>>();
        frame["body"]["template_card"]["button_list"] = json!(buttons);
        self.send_frame(frame).await
    }
}

fn message_frame(
    target: &ChannelMessageTarget,
    message: &InteractiveMessage,
) -> Result<Value, ChatChannelError> {
    let chat_id = target
        .chat_id
        .as_deref()
        .ok_or_else(|| ChatChannelError::SendFailed("WeCom target missing".into()))?;
    let req_id = uuid::Uuid::new_v4().simple().to_string();
    let card = json!({
        "card_type":"button_interaction","task_id":req_id,
        "main_title":{"title":message.base.title.as_deref().unwrap_or("任务通知")},
        "sub_title_text":message.base.to_plain_text().chars().take(112).collect::<String>(),
    });
    Ok(
        json!({"cmd":"aibot_send_msg","headers":{"req_id":req_id},"body":{
            "chatid":chat_id,"chat_type":protocol::target_chat_type(target).unwrap_or(1),
            "msgtype":"template_card","template_card":card
        }}),
    )
}

pub(super) fn callback(frame: &Value, channel_id: i32) -> Option<IncomingCommand> {
    if frame["cmd"] != "aibot_event_callback" {
        return None;
    }
    let body = frame.get("body")?;
    let event = body.get("event").unwrap_or(body);
    if event["eventtype"] != "template_card_event" {
        return None;
    }
    let key = event
        .pointer("/template_card_event/event_key")
        .or_else(|| event.get("event_key"))?
        .as_str()?;
    if !key.starts_with("iywt:") {
        return None;
    }
    let sender_id = body
        .pointer("/from/userid")?
        .as_str()
        .filter(|id| !id.is_empty())?;
    let chat_id = if body["chattype"] == "group" {
        body["chatid"].as_str().filter(|id| !id.is_empty())?
    } else {
        body["chatid"]
            .as_str()
            .filter(|id| !id.is_empty())
            .unwrap_or(sender_id)
    };
    let req_id = frame.pointer("/headers/req_id")?.as_str()?;
    Some(IncomingCommand {
        channel_id,
        sender_id: sender_id.into(),
        sender_name: None,
        command_text: "卡片操作".into(),
        attachments: vec![],
        callback_data: Some(key.into()),
        target: ChannelMessageTarget {
            channel_id,
            chat_id: Some(chat_id.into()),
            thread_key: None,
            thread_kind: Some("wecom_ai_bot".into()),
            provider_payload: Some(json!({"req_id":req_id,"chat_type":body["chattype"]})),
        },
        metadata: json!({"chattype":body["chattype"]}),
        message_trace_id: crate::chat_channel::dedupe::new_message_trace_id(channel_id),
        provider_message_id: Some(format!(
            "event:{}",
            body["msgid"].as_str().unwrap_or(req_id)
        )),
        received_at: chrono::Utc::now(),
    })
}
