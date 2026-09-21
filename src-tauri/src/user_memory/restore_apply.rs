use super::{
    authority_export as export, authority_sql as sql,
    restore_bundle::{self, RecoveryBundle, TABLES},
    structured_file, MemoryReconciliationResult, RestoreMemoryAuthorityRequest, UserMemoryService,
};
use crate::app_error::AppCommandError;
use sea_orm::{ConnectionTrait, TransactionTrait};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const RESTORE_JOURNAL: &str = ".memory-authority-restore.json";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RestoreJournal {
    previous: Option<(String, i64, String)>,
    bundle: RecoveryBundle,
    bundle_digest: String,
    exports: BTreeMap<String, Option<String>>,
}

impl UserMemoryService {
    pub async fn restore_memory_authority(
        &self,
        request: RestoreMemoryAuthorityRequest,
    ) -> Result<MemoryReconciliationResult, AppCommandError> {
        let (_guard, _file) = self.acquire_reconciliation_locks().await?;
        let preview = self.reconciliation_locked().await?;
        if preview.mode != "restore_required" || preview.revision != request.expected_revision {
            return Err(super::helpers::conflict(
                "Memory restore state changed; refresh reconciliation",
            ));
        }
        if super::authority_recovery::has_pending(self.resolved_root()?)? {
            return Err(super::helpers::conflict(
                "Finish pending memory commit recovery first",
            ));
        }
        let bundle = self
            .recovery_sources_locked()
            .await?
            .into_iter()
            .find(|source| source.id == request.source_id)
            .ok_or_else(|| {
                super::helpers::conflict("Recovery evidence changed or is no longer available")
            })?
            .bundle;
        let key = self.authority_key()?;
        let current = restore_bundle::load(&self.db, &key).await?;
        if let Some(current) = &current {
            super::restore_integrity::preserves(current, &bundle)?;
        }
        let backup_path = self
            .backup_authority_source(
                current
                    .as_ref()
                    .map(|value| &value.snapshot.data)
                    .unwrap_or(&bundle.snapshot.data),
            )
            .await?;
        let journal = self.prepare_restore_journal(bundle, current.as_ref())?;
        self.apply_restore_journal(&journal).await?;
        self.load_authority_locked().await?;
        self.schedule_index_refresh();
        tracing::info!(
            epoch = journal.bundle.snapshot.epoch,
            "[memory-reconcile] restored verified authority and full history"
        );
        Ok(MemoryReconciliationResult { backup_path })
    }

    fn prepare_restore_journal(
        &self,
        mut bundle: RecoveryBundle,
        current: Option<&RecoveryBundle>,
    ) -> Result<RestoreJournal, AppCommandError> {
        let exports = self.archive_restore_files(&bundle)?;
        let previous = current
            .as_ref()
            .map(|value| &value.snapshot)
            .map(|snapshot| {
                (
                    snapshot.store_id.clone(),
                    snapshot.epoch,
                    snapshot.digest.clone(),
                )
            });
        bundle.snapshot.epoch += 1;
        let journal = RestoreJournal {
            previous,
            exports,
            bundle_digest: restore_bundle::fingerprint(&bundle)?,
            bundle,
        };
        structured_file::write_json_atomic(self.resolved_root()?, RESTORE_JOURNAL, &journal)?;
        Ok(journal)
    }

    pub(super) async fn recover_memory_restore_locked(&self) -> Result<(), AppCommandError> {
        let Some(journal) = structured_file::read_json_optional::<RestoreJournal>(
            self.resolved_root()?,
            RESTORE_JOURNAL,
            restore_bundle::MAX_BUNDLE_BYTES,
        )?
        else {
            return Ok(());
        };
        self.apply_restore_journal(&journal).await
    }

    fn archive_restore_files(
        &self,
        bundle: &RecoveryBundle,
    ) -> Result<BTreeMap<String, Option<String>>, AppCommandError> {
        let mut files = BTreeMap::new();
        for name in export::export_contents(&bundle.snapshot)?.keys() {
            let content = export::read_export(self.resolved_root()?, name)?;
            self.archive_memory_conflict(name, content.as_deref())?;
            files.insert(
                name.clone(),
                super::reconcile_files::digest(content.as_deref()),
            );
        }
        Ok(files)
    }

    async fn apply_restore_journal(&self, journal: &RestoreJournal) -> Result<(), AppCommandError> {
        validate_journal(self, journal)?;
        let snapshot = &journal.bundle.snapshot;
        let txn = self
            .db
            .begin()
            .await
            .map_err(super::index_checkpoint::database_error)?;
        let key = self.authority_key()?;
        let current = sql::load(&txn, &key).await?;
        let current_id = current
            .as_ref()
            .map(|s| (s.store_id.clone(), s.epoch, s.digest.clone()));
        let next_id = Some((
            snapshot.store_id.clone(),
            snapshot.epoch,
            snapshot.digest.clone(),
        ));
        if current_id != journal.previous && current_id != next_id {
            return Err(super::helpers::conflict(
                "Recovery journal does not match current database",
            ));
        }
        if current_id != next_id {
            write_bundle(&txn, &key, &journal.bundle).await?;
        }
        let restored = restore_bundle::load(&txn, &key)
            .await?
            .ok_or_else(|| restore_bundle::invalid("Recovered authority missing"))?;
        if restore_bundle::fingerprint(&restored)? != journal.bundle_digest {
            return Err(restore_bundle::invalid(
                "Restored memory history failed reconciliation",
            ));
        }
        export::write_fence(self.resolved_root()?, snapshot)?;
        txn.commit()
            .await
            .map_err(super::index_checkpoint::database_error)?;
        let mut marker = export::marker(self.resolved_root()?)?.expect("persisted fence");
        marker.exports = journal.exports.clone();
        structured_file::write_json_atomic(self.resolved_root()?, export::MARKER_FILE, &marker)?;
        if let Err(error) = self.export_authority_locked(snapshot).await {
            tracing::warn!(code=?error.code,"[memory-reconcile] authority recovered; compatibility export deferred");
        }
        structured_file::remove_optional(self.resolved_root()?, RESTORE_JOURNAL)
    }
}

fn validate_journal(
    service: &UserMemoryService,
    journal: &RestoreJournal,
) -> Result<(), AppCommandError> {
    restore_bundle::validate(&journal.bundle)?;
    super::restore_integrity::matches_snapshot(service, &journal.bundle)?;
    if restore_bundle::fingerprint(&journal.bundle)? != journal.bundle_digest {
        return Err(restore_bundle::invalid("Recovery journal digest mismatch"));
    }
    let next = &journal.bundle.snapshot;
    let marker = export::marker(service.resolved_root()?)?
        .ok_or_else(|| super::helpers::conflict("Recovery fence missing"))?;
    if marker.store_id != next.store_id
        || marker.epoch > next.epoch
        || marker.epoch == next.epoch && marker.digest != next.digest
    {
        return Err(super::helpers::conflict(
            "Recovery journal is older than the memory fence",
        ));
    }
    Ok(())
}

async fn write_bundle<C: ConnectionTrait>(
    db: &C,
    key: &str,
    bundle: &RecoveryBundle,
) -> Result<(), AppCommandError> {
    let snapshot = &bundle.snapshot;
    sql::execute(db,"INSERT INTO memory_authority(root_key,store_id,mode,epoch,snapshot_json,source_digest,backup_path,updated_at) VALUES(?,?,'active',?,?,?,?,?) ON CONFLICT(root_key) DO UPDATE SET store_id=excluded.store_id,mode='active',epoch=excluded.epoch,snapshot_json=excluded.snapshot_json,source_digest=excluded.source_digest,backup_path=excluded.backup_path,updated_at=excluded.updated_at",
        vec![key.into(),snapshot.store_id.clone().into(),snapshot.epoch.into(),super::authority::encode(&snapshot.data)?.into(),snapshot.digest.clone().into(),snapshot.backup_path.clone().into(),chrono::Utc::now().to_rfc3339().into()]).await?;
    for (index, (table, columns)) in TABLES.into_iter().enumerate() {
        let conflict = if index == 0 {
            "ON CONFLICT(root_key,record_id) DO UPDATE SET source_id=excluded.source_id,revision=excluded.revision,kind=excluded.kind,content_digest=excluded.content_digest,state=excluded.state"
        } else {
            "ON CONFLICT DO NOTHING"
        };
        let placeholders = std::iter::repeat_n("?", columns.split(',').count() + 1)
            .collect::<Vec<_>>()
            .join(",");
        let query =
            format!("INSERT INTO {table}(root_key,{columns}) VALUES({placeholders}) {conflict}");
        for row in &bundle.tables[index] {
            let mut values = vec![key.into()];
            values.extend(row.iter().map(|value| match value.as_i64() {
                Some(value) => value.into(),
                None => value.as_str().expect("validated recovery field").into(),
            }));
            sql::execute(db, &query, values).await?;
        }
    }
    super::authority::queue_projections(db, key, snapshot.epoch).await
}
