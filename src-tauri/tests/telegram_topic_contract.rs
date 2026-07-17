use iyw_claw_lib::chat_channel::types::{ChannelMessageTarget, ChannelType};
use iyw_claw_lib::db::service::{chat_channel_service, thread_binding_service};
use iyw_claw_lib::db::test_helpers::{fresh_in_memory_db, seed_conversation, seed_folder};
use iyw_claw_lib::models::AgentType;

#[test]
fn telegram_channel_and_topic_targets_round_trip() {
    assert_eq!(
        serde_json::to_value(ChannelType::Telegram).unwrap(),
        serde_json::json!("telegram")
    );

    let general = ChannelMessageTarget::telegram_general(7, "-100123");
    let topic = ChannelMessageTarget::telegram_forum_topic(7, "-100123", "42");
    assert!(general.is_telegram_general_topic());
    assert!(!general.is_telegram_forum_topic());
    assert!(topic.is_telegram_forum_topic());
    assert!(!topic.matches_thread(&general));

    let encoded = serde_json::to_value(&topic).unwrap();
    let decoded: ChannelMessageTarget = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, topic);
}

#[tokio::test]
async fn thread_binding_upsert_reuses_the_topic_identity() {
    let db = fresh_in_memory_db().await;
    let channel = chat_channel_service::create(
        &db.conn,
        "Telegram".into(),
        "telegram".into(),
        r#"{"chat_id":"-100123","topic_mode":true}"#.into(),
        true,
        false,
        None,
    )
    .await
    .expect("seed channel");
    let folder = seed_folder(&db, "/tmp/iyw-claw-telegram-topic").await;
    let conversation = seed_conversation(&db, folder, AgentType::Codex).await;
    let target = ChannelMessageTarget::telegram_forum_topic(channel.id, "-100123", "42");

    let first = thread_binding_service::upsert_for_target(
        &db.conn,
        thread_binding_service::ThreadBindingUpsert {
            target: &target,
            channel_type: "telegram",
            conversation_id: conversation,
            connection_id: Some("conn-1".into()),
            created_by_sender_id: "sender-1",
            display_title: Some("Initial".into()),
        },
    )
    .await
    .expect("insert binding");
    let updated = thread_binding_service::upsert_for_target(
        &db.conn,
        thread_binding_service::ThreadBindingUpsert {
            target: &target,
            channel_type: "telegram",
            conversation_id: conversation,
            connection_id: Some("conn-2".into()),
            created_by_sender_id: "sender-2",
            display_title: Some("Updated".into()),
        },
    )
    .await
    .expect("update binding");

    assert_eq!(updated.id, first.id);
    assert_eq!(updated.connection_id.as_deref(), Some("conn-2"));
    assert_eq!(updated.display_title.as_deref(), Some("Updated"));
    assert_eq!(
        thread_binding_service::get_by_target(&db.conn, &target)
            .await
            .expect("lookup")
            .expect("binding")
            .id,
        first.id
    );
}
