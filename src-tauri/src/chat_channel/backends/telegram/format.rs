use crate::chat_channel::error::ChatChannelError;
use crate::chat_channel::types::{
    ChannelMessageTarget, InteractiveMessage, MessageLevel, RichMessage,
};

pub(super) fn strip_bot_mention(text: &str, bot_username: &str) -> String {
    if bot_username.is_empty() {
        return text.to_string();
    }
    let mention = format!("@{bot_username}");
    let lower = text.to_lowercase();
    let mention_lower = mention.to_lowercase();
    let Some(position) = lower.find(&mention_lower) else {
        return text.to_string();
    };
    let mut result = String::with_capacity(text.len());
    result.push_str(&text[..position]);
    result.push_str(&text[position + mention.len()..]);
    result.trim().to_string()
}

pub(super) fn message_chat_matches(message: &serde_json::Value, configured_chat_id: &str) -> bool {
    let configured = configured_chat_id.trim();
    if configured.is_empty() {
        return false;
    }
    if message
        .pointer("/chat/id")
        .and_then(json_scalar_to_string)
        .as_deref()
        == Some(configured)
    {
        return true;
    }
    let username = configured.strip_prefix('@').unwrap_or(configured);
    message
        .pointer("/chat/username")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(username))
}

pub(super) fn message_target(
    channel_id: i32,
    configured_chat_id: &str,
    topic_mode: bool,
    message: &serde_json::Value,
) -> ChannelMessageTarget {
    if !topic_mode {
        return ChannelMessageTarget::channel(channel_id);
    }
    let chat_id = message
        .pointer("/chat/id")
        .and_then(json_scalar_to_string)
        .unwrap_or_else(|| configured_chat_id.to_string());
    match message
        .pointer("/message_thread_id")
        .and_then(json_scalar_to_string)
    {
        Some(thread_key) => {
            ChannelMessageTarget::telegram_forum_topic(channel_id, chat_id, thread_key)
        }
        None => ChannelMessageTarget::telegram_general(channel_id, chat_id),
    }
}

pub(super) fn should_process_text(
    chat_type: &str,
    text: &str,
    bot_username: &str,
    topic_mode: bool,
) -> bool {
    if topic_mode || bot_username.is_empty() || !matches!(chat_type, "group" | "supergroup") {
        return true;
    }
    text.to_lowercase()
        .contains(&format!("@{bot_username}").to_lowercase())
}

pub(super) fn json_scalar_to_string(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .map(ToString::to_string)
        .or_else(|| value.as_i64().map(|number| number.to_string()))
        .or_else(|| value.as_u64().map(|number| number.to_string()))
}

pub(super) fn topic_title(title: &str) -> String {
    let title = title.trim();
    let title = if title.is_empty() {
        "iyw-claw session"
    } else {
        title
    };
    title.chars().take(128).collect()
}

pub(super) fn inline_keyboard(message: &InteractiveMessage) -> Option<serde_json::Value> {
    if message.buttons.is_empty() {
        return None;
    }
    let rows = message
        .buttons
        .chunks(2)
        .map(|chunk| {
            serde_json::Value::Array(
                chunk
                    .iter()
                    .map(|button| {
                        serde_json::json!({
                            "text": button.label,
                            "callback_data": button.id,
                        })
                    })
                    .collect(),
            )
        })
        .collect::<Vec<_>>();
    Some(serde_json::json!({ "inline_keyboard": rows }))
}

pub(super) fn send_message_body(
    default_chat_id: &str,
    text: &str,
    parse_mode: Option<&str>,
    target: Option<&ChannelMessageTarget>,
    reply_markup: Option<serde_json::Value>,
) -> Result<serde_json::Value, ChatChannelError> {
    let chat_id = target
        .and_then(|value| value.chat_id.as_deref())
        .unwrap_or(default_chat_id);
    let mut body = serde_json::json!({ "chat_id": chat_id, "text": text });
    if let Some(mode) = parse_mode {
        body["parse_mode"] = mode.into();
    }
    if let Some(markup) = reply_markup {
        body["reply_markup"] = markup;
    }
    if let Some(thread_id) = forum_thread_id(target)? {
        body["message_thread_id"] = thread_id.into();
    }
    Ok(body)
}

fn forum_thread_id(target: Option<&ChannelMessageTarget>) -> Result<Option<i64>, ChatChannelError> {
    let Some(target) = target.filter(|value| value.is_telegram_forum_topic()) else {
        return Ok(None);
    };
    target
        .thread_key
        .as_deref()
        .and_then(|value| value.parse::<i64>().ok())
        .map(Some)
        .ok_or_else(|| {
            ChatChannelError::SendFailed("invalid Telegram message_thread_id target".to_string())
        })
}

pub(super) fn format_markdown(message: &RichMessage) -> String {
    let mut text = String::new();
    let level = match message.level {
        MessageLevel::Info => "INFO",
        MessageLevel::Warning => "WARNING",
        MessageLevel::Error => "ERROR",
    };
    if let Some(title) = &message.title {
        text.push_str(&format!("*{}: {}*\n", level, escape_markdown(title)));
    }
    text.push_str(&escape_markdown(&message.body));
    for (key, value) in &message.fields {
        text.push_str(&format!(
            "\n\n*{}*: {}",
            escape_markdown(key),
            escape_markdown(value)
        ));
    }
    text
}

fn escape_markdown(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if "\\_*[]()~`>#+-=|{}.!".contains(character) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}
