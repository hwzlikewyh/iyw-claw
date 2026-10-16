use crate::chat_channel::types::*;
use serde_json::{json, Value};

pub(super) fn card(message: &InteractiveMessage) -> Value {
    let buttons = message
        .buttons
        .iter()
        .take(6)
        .map(|button| {
            json!({
                "tag":"button","text":{"tag":"plain_text","content":button.label},
                "type":"default","value":{"iyw_action":button.id}
            })
        })
        .collect::<Vec<_>>();
    let mut elements = vec![json!({"tag":"markdown","content":message.base.to_plain_text()})];
    if !buttons.is_empty() {
        elements.push(json!({"tag":"action","actions":buttons}));
    }
    json!({"config":{"wide_screen_mode":true},
        "header":{"title":{"tag":"plain_text","content":message.base.title.as_deref().unwrap_or("任务通知")}},
        "elements":elements})
}

pub(super) fn callback(event: &Value, channel_id: i32) -> Option<IncomingCommand> {
    if event.pointer("/header/event_type")?.as_str()? != "card.action.trigger" {
        return None;
    }
    let body = event.get("event")?;
    let key = body.pointer("/action/value/iyw_action")?.as_str()?;
    if !key.starts_with("iywt:") {
        return None;
    }
    let sender_id = body.pointer("/operator/open_id")?.as_str()?;
    let chat_id = body.pointer("/context/open_chat_id")?.as_str()?;
    let event_id = event.pointer("/header/event_id")?.as_str()?;
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
            thread_kind: Some("lark_chat".into()),
            provider_payload: None,
        },
        metadata: json!({}),
        message_trace_id: crate::chat_channel::dedupe::new_message_trace_id(channel_id),
        provider_message_id: Some(format!("card:{event_id}")),
        received_at: chrono::Utc::now(),
    })
}
