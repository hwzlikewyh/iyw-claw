use std::collections::BTreeMap;

use iyw_claw_lib::models::agent::AgentType;
use iyw_claw_lib::user_memory::*;

async fn service() -> (tempfile::TempDir, UserMemoryService) {
    let temp = tempfile::tempdir().unwrap();
    let db = iyw_claw_lib::db::init_database(temp.path(), "test")
        .await
        .unwrap();
    let service = UserMemoryService::new(db.conn, temp.path().join("memory"));
    (temp, service)
}

#[tokio::test]
async fn effective_fingerprint_ignores_changes_to_disabled_documents() {
    let (_temp, service) = service().await;
    let initial = service.snapshot().await.unwrap();
    let profile = &initial.documents[&UserMemoryDocumentId::Profile];
    let disabled = service
        .update(UserMemoryUpdateRequest {
            expected_revision: initial.revision,
            documents: BTreeMap::from([(
                UserMemoryDocumentId::Profile,
                UserMemoryDocumentPatch {
                    enabled: Some(false),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        })
        .await
        .unwrap();
    let before = service
        .context_for(AgentType::Codex, UserMemoryOrigin::Root)
        .await
        .unwrap();
    service
        .update(UserMemoryUpdateRequest {
            expected_revision: disabled.revision,
            documents: BTreeMap::from([(
                UserMemoryDocumentId::Profile,
                UserMemoryDocumentPatch {
                    content: Some("Invisible profile update".into()),
                    expected_etag: Some(profile.etag.clone()),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        })
        .await
        .unwrap();
    let after = service
        .context_for(AgentType::Codex, UserMemoryOrigin::Root)
        .await
        .unwrap();

    assert_eq!(before.effective_fingerprint, after.effective_fingerprint);
}

#[tokio::test]
async fn context_bounds_each_document_without_crowding_out_later_sections() {
    let (_temp, service) = service().await;
    let before = service.snapshot().await.unwrap();
    let documents = UserMemoryDocumentId::ALL
        .into_iter()
        .map(|id| {
            let label = match id {
                UserMemoryDocumentId::Memory => "MEMORY-CONTENT ",
                UserMemoryDocumentId::Profile => "PROFILE-CONTENT ",
                UserMemoryDocumentId::Soul => "SOUL-CONTENT ",
            };
            (
                id,
                UserMemoryDocumentPatch {
                    content: Some(format!("{label}{}", "x".repeat(8_000))),
                    expected_etag: Some(before.documents[&id].etag.clone()),
                    ..Default::default()
                },
            )
        })
        .collect();
    service
        .update(UserMemoryUpdateRequest {
            expected_revision: before.revision,
            documents,
            ..Default::default()
        })
        .await
        .unwrap();

    let rendered = service
        .context_for(AgentType::Codex, UserMemoryOrigin::Root)
        .await
        .unwrap()
        .rendered
        .unwrap();
    assert!(rendered.contains("MEMORY-CONTENT"));
    assert!(rendered.contains("PROFILE-CONTENT"));
    assert!(rendered.contains("SOUL-CONTENT"));
    assert!(rendered.chars().count() <= USER_MEMORY_MAX_CONTEXT_CHARS);
}
