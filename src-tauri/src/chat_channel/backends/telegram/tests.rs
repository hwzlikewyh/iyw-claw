use super::*;
use crate::chat_channel::backends::telegram::api::redact_token;
use crate::chat_channel::backends::telegram::format::{
    inline_keyboard, message_chat_matches, message_target, send_message_body, should_process_text,
    topic_title,
};

#[test]
fn token_is_redacted_from_transport_error_urls() {
    let token = "123456:AAExampleSecretToken";
    let leaked = format!(
        "error sending request for url (https://api.telegram.org/bot{token}/createForumTopic)"
    );
    let scrubbed = redact_token(leaked, token);
    assert!(!scrubbed.contains(token));
    assert!(scrubbed.contains("bot***/createForumTopic"));
}

#[test]
fn empty_token_redaction_is_a_noop() {
    let message = "some error without any token".to_string();
    assert_eq!(redact_token(message.clone(), ""), message);
}

#[tokio::test]
async fn numeric_chat_id_resolves_without_network() {
    let backend = TelegramBackend::new(1, "token".into(), "-100123".into(), true);
    assert_eq!(backend.canonical_chat_id().await.unwrap(), "-100123");
}

#[test]
fn chat_filter_supports_numeric_ids_and_case_insensitive_usernames() {
    let message = serde_json::json!({
        "chat": { "id": -100123, "username": "IywTopics", "type": "supergroup" }
    });
    assert!(message_chat_matches(&message, "-100123"));
    assert!(message_chat_matches(&message, "@iywtopics"));
    assert!(message_chat_matches(&message, "IYWTOPICS"));
    assert!(!message_chat_matches(&message, "-100456"));
}

#[test]
fn target_parser_distinguishes_legacy_general_and_forum_targets() {
    let topic = serde_json::json!({
        "chat": { "id": -100123 },
        "message_thread_id": 2
    });
    assert_eq!(
        message_target(7, "-100123", false, &topic),
        ChannelMessageTarget::channel(7)
    );
    assert_eq!(
        message_target(
            7,
            "-100123",
            true,
            &serde_json::json!({ "chat": { "id": -100123 } })
        ),
        ChannelMessageTarget::telegram_general(7, "-100123")
    );
    assert_eq!(
        message_target(7, "-100123", true, &topic),
        ChannelMessageTarget::telegram_forum_topic(7, "-100123", "2")
    );
}

#[test]
fn topic_mode_allows_followups_without_bot_mentions() {
    assert!(should_process_text(
        "supergroup",
        "plain follow-up",
        "iyw_bot",
        true
    ));
    assert!(!should_process_text(
        "supergroup",
        "/task build",
        "iyw_bot",
        false
    ));
    assert!(should_process_text(
        "supergroup",
        "/task@iyw_bot build",
        "iyw_bot",
        false
    ));
}

#[test]
fn send_body_targets_the_forum_topic_and_rejects_invalid_ids() {
    let target = ChannelMessageTarget::telegram_forum_topic(7, "-100123", "42");
    let body = send_message_body(
        "fallback",
        "hello",
        Some("MarkdownV2"),
        Some(&target),
        Some(serde_json::json!({ "inline_keyboard": [] })),
    )
    .expect("body");
    assert_eq!(body["chat_id"], "-100123");
    assert_eq!(body["message_thread_id"], 42);
    assert_eq!(body["parse_mode"], "MarkdownV2");

    let invalid = ChannelMessageTarget::telegram_forum_topic(7, "-100123", "bad");
    assert!(send_message_body("fallback", "hello", None, Some(&invalid), None).is_err());
}

#[test]
fn inline_keyboard_uses_two_button_rows() {
    let button = |id: &str| MessageButton {
        id: id.to_string(),
        label: id.to_string(),
        style: ButtonStyle::Default,
    };
    let message = InteractiveMessage {
        base: RichMessage::info("Pick"),
        buttons: vec![button("one"), button("two"), button("three")],
        callback_context: serde_json::json!({}),
    };
    let keyboard = inline_keyboard(&message).expect("keyboard");
    assert_eq!(keyboard["inline_keyboard"].as_array().unwrap().len(), 2);
    assert_eq!(keyboard["inline_keyboard"][1][0]["callback_data"], "three");
}

#[test]
fn empty_topic_title_uses_the_local_brand() {
    assert_eq!(topic_title(""), "iyw-claw session");
    assert_eq!(topic_title(&"x".repeat(140)).chars().count(), 128);
}
