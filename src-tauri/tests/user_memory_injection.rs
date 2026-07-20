use std::collections::BTreeMap;
use std::sync::Arc;

use iyw_claw_lib::acp::connection::ConnectionCommand;
use iyw_claw_lib::acp::manager::ConnectionManager;
use iyw_claw_lib::acp::types::PromptInputBlock;
use iyw_claw_lib::models::agent::AgentType;
use iyw_claw_lib::user_memory::{
    UserMemoryDocumentId, UserMemoryDocumentPatch, UserMemoryService, UserMemoryUpdateRequest,
    USER_CONTEXT_START,
};
use iyw_claw_lib::web::event_bridge::EventEmitter;

async fn configured_service() -> (tempfile::TempDir, Arc<UserMemoryService>) {
    let temp = tempfile::tempdir().unwrap();
    let db = iyw_claw_lib::db::init_database(temp.path(), "test")
        .await
        .unwrap();
    let service = Arc::new(UserMemoryService::new(db.conn, temp.path().join("memory")));
    let before = service.snapshot().await.unwrap();
    let memory = &before.documents[&UserMemoryDocumentId::Memory];
    service
        .update(UserMemoryUpdateRequest {
            expected_revision: before.revision,
            documents: BTreeMap::from([(
                UserMemoryDocumentId::Memory,
                UserMemoryDocumentPatch {
                    content: Some("Uses Simplified Chinese.".into()),
                    expected_etag: Some(memory.etag.clone()),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        })
        .await
        .unwrap();
    (temp, service)
}

#[tokio::test]
async fn first_accepted_prompt_gets_context_once_and_keeps_original_blocks() {
    let (_temp, service) = configured_service().await;
    let manager = ConnectionManager::new();
    manager.install_user_memory(service);
    let mut receiver = manager
        .insert_test_connection_live("memory-once", AgentType::Codex, None, EventEmitter::Noop)
        .await;

    manager
        .send_prompt(
            "memory-once",
            vec![PromptInputBlock::Text {
                text: "original prompt".into(),
            }],
        )
        .await
        .unwrap();
    let first = receiver.recv().await.unwrap();
    let ConnectionCommand::Prompt {
        blocks,
        user_context,
        ..
    } = first
    else {
        panic!("expected prompt")
    };
    assert!(user_context.unwrap().contains(USER_CONTEXT_START));
    assert!(matches!(
        blocks.as_slice(),
        [PromptInputBlock::Text { text }] if text == "original prompt"
    ));

    manager
        .get_state("memory-once")
        .await
        .unwrap()
        .write()
        .await
        .turn_in_flight = false;
    manager
        .send_prompt(
            "memory-once",
            vec![PromptInputBlock::Text {
                text: "follow-up".into(),
            }],
        )
        .await
        .unwrap();
    let second = receiver.recv().await.unwrap();
    let ConnectionCommand::Prompt { user_context, .. } = second else {
        panic!("expected prompt")
    };
    assert!(user_context.is_none());
}

#[tokio::test]
async fn rejected_empty_prompt_does_not_consume_initial_context() {
    let (_temp, service) = configured_service().await;
    let manager = ConnectionManager::new();
    manager.install_user_memory(service);
    let mut receiver = manager
        .insert_test_connection_live(
            "memory-after-empty",
            AgentType::Codex,
            None,
            EventEmitter::Noop,
        )
        .await;

    assert!(manager
        .send_prompt("memory-after-empty", Vec::new())
        .await
        .is_err());
    manager
        .send_prompt(
            "memory-after-empty",
            vec![PromptInputBlock::Text {
                text: "real".into(),
            }],
        )
        .await
        .unwrap();
    let ConnectionCommand::Prompt { user_context, .. } = receiver.recv().await.unwrap() else {
        panic!("expected prompt")
    };
    assert!(user_context.is_some());
}

#[tokio::test]
async fn resumed_session_does_not_receive_a_second_private_context() {
    let (_temp, service) = configured_service().await;
    let manager = ConnectionManager::new();
    manager.install_user_memory(service);
    let mut receiver = manager
        .insert_test_resumed_connection_live(
            "memory-resumed",
            AgentType::Codex,
            None,
            EventEmitter::Noop,
        )
        .await;

    manager
        .send_prompt(
            "memory-resumed",
            vec![PromptInputBlock::Text {
                text: "continue the existing conversation".into(),
            }],
        )
        .await
        .unwrap();
    let ConnectionCommand::Prompt { user_context, .. } = receiver.recv().await.unwrap() else {
        panic!("expected prompt")
    };

    assert!(user_context.is_none());
}
