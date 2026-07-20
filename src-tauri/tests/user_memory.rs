use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::time::{Duration, Instant};

use iyw_claw_lib::db::service::app_metadata_service;
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
async fn malformed_persisted_policy_fails_closed() {
    let temp = tempfile::tempdir().unwrap();
    let db = iyw_claw_lib::db::init_database(temp.path(), "test")
        .await
        .unwrap();
    app_metadata_service::upsert_value(&db.conn, "user_memory.settings", "{not-valid-json")
        .await
        .unwrap();
    let service = UserMemoryService::new(db.conn, temp.path().join("memory"));

    let error = service.snapshot().await.unwrap_err();

    assert_eq!(
        serde_json::to_value(error.code).unwrap(),
        serde_json::json!("configuration_invalid")
    );
}

#[tokio::test]
async fn pending_update_journal_rolls_back_a_partial_document_generation() {
    let (_temp, service) = service().await;
    let snapshot = service.snapshot().await.unwrap();
    std::fs::write(
        service
            .root()
            .join(UserMemoryDocumentId::Memory.file_name()),
        "new memory",
    )
    .unwrap();
    let journal = serde_json::json!({
        "previousPolicy": UserMemoryPolicy::default(),
        "nextPolicy": UserMemoryPolicy::default(),
        "previousDocuments": {
            "memory": snapshot.documents[&UserMemoryDocumentId::Memory].content,
            "profile": snapshot.documents[&UserMemoryDocumentId::Profile].content
        },
        "nextDocuments": {
            "memory": "new memory",
            "profile": "new profile"
        }
    });
    std::fs::write(
        service.root().join(".user-memory.transaction.json"),
        serde_json::to_vec(&journal).unwrap(),
    )
    .unwrap();

    let recovered = service.snapshot().await.unwrap();

    assert_eq!(
        recovered.documents[&UserMemoryDocumentId::Memory].content,
        ""
    );
    assert_eq!(
        recovered.documents[&UserMemoryDocumentId::Profile].content,
        ""
    );
    assert!(!service
        .root()
        .join(".user-memory.transaction.json")
        .exists());
}

#[tokio::test(flavor = "current_thread")]
async fn waiting_for_the_file_lock_does_not_block_the_async_runtime() {
    let (_temp, service) = service().await;
    service.snapshot().await.unwrap();
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(service.root().join(".user-memory.lock"))
        .unwrap();
    lock.lock().unwrap();
    let release = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(250));
        lock.unlock().unwrap();
    });
    let started = Instant::now();

    let (snapshot, timer_elapsed) = tokio::join!(service.snapshot(), async {
        tokio::time::sleep(Duration::from_millis(25)).await;
        started.elapsed()
    });
    release.join().unwrap();

    snapshot.unwrap();
    assert!(
        timer_elapsed < Duration::from_millis(150),
        "file-lock wait blocked the runtime for {timer_elapsed:?}"
    );
}

#[tokio::test]
async fn locked_access_removes_only_strictly_named_stale_temporary_files() {
    let (_temp, service) = service().await;
    service.snapshot().await.unwrap();
    let uuid = "0123456789abcdef0123456789abcdef";
    let stale = [
        format!(".user-memory.md.iyw-claw-next-42.{uuid}.tmp"),
        format!(".user-profile.md.iyw-claw-previous-42.{uuid}.tmp"),
        format!(".user-memory.transaction.json.42.{uuid}.tmp"),
    ];
    for name in &stale {
        std::fs::write(service.root().join(name), "private residue").unwrap();
    }
    let unrelated = service.root().join(".user-memory.md.keep.tmp");
    std::fs::write(&unrelated, "keep").unwrap();

    service.snapshot().await.unwrap();

    for name in stale {
        assert!(!service.root().join(name).exists());
    }
    assert!(unrelated.exists());
}

#[tokio::test]
async fn update_content_is_rendered_without_replacing_prompt_semantics() {
    let (_temp, service) = service().await;
    let before = service.snapshot().await.unwrap();
    let memory = before.documents.get(&UserMemoryDocumentId::Memory).unwrap();
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
async fn duplicate_memory_is_suppressed_across_agents() {
    let (_temp, service) = service().await;
    let content = "Prefers stable cross-agent memory";
    let first = service
        .append_agent_memory(AgentMemoryAppend {
            content: content.into(),
            agent_type: AgentType::Codex,
        })
        .await
        .unwrap();
    let second = service
        .append_agent_memory(AgentMemoryAppend {
            content: content.into(),
            agent_type: AgentType::Gemini,
        })
        .await
        .unwrap();

    assert!(first.appended);
    assert!(!second.appended);
    assert_eq!(first.entry_id, second.entry_id);
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

#[tokio::test]
async fn delegation_inheritance_can_be_disabled_without_disabling_root_context() {
    let (_temp, service) = service().await;
    let before = service.snapshot().await.unwrap();
    service
        .update(UserMemoryUpdateRequest {
            expected_revision: before.revision,
            inherit_to_subagents: Some(false),
            ..Default::default()
        })
        .await
        .unwrap();

    let root = service
        .context_for(AgentType::Codex, UserMemoryOrigin::Root)
        .await
        .unwrap();
    let delegated = service
        .context_for(AgentType::Codex, UserMemoryOrigin::Delegation)
        .await
        .unwrap();
    assert!(root.rendered.is_some());
    assert!(root.memory_write_enabled);
    assert!(delegated.rendered.is_none());
    assert!(!delegated.memory_write_enabled);
}

#[test]
fn strips_only_the_private_context_envelope() {
    let transcript =
        format!("{USER_CONTEXT_START}\nprivate\n{USER_CONTEXT_END}\n\nactual user prompt");
    assert_eq!(strip_user_context(&transcript), "actual user prompt");
    assert_eq!(strip_user_context("ordinary prompt"), "ordinary prompt");
}

#[tokio::test]
async fn stale_revision_is_rejected_without_overwriting_newer_content() {
    let (_temp, service) = service().await;
    let stale = service.snapshot().await.unwrap();
    service
        .append_agent_memory(AgentMemoryAppend {
            content: "Prefers deterministic identifiers".into(),
            agent_type: AgentType::Codex,
        })
        .await
        .unwrap();
    let error = service
        .update(UserMemoryUpdateRequest {
            expected_revision: stale.revision,
            enabled: Some(false),
            ..Default::default()
        })
        .await
        .unwrap_err();

    assert_eq!(
        serde_json::to_value(error.code).unwrap(),
        serde_json::json!("conflict")
    );
    assert!(service.snapshot().await.unwrap().enabled);
}

#[tokio::test]
async fn agent_append_rejects_secrets_and_unsupported_agents() {
    let (_temp, service) = service().await;
    let secret = service
        .append_agent_memory(AgentMemoryAppend {
            content: "API key is sk-example".into(),
            agent_type: AgentType::Codex,
        })
        .await;
    let unsupported = service
        .append_agent_memory(AgentMemoryAppend {
            content: "Prefers concise answers".into(),
            agent_type: AgentType::OpenClaw,
        })
        .await;

    assert!(secret.is_err());
    assert!(unsupported.is_err());
    assert_eq!(
        service.snapshot().await.unwrap().documents[&UserMemoryDocumentId::Memory].content,
        ""
    );
}

#[tokio::test]
async fn agent_append_rejects_common_credential_formats_without_broad_keyword_guessing() {
    let (_temp, service) = service().await;
    for credential in [
        "Authorization: Bearer abc.def.ghi",
        "JWT eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.signature",
        "GitHub ghp_0123456789abcdefghijklmnopqrstuv",
        "GitHub github_pat_11AA0123456789abcdefghijkl",
        "AWS AKIAIOSFODNN7EXAMPLE",
        "client_secret: example-value",
        "token=example-value",
        "secret: example-value",
        "密码：example-value",
        "口令 example-value",
    ] {
        let result = service
            .append_agent_memory(AgentMemoryAppend {
                content: credential.into(),
                agent_type: AgentType::Codex,
            })
            .await;
        assert!(
            result.is_err(),
            "credential should be rejected: {credential}"
        );
    }

    for preference in [
        "Prefers token-efficient summaries",
        "Usually reads AWS documentation in English",
        "Prefers concise authentication explanations",
    ] {
        assert!(
            service
                .append_agent_memory(AgentMemoryAppend {
                    content: preference.into(),
                    agent_type: AgentType::Codex,
                })
                .await
                .unwrap()
                .appended,
            "ordinary preference should remain allowed: {preference}"
        );
    }
}

#[tokio::test]
async fn concurrent_agent_appends_do_not_interleave_or_drop_entries() {
    let (_temp, service) = service().await;
    let left = service.clone();
    let right = service.clone();
    let (left_result, right_result) = tokio::join!(
        left.append_agent_memory(AgentMemoryAppend {
            content: "Prefers the first durable fact".into(),
            agent_type: AgentType::Codex,
        }),
        right.append_agent_memory(AgentMemoryAppend {
            content: "Prefers the second durable fact".into(),
            agent_type: AgentType::Gemini,
        })
    );
    assert!(left_result.unwrap().appended);
    assert!(right_result.unwrap().appended);

    let content = service.snapshot().await.unwrap().documents[&UserMemoryDocumentId::Memory]
        .content
        .clone();
    assert!(content.contains("first durable fact"));
    assert!(content.contains("second durable fact"));
    assert_eq!(content.lines().count(), 2);
}

#[tokio::test]
async fn authenticated_launch_snapshot_can_append_after_policy_changes() {
    let (_temp, service) = service().await;
    let before = service.snapshot().await.unwrap();
    service
        .update(UserMemoryUpdateRequest {
            expected_revision: before.revision,
            agent_write_enabled: Some(false),
            ..Default::default()
        })
        .await
        .unwrap();
    let input = AgentMemoryAppend {
        content: "Prefers launch-scoped capability snapshots".into(),
        agent_type: AgentType::Codex,
    };

    assert!(service.append_agent_memory(input.clone()).await.is_err());
    assert!(
        service
            .append_agent_memory_authorized(input)
            .await
            .unwrap()
            .appended
    );
}

#[test]
fn malformed_or_nested_context_is_never_returned_as_visible_user_text() {
    let nested = format!(
        "before\n{USER_CONTEXT_START}\nprivate outer\n{USER_CONTEXT_START}\nprivate inner\n{USER_CONTEXT_END}\nprivate tail\n{USER_CONTEXT_END}\nafter"
    );
    let stripped = strip_user_context(&nested);
    assert_eq!(stripped, "before\nafter");
    assert!(!stripped.contains("private"));

    let malformed = format!("before\n{USER_CONTEXT_START}\nprivate without end");
    assert_eq!(strip_user_context(&malformed), "before");
}

#[tokio::test]
async fn manual_documents_reject_private_context_markers() {
    let (_temp, service) = service().await;
    let before = service.snapshot().await.unwrap();
    let memory = &before.documents[&UserMemoryDocumentId::Memory];
    let content = format!("ordinary\n{USER_CONTEXT_START}\nprivate");
    let error = service
        .update(UserMemoryUpdateRequest {
            expected_revision: before.revision,
            documents: BTreeMap::from([(
                UserMemoryDocumentId::Memory,
                UserMemoryDocumentPatch {
                    content: Some(content),
                    expected_etag: Some(memory.etag.clone()),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        })
        .await
        .expect_err("manual context markers must be rejected");
    assert_eq!(
        serde_json::to_value(error.code).unwrap(),
        serde_json::json!("invalid_input")
    );
}

#[tokio::test]
async fn multi_document_update_does_not_commit_earlier_files_when_later_write_fails() {
    let (_temp, service) = service().await;
    let before = service.snapshot().await.unwrap();
    let memory = &before.documents[&UserMemoryDocumentId::Memory];
    let profile = &before.documents[&UserMemoryDocumentId::Profile];
    let profile_path = profile.path.clone();
    let mut permissions = std::fs::metadata(&profile_path).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&profile_path, permissions).unwrap();

    let result = service
        .update(UserMemoryUpdateRequest {
            expected_revision: before.revision,
            documents: BTreeMap::from([
                (
                    UserMemoryDocumentId::Memory,
                    UserMemoryDocumentPatch {
                        content: Some("new memory".into()),
                        expected_etag: Some(memory.etag.clone()),
                        ..Default::default()
                    },
                ),
                (
                    UserMemoryDocumentId::Profile,
                    UserMemoryDocumentPatch {
                        content: Some("new profile".into()),
                        expected_etag: Some(profile.etag.clone()),
                        ..Default::default()
                    },
                ),
            ]),
            ..Default::default()
        })
        .await;

    assert!(
        result.is_err(),
        "the read-only profile must fail the update"
    );
    assert_eq!(
        std::fs::read_to_string(memory.path.clone()).unwrap(),
        "",
        "the earlier memory write must be rolled back"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn user_memory_rejects_symlinked_document() {
    let (_temp, service) = service().await;
    let snapshot = service.snapshot().await.unwrap();
    let memory_path = snapshot.documents[&UserMemoryDocumentId::Memory]
        .path
        .clone();
    let target = memory_path.with_file_name("outside.md");
    std::fs::write(&target, "outside").unwrap();
    std::fs::remove_file(&memory_path).unwrap();
    std::os::unix::fs::symlink(&target, &memory_path).unwrap();

    let error = service.snapshot().await.unwrap_err();
    assert_eq!(
        serde_json::to_value(error.code).unwrap(),
        serde_json::json!("permission_denied")
    );
    assert_eq!(std::fs::read_to_string(target).unwrap(), "outside");
}
