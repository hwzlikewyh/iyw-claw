#![cfg(all(feature = "tauri-runtime", feature = "test-utils"))]

use std::ffi::OsString;
use std::fs::OpenOptions;
use std::path::Path;
use std::sync::Arc;

use axum::extract::Extension;
use axum::Json;
use iyw_claw_lib::app_state::AppState;
use iyw_claw_lib::commands::backup::archive;
use iyw_claw_lib::commands::backup::manifest::BACKUP_PROGRESS_EVENT;
use iyw_claw_lib::commands::backup::restore::{
    apply_pending_restore_on_startup, RestoreApplied, PENDING_MARKER, STAGING_DIR,
};
use iyw_claw_lib::db::service::app_metadata_service;
use iyw_claw_lib::user_memory::UserMemoryPolicy;
use iyw_claw_lib::web::handlers::backup::{backup_create_ticket, CreateBackupParams};
use sea_orm::Database;
use tokio_util::sync::CancellationToken;

struct EnvGuard {
    key: &'static str,
    previous: Option<OsString>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &Path) -> Self {
        let previous = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

async fn persist_generation(state: &AppState, content: &str, enabled: bool) {
    std::fs::write(state.user_memory.root().join("user-memory.md"), content).unwrap();
    let policy = UserMemoryPolicy {
        enabled,
        ..Default::default()
    };
    app_metadata_service::upsert_value(
        &state.db.conn,
        "user_memory.settings",
        &serde_json::to_string(&policy).unwrap(),
    )
    .await
    .unwrap();
}

async fn backup_reached_archiving(
    events: &mut tokio::sync::broadcast::Receiver<iyw_claw_lib::web::event_bridge::WebEvent>,
) -> bool {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let event = events.recv().await.unwrap();
            if event.channel == BACKUP_PROGRESS_EVENT
                && event.payload.get("phase").and_then(|value| value.as_str()) == Some("archiving")
            {
                return;
            }
        }
    })
    .await
    .is_ok()
}

async fn archived_generation(path: &Path) -> (bool, String) {
    let extracted = tempfile::tempdir().unwrap();
    let manifest = archive::read_manifest(path).unwrap();
    archive::extract_all(
        path,
        extracted.path(),
        &manifest,
        &CancellationToken::new(),
        &mut archive::null_progress(),
    )
    .unwrap();
    let db_path = extracted.path().join("db/iyw-claw.db");
    let db_url = format!(
        "sqlite:{}?mode=ro",
        urlencoding::encode(&db_path.to_string_lossy())
    );
    let conn = Database::connect(db_url).await.unwrap();
    let raw = app_metadata_service::get_value(&conn, "user_memory.settings")
        .await
        .unwrap()
        .unwrap();
    let policy: UserMemoryPolicy = serde_json::from_str(&raw).unwrap();
    let content =
        std::fs::read_to_string(extracted.path().join("user-memory/user-memory.md")).unwrap();
    (policy.enabled, content)
}

#[tokio::test]
async fn backup_restore_roundtrip_includes_only_fixed_user_memory_files() {
    let temp = tempfile::tempdir().unwrap();
    let data_dir = temp.path().join("data");
    let memory_root = temp.path().join("canonical-memory");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::create_dir_all(&memory_root).unwrap();
    let _home = EnvGuard::set("IYW_CLAW_HOME", &memory_root);

    let db = iyw_claw_lib::db::init_database(&data_dir, env!("CARGO_PKG_VERSION"))
        .await
        .unwrap();
    let state = Arc::new(AppState::new_for_test(db, data_dir.clone()));
    for (name, content) in [
        ("user-memory.md", "new memory"),
        ("user-profile.md", "new profile"),
        ("user-soul.md", "new soul"),
    ] {
        std::fs::write(state.user_memory.root().join(name), content).unwrap();
    }
    std::fs::write(
        state.user_memory.root().join("unrelated.txt"),
        "not backed up",
    )
    .unwrap();
    let Json(issued) = backup_create_ticket(
        Extension(state.clone()),
        Json(CreateBackupParams {
            include_external_transcripts: false,
            passphrase: None,
        }),
    )
    .await
    .unwrap();
    let ticket = state
        .workspace_transfer
        .consume_download_ticket(&issued.ticket)
        .await
        .unwrap();
    let manifest = archive::read_manifest(&ticket.target_path).unwrap();
    let archived_paths = manifest
        .entries
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<Vec<_>>();
    assert!(archived_paths.contains(&"user-memory/user-memory.md"));
    assert!(archived_paths.contains(&"user-memory/user-profile.md"));
    assert!(archived_paths.contains(&"user-memory/user-soul.md"));
    assert!(!archived_paths.iter().any(|path| path.contains("unrelated")));

    let restore_data_dir = temp.path().join("restore-data");
    std::fs::create_dir_all(&restore_data_dir).unwrap();
    let staging = restore_data_dir
        .join(STAGING_DIR)
        .join("memory-integration");
    archive::extract_all(
        &ticket.target_path,
        &staging,
        &manifest,
        &CancellationToken::new(),
        &mut archive::null_progress(),
    )
    .unwrap();

    for (name, content) in [
        ("user-memory.md", "old memory"),
        ("user-profile.md", "old profile"),
        ("user-soul.md", "old soul"),
    ] {
        std::fs::write(memory_root.join(name), content).unwrap();
    }
    drop(state);
    std::fs::write(
        restore_data_dir.join(PENDING_MARKER),
        serde_json::to_vec(&serde_json::json!({
            "staging_dir": staging,
            "created_at": "2026-07-20T00:00:00Z",
            "app_version": "0.1.7",
            "latest_migration": "m20260703_000001_chat_channel_thread_binding"
        }))
        .unwrap(),
    )
    .unwrap();

    let RestoreApplied::Applied {
        safety_snapshot: Some(snapshot),
    } = apply_pending_restore_on_startup(&restore_data_dir).unwrap()
    else {
        panic!("expected staged restore to apply");
    };

    for (name, expected_new, expected_old) in [
        ("user-memory.md", "new memory", "old memory"),
        ("user-profile.md", "new profile", "old profile"),
        ("user-soul.md", "new soul", "old soul"),
    ] {
        assert_eq!(
            std::fs::read_to_string(memory_root.join(name)).unwrap(),
            expected_new
        );
        assert_eq!(
            std::fs::read_to_string(snapshot.join("user-memory").join(name)).unwrap(),
            expected_old
        );
    }
}

#[tokio::test]
async fn backup_keeps_user_memory_policy_and_documents_in_one_generation() {
    let temp = tempfile::tempdir().unwrap();
    let data_dir = temp.path().join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let db = iyw_claw_lib::db::init_database(&data_dir, env!("CARGO_PKG_VERSION"))
        .await
        .unwrap();
    let state = Arc::new(AppState::new_for_test(db, data_dir));
    state.user_memory.snapshot().await.unwrap();
    persist_generation(&state, "old memory", false).await;

    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(state.user_memory.root().join(".user-memory.lock"))
        .unwrap();
    lock.lock().unwrap();
    let mut events = state.event_broadcaster.subscribe();
    let backup_state = state.clone();
    let backup = tokio::spawn(async move {
        backup_create_ticket(
            Extension(backup_state),
            Json(CreateBackupParams {
                include_external_transcripts: false,
                passphrase: None,
            }),
        )
        .await
        .unwrap()
    });

    let _db_snapshot_completed = backup_reached_archiving(&mut events).await;
    persist_generation(&state, "new memory", true).await;
    lock.unlock().unwrap();

    let Json(issued) = backup.await.unwrap();
    let ticket = state
        .workspace_transfer
        .consume_download_ticket(&issued.ticket)
        .await
        .unwrap();
    let generation = archived_generation(&ticket.target_path).await;

    assert!(
        generation == (false, "old memory".to_string())
            || generation == (true, "new memory".to_string()),
        "mixed backup generation: {generation:?}"
    );
}
