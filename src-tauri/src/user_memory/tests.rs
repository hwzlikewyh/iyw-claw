use std::collections::BTreeMap;

use crate::models::agent::AgentType;

use super::*;

async fn service() -> (tempfile::TempDir, UserMemoryService) {
    let temp = tempfile::tempdir().unwrap();
    let db = crate::db::test_helpers::fresh_in_memory_db().await;
    let service = UserMemoryService::new(db.conn, temp.path().to_path_buf());
    (temp, service)
}

#[tokio::test]
async fn default_snapshot_creates_all_enabled_documents() {
    let (_temp, service) = service().await;
    let snapshot = service.snapshot().await.unwrap();

    assert!(snapshot.enabled);
    assert!(snapshot.agent_write_enabled);
    assert!(snapshot.inherit_to_subagents);
    assert_eq!(snapshot.documents.len(), 3);
    for id in UserMemoryDocumentId::ALL {
        let document = snapshot.documents.get(&id).unwrap();
        assert!(document.enabled);
        assert_eq!(document.content, "");
        assert!(document.path.ends_with(id.file_name()));
        assert!(document.path.is_file());
    }
}

#[tokio::test]
async fn update_content_is_rendered_without_replacing_prompt_semantics() {
    let (_temp, service) = service().await;
    let before = service.snapshot().await.unwrap();
    let memory = before
        .documents
        .get(&UserMemoryDocumentId::Memory)
        .unwrap();
    let request = UserMemoryUpdateRequest {
        expected_revision: before.revision,
        documents: BTreeMap::from([(
            UserMemoryDocumentId::Memory,
            UserMemoryDocumentPatch {
                content: Some("Prefers concise Chinese answers.".into()),
                expected_etag: Some(memory.etag.clone()),
                ..Default::default()
            },
        )]),
        ..Default::default()
    };
    service.update(request).await.unwrap();

    let context = service
        .context_for(AgentType::Codex, UserMemoryOrigin::Root)
        .await
        .unwrap();
    let rendered = context.rendered.unwrap();
    assert!(rendered.contains(USER_CONTEXT_START));
    assert!(rendered.contains("Prefers concise Chinese answers."));
    assert!(rendered.contains("higher-priority instructions"));
    assert!(rendered.contains("append_user_memory"));
}

#[tokio::test]
async fn append_is_single_line_and_deduplicated() {
    let (_temp, service) = service().await;
    let input = AgentMemoryAppend {
        content: "Prefers\ncompact  status updates".into(),
        agent_type: AgentType::Codex,
    };

    let first = service.append_agent_memory(input.clone()).await.unwrap();
    let second = service.append_agent_memory(input).await.unwrap();
    let snapshot = service.snapshot().await.unwrap();
    let content = &snapshot.documents[&UserMemoryDocumentId::Memory].content;

    assert!(first.appended);
    assert!(!second.appended);
    assert_eq!(first.entry_id, second.entry_id);
    assert!(content.contains("Prefers compact status updates"));
    assert_eq!(content.matches(&first.entry_id).count(), 1);
}

#[tokio::test]
async fn disabled_agent_and_probe_receive_no_context_or_write_permission() {
    let (_temp, service) = service().await;
    let before = service.snapshot().await.unwrap();
    let mut per_agent = before.per_agent.clone();
    per_agent.insert(AgentType::Gemini, false);
    service
        .update(UserMemoryUpdateRequest {
            expected_revision: before.revision,
            per_agent: Some(per_agent),
            ..Default::default()
        })
        .await
        .unwrap();

    let disabled = service
        .context_for(AgentType::Gemini, UserMemoryOrigin::Root)
        .await
        .unwrap();
    let probe = service
        .context_for(AgentType::Codex, UserMemoryOrigin::Probe)
        .await
        .unwrap();
    assert!(disabled.rendered.is_none());
    assert!(!disabled.memory_write_enabled);
    assert!(probe.rendered.is_none());
    assert!(!probe.memory_write_enabled);
}

#[test]
fn strips_only_the_private_context_envelope() {
    let transcript = format!(
        "{USER_CONTEXT_START}\nprivate\n{USER_CONTEXT_END}\n\nactual user prompt"
    );
    assert_eq!(strip_user_context(&transcript), "actual user prompt");
    assert_eq!(strip_user_context("ordinary prompt"), "ordinary prompt");
}
