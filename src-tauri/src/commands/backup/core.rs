//! Runtime-agnostic backup engine.
//!
//! `create_backup_core` / `inspect_backup_core` take plain references
//! (`&DatabaseConnection`, `&EventEmitter`, `&CancellationToken`) so the same
//! code path serves the desktop Tauri commands, the Axum web handlers, and a
//! future headless scheduler (which would pass `EventEmitter::Noop`).

use std::path::{Path, PathBuf};

use chrono::Utc;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};
use sea_orm_migration::MigratorTrait;
use tokio_util::sync::CancellationToken;

use crate::app_error::{
    AppCommandError, BACKUP_I18N_KEY_NEWER_VERSION, BACKUP_I18N_KEY_UNKNOWN_FORMAT,
};
use crate::db::migration::Migrator;
use crate::web::event_bridge::{emit_event, EventEmitter};

use super::archive::{self, ArchiveBuilder};
use super::cancelled_error;
use super::crypto;
use super::external;
use super::manifest::{
    BackupManifest, BackupPhase, BackupPreview, BackupProgress, BACKUP_FORMAT_VERSION, BACKUP_KIND,
    BACKUP_PROGRESS_EVENT,
};

/// Options that shape a backup.
#[derive(Debug, Clone, Default)]
pub struct BackupOptions {
    pub include_external_transcripts: bool,
    /// `None` or empty → unencrypted archive. Otherwise the archive is wrapped
    /// in an AES-256-GCM envelope keyed off this passphrase.
    pub passphrase: Option<String>,
}

/// Everything the engine needs to assemble a backup, resolved by the caller
/// (desktop command / web handler) so the engine stays free of env lookups.
pub struct BackupInputs<'a> {
    pub conn: &'a DatabaseConnection,
    pub data_dir: &'a Path,
    pub uploads_root: PathBuf,
    pub user_memory_root: PathBuf,
    pub app_version: &'a str,
    pub runtime_label: &'static str,
}

/// Build a backup archive at `dest_path`. Emits [`BACKUP_PROGRESS_EVENT`]
/// throughout and honors `cancel`. Writes to a sibling `.part` file and renames
/// on success so a crash never leaves a half-written backup at `dest_path`.
pub(crate) async fn create_backup_core(
    inputs: BackupInputs<'_>,
    options: BackupOptions,
    dest_path: &Path,
    emitter: &EventEmitter,
    op_id: &str,
    cancel: &CancellationToken,
) -> Result<BackupManifest, AppCommandError> {
    let work = tempfile::tempdir().map_err(AppCommandError::io)?;
    let db_snapshot = work.path().join("iyw-claw.db");
    let zip_tmp = work.path().join("payload.zip");

    // ── Phase 1: consistent DB snapshot via VACUUM INTO ──────────────────
    emit(emitter, op_id, BackupPhase::Snapshotting, 0, None, None);
    if cancel.is_cancelled() {
        return Err(cancelled_error());
    }
    let user_memory_root = inputs.user_memory_root.clone();
    let user_memory_service =
        crate::user_memory::UserMemoryService::new(inputs.conn.clone(), user_memory_root.clone());
    let user_memory_lock = user_memory_service.lock_for_backup_snapshot().await?;
    snapshot_db_to(inputs.conn, &db_snapshot).await?;
    let user_memory_snapshot_root = work.path().join("user-memory-snapshot");
    let memory_documents = tokio::task::spawn_blocking(move || {
        super::user_memory::snapshot_for_backup(
            &user_memory_root,
            &user_memory_snapshot_root,
            user_memory_lock.file(),
        )
    })
    .await
    .map_err(|error| {
        AppCommandError::task_execution_failed("User memory snapshot task failed")
            .with_detail(error.to_string())
    })??;

    // ── Phase 2: build the ZIP payload (blocking) ────────────────────────
    let manifest_template = BackupManifest {
        format_version: BACKUP_FORMAT_VERSION,
        kind: BACKUP_KIND.to_string(),
        created_at: Utc::now().to_rfc3339(),
        app_version: inputs.app_version.to_string(),
        latest_migration: latest_migration_name(),
        runtime: inputs.runtime_label.to_string(),
        includes_external_transcripts: false, // set after packing
        includes_secrets: true,
        entries: Vec::new(),
    };

    let uploads_root = inputs.uploads_root.clone();
    let tokens_json = inputs.data_dir.join("tokens.json");
    let prefs_json = crate::paths::iyw_claw_home_dir().join("preferences.json");
    let include_external = options.include_external_transcripts;

    let zip_tmp_c = zip_tmp.clone();
    let db_snapshot_c = db_snapshot.clone();
    let cancel_c = cancel.clone();
    let emitter_c = emitter.clone();
    let op_id_c = op_id.to_string();

    emit(emitter, op_id, BackupPhase::Archiving, 0, None, None);
    let manifest =
        tokio::task::spawn_blocking(move || -> Result<BackupManifest, AppCommandError> {
            let mut builder = ArchiveBuilder::create(&zip_tmp_c)?;
            let mut prog = |path: &str, processed: u64| {
                emit(
                    &emitter_c,
                    &op_id_c,
                    BackupPhase::Archiving,
                    processed,
                    None,
                    Some(path.to_string()),
                );
            };
            builder.add_file("db/iyw-claw.db", &db_snapshot_c, &cancel_c, &mut prog)?;
            builder.add_dir(
                "uploads",
                &uploads_root,
                &is_excluded_upload,
                &cancel_c,
                &mut prog,
            )?;
            if tokens_json.is_file() {
                builder.add_file("tokens.json", &tokens_json, &cancel_c, &mut prog)?;
            }
            if prefs_json.is_file() {
                builder.add_file("preferences.json", &prefs_json, &cancel_c, &mut prog)?;
            }
            for (file_name, source) in memory_documents {
                let archive_path = format!("{}/{file_name}", super::USER_MEMORY_ARCHIVE_DIR);
                builder.add_file(&archive_path, &source, &cancel_c, &mut prog)?;
            }
            let mut manifest = manifest_template;
            let packed_external = if include_external {
                external::add_external_sources(&mut builder, &cancel_c, &mut prog)?
            } else {
                false
            };
            manifest.includes_external_transcripts = packed_external;
            builder.finish(manifest)
        })
        .await
        .map_err(|e| {
            AppCommandError::task_execution_failed("Archive task failed").with_detail(e.to_string())
        })??;

    // ── Phase 3: deliver (encrypt or copy) into dest_path atomically ─────
    let part = with_part_suffix(dest_path);
    if let Some(parent) = dest_path.parent() {
        tokio::fs::create_dir_all(parent).await.ok();
    }
    match options.passphrase.as_deref().filter(|p| !p.is_empty()) {
        Some(pass) => {
            emit(emitter, op_id, BackupPhase::Encrypting, 0, None, None);
            let zip_tmp_c = zip_tmp.clone();
            let part_c = part.clone();
            let pass = pass.to_string();
            let cancel_c = cancel.clone();
            tokio::task::spawn_blocking(move || {
                crypto::encrypt_file(&zip_tmp_c, &part_c, &pass, &cancel_c)
            })
            .await
            .map_err(|e| {
                AppCommandError::task_execution_failed("Encrypt task failed")
                    .with_detail(e.to_string())
            })??;
        }
        None => {
            tokio::fs::copy(&zip_tmp, &part)
                .await
                .map_err(super::map_disk_full)?;
        }
    }
    tokio::fs::rename(&part, dest_path)
        .await
        .map_err(AppCommandError::io)?;

    let total = manifest.total_bytes();
    emit(emitter, op_id, BackupPhase::Done, total, Some(total), None);
    Ok(manifest)
}

/// Validate a candidate backup before applying it. Detects encryption, reads
/// (and thereby passphrase-verifies) the manifest, and checks version
/// compatibility — without touching live data.
pub(crate) async fn inspect_backup_core(
    src: &Path,
    passphrase: Option<&str>,
) -> Result<BackupPreview, AppCommandError> {
    let src_buf = src.to_path_buf();
    let encrypted = tokio::task::spawn_blocking(move || crypto::is_encrypted(&src_buf))
        .await
        .map_err(|e| {
            AppCommandError::task_execution_failed("Inspect task failed").with_detail(e.to_string())
        })??;

    if encrypted && passphrase.is_none_or(|p| p.is_empty()) {
        return Ok(BackupPreview {
            encrypted: true,
            needs_passphrase: true,
            manifest: None,
            compatible: false,
            reject_reason: None,
        });
    }

    let (zip_path, _guard) = obtain_plaintext_zip(src, encrypted, passphrase).await?;
    let manifest = tokio::task::spawn_blocking(move || archive::read_manifest(&zip_path))
        .await
        .map_err(|e| {
            AppCommandError::task_execution_failed("Inspect task failed").with_detail(e.to_string())
        })??;

    let (mut compatible, mut reject_reason) = evaluate_compat(&manifest);
    // Mirror the structural checks stage applies, so the preview never reports
    // "compatible" for a backup that stage will reject (missing db/iyw-claw.db,
    // unsafe/duplicate manifest paths).
    if compatible && archive::validate_manifest(&manifest).is_err() {
        compatible = false;
        reject_reason = Some(BACKUP_I18N_KEY_UNKNOWN_FORMAT.to_string());
    }
    Ok(BackupPreview {
        encrypted,
        needs_passphrase: false,
        manifest: Some(manifest),
        compatible,
        reject_reason,
    })
}

/// Scan a backup for external transcript entries whose live target already
/// exists. Called only when the user opts to restore to original locations,
/// so the UI can surface conflicts before any write.
pub(crate) async fn scan_external_conflicts_core(
    src: &Path,
    passphrase: Option<&str>,
) -> Result<Vec<super::external::ExternalConflict>, AppCommandError> {
    let src_buf = src.to_path_buf();
    let encrypted = tokio::task::spawn_blocking(move || crypto::is_encrypted(&src_buf))
        .await
        .map_err(|e| {
            AppCommandError::task_execution_failed("Scan task failed").with_detail(e.to_string())
        })??;
    let (zip_path, _guard) = obtain_plaintext_zip(src, encrypted, passphrase).await?;
    tokio::task::spawn_blocking(move || super::external::scan_external_conflicts(&zip_path))
        .await
        .map_err(|e| {
            AppCommandError::task_execution_failed("Scan task failed").with_detail(e.to_string())
        })?
}

/// Run `VACUUM INTO` to produce a transactionally-consistent, defragmented
/// single-file copy of the live DB — sidesteps the WAL `-wal`/`-shm` sidecars.
pub(crate) async fn snapshot_db_to(
    conn: &DatabaseConnection,
    dest: &Path,
) -> Result<(), AppCommandError> {
    // VACUUM INTO requires the destination not to exist.
    if dest.exists() {
        tokio::fs::remove_file(dest)
            .await
            .map_err(AppCommandError::io)?;
    }
    let dest_lit = dest.to_string_lossy().replace('\'', "''");
    let sql = format!("VACUUM INTO '{dest_lit}';");
    conn.execute(Statement::from_string(DbBackend::Sqlite, sql))
        .await
        .map_err(|e| {
            AppCommandError::database_error("VACUUM INTO failed").with_detail(e.to_string())
        })?;
    Ok(())
}

/// Decrypt-to-temp if needed; returns a plaintext ZIP path plus a guard that
/// must outlive any read of that path.
pub(crate) async fn obtain_plaintext_zip(
    src: &Path,
    encrypted: bool,
    passphrase: Option<&str>,
) -> Result<(PathBuf, Option<tempfile::TempDir>), AppCommandError> {
    if !encrypted {
        return Ok((src.to_path_buf(), None));
    }
    let pass = passphrase.unwrap_or_default().to_string();
    let td = tempfile::tempdir().map_err(AppCommandError::io)?;
    let out = td.path().join("decrypted.zip");
    let src_c = src.to_path_buf();
    let out_c = out.clone();
    let cancel = CancellationToken::new();
    tokio::task::spawn_blocking(move || crypto::decrypt_file(&src_c, &out_c, &pass, &cancel))
        .await
        .map_err(|e| {
            AppCommandError::task_execution_failed("Decrypt task failed").with_detail(e.to_string())
        })??;
    Ok((out, Some(td)))
}

/// `(compatible, reject_reason_i18n_key)`. Schema compatibility is keyed off
/// the migration identity (more robust than semver): an unknown
/// `latest_migration` means the backup is newer than this binary understands.
pub(crate) fn evaluate_compat(manifest: &BackupManifest) -> (bool, Option<String>) {
    if manifest.format_version > BACKUP_FORMAT_VERSION || manifest.kind != BACKUP_KIND {
        return (false, Some(BACKUP_I18N_KEY_UNKNOWN_FORMAT.to_string()));
    }
    if !known_migration(&manifest.latest_migration) {
        return (false, Some(BACKUP_I18N_KEY_NEWER_VERSION.to_string()));
    }
    (true, None)
}

fn latest_migration_name() -> String {
    Migrator::migrations()
        .last()
        .map(|m| m.name().to_string())
        .unwrap_or_default()
}

fn known_migration(name: &str) -> bool {
    Migrator::migrations().iter().any(|m| m.name() == name)
}

/// Exclude upload staging dirs (`uploads/.tmp/`) and any iyw-claw-internal
/// `.iyw-claw-*` directory (restore staging / safety snapshots) from the archive.
fn is_excluded_upload(rel: &Path) -> bool {
    rel.components().any(|c| match c {
        std::path::Component::Normal(s) => {
            let s = s.to_string_lossy();
            s == ".tmp" || s.starts_with(".iyw-claw")
        }
        _ => false,
    })
}

fn with_part_suffix(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

fn emit(
    emitter: &EventEmitter,
    op_id: &str,
    phase: BackupPhase,
    processed: u64,
    total: Option<u64>,
    path: Option<String>,
) {
    emit_event(
        emitter,
        BACKUP_PROGRESS_EVENT,
        BackupProgress {
            op_id: op_id.to_string(),
            phase,
            processed_bytes: processed,
            total_bytes: total,
            current_path: path,
            error: None,
        },
    );
}
