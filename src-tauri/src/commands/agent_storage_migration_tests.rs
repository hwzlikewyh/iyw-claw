use crate::acp::agent_storage::load_config;
use crate::models::agent::AgentType;

use super::{
    initialize_agent_storage_core, migrate_agent_storage_with_verifier_core, MigrationActivity,
};

#[tokio::test]
async fn migrate_agent_storage_failure_keeps_old_root_and_source_untouched() {
    let db = crate::db::test_helpers::fresh_in_memory_db().await;
    let parent = tempfile::tempdir().expect("temp dir");
    let old_root = parent.path().join("old");
    let new_root = parent.path().join("new");
    initialize_agent_storage_core(&db.conn, old_root.clone(), false, None, false)
        .await
        .expect("initialize old root");
    let source_file = old_root.join("config/codex/config.toml");
    std::fs::write(&source_file, "model = \"keep\"\n").expect("seed source");

    let error = migrate_agent_storage_with_verifier_core(
        &db.conn,
        new_root.clone(),
        false,
        None,
        MigrationActivity::default(),
        |_| Err("injected verification failure".to_string()),
    )
    .await
    .expect_err("migration must roll back");

    assert!(error
        .detail
        .as_deref()
        .is_some_and(|detail| detail.contains("injected verification failure")));
    let persisted = load_config(&db.conn)
        .await
        .expect("load config")
        .expect("config");
    assert_eq!(persisted.root, Some(old_root));
    assert_eq!(
        std::fs::read_to_string(source_file).expect("source remains"),
        "model = \"keep\"\n"
    );
    assert!(!new_root.exists());
}

#[tokio::test]
async fn migrate_agent_storage_success_reports_old_root_without_deleting_it() {
    let db = crate::db::test_helpers::fresh_in_memory_db().await;
    let parent = tempfile::tempdir().expect("temp dir");
    let old_root = parent.path().join("old");
    let new_root = parent.path().join("new");
    initialize_agent_storage_core(&db.conn, old_root.clone(), false, None, false)
        .await
        .expect("initialize old root");
    std::fs::write(
        old_root.join("config/codex/config.toml"),
        "model = \"keep\"\n",
    )
    .expect("seed source");

    let status = migrate_agent_storage_with_verifier_core(
        &db.conn,
        new_root.clone(),
        false,
        None,
        MigrationActivity::default(),
        |paths| {
            if paths
                .profile(AgentType::Codex)
                .root
                .join("config.toml")
                .is_file()
            {
                Ok(())
            } else {
                Err("copied Codex profile is missing".to_string())
            }
        },
    )
    .await
    .expect("migration succeeds");

    assert_eq!(status.active_root, Some(new_root.clone()));
    assert_eq!(status.previous_root, Some(old_root.clone()));
    assert!(status.restart_required);
    assert!(old_root.join("config/codex/config.toml").is_file());
    assert!(new_root.join("config/codex/config.toml").is_file());
}

#[tokio::test]
async fn migrate_agent_storage_rejects_active_sessions_and_installs() {
    let db = crate::db::test_helpers::fresh_in_memory_db().await;
    let parent = tempfile::tempdir().expect("temp dir");
    let old_root = parent.path().join("old");
    initialize_agent_storage_core(&db.conn, old_root, false, None, false)
        .await
        .expect("initialize old root");

    for activity in [
        MigrationActivity {
            active_connections: true,
            active_installs: false,
        },
        MigrationActivity {
            active_connections: false,
            active_installs: true,
        },
    ] {
        migrate_agent_storage_with_verifier_core(
            &db.conn,
            parent.path().join(format!("new-{}", uuid::Uuid::new_v4())),
            false,
            None,
            activity,
            |_| Ok(()),
        )
        .await
        .expect_err("active work must block migration");
    }
}
