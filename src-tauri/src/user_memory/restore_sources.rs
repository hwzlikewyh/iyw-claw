use super::{
    authority_export, authority_sql as sql, helpers,
    restore_bundle::{self, RecoveryBundle},
    MemoryRecoverySource, UserMemoryService,
};
use crate::app_error::AppCommandError;
use sea_orm::{DatabaseConnection, TransactionTrait};
use std::path::{Path, PathBuf};

const MAX_SOURCES: usize = 32;

pub(super) struct RecoverySource {
    pub id: String,
    pub label: String,
    pub bundle: RecoveryBundle,
}

impl UserMemoryService {
    pub(super) async fn memory_recovery_sources(
        &self,
    ) -> Result<Vec<MemoryRecoverySource>, AppCommandError> {
        Ok(self
            .recovery_sources_locked()
            .await?
            .into_iter()
            .map(|source| MemoryRecoverySource {
                id: source.id,
                label: source.label,
                epoch: source.bundle.snapshot.epoch,
                records: source.bundle.tables[0].len(),
                revisions: source.bundle.tables[1].len(),
            })
            .collect())
    }

    pub(super) async fn recovery_sources_locked(
        &self,
    ) -> Result<Vec<RecoverySource>, AppCommandError> {
        let Some(marker) = authority_export::marker(self.resolved_root()?)? else {
            return Ok(Vec::new());
        };
        let key = self.authority_key()?;
        let mut result = Vec::new();
        for path in self.recovery_paths().await? {
            let bundle = match read_bundle(&path, &key).await {
                Ok(Some(bundle)) => bundle,
                Ok(None) => continue,
                Err(error) => {
                    tracing::debug!(code=?error.code,"[memory-reconcile] unusable recovery candidate");
                    continue;
                }
            };
            if bundle.snapshot.mode != "active"
                || bundle.snapshot.store_id != marker.store_id
                || bundle.snapshot.epoch < marker.epoch
                || bundle.snapshot.epoch == marker.epoch && bundle.snapshot.digest != marker.digest
            {
                continue;
            }
            if super::restore_integrity::matches_snapshot(self, &bundle).is_err() {
                continue;
            }
            let id = helpers::hash_parts(&[
                path.to_string_lossy().as_bytes(),
                restore_bundle::fingerprint(&bundle)?.as_bytes(),
            ]);
            let label = path
                .parent()
                .and_then(Path::file_name)
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            result.push(RecoverySource { id, label, bundle });
        }
        result.sort_by(|a, b| b.bundle.snapshot.epoch.cmp(&a.bundle.snapshot.epoch));
        Ok(result)
    }

    async fn recovery_paths(&self) -> Result<Vec<PathBuf>, AppCommandError> {
        let rows = sql::rows(&self.db, "PRAGMA database_list", vec![]).await?;
        let main = rows
            .iter()
            .find(|row| sql::field::<String>(row, "name").ok().as_deref() == Some("main"))
            .ok_or_else(|| {
                AppCommandError::configuration_invalid("Memory database path unavailable")
            })?;
        let database = PathBuf::from(sql::field::<String>(main, "file")?);
        let Some(parent) = database.parent() else {
            return Ok(Vec::new());
        };
        let safety = parent.join(crate::commands::backup::restore::SAFETY_DIR);
        helpers::reject_symlink(&safety)?;
        if !safety.try_exists().map_err(AppCommandError::io)? {
            return Ok(Vec::new());
        }
        let mut directories = std::fs::read_dir(&safety)
            .map_err(AppCommandError::io)?
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        directories.sort();
        directories.reverse();
        Ok(directories
            .into_iter()
            .take(MAX_SOURCES)
            .map(|path| path.join(crate::db::database_file_name()))
            .filter(|path| path.is_file())
            .collect())
    }
}

pub(super) async fn read_bundle(
    path: &Path,
    key: &str,
) -> Result<Option<RecoveryBundle>, AppCommandError> {
    helpers::reject_symlink(path)?;
    let db = open_readonly(path).await?;
    let result = async {
        let txn = db
            .begin()
            .await
            .map_err(super::index_checkpoint::database_error)?;
        let check = sql::rows(&txn, "PRAGMA quick_check", vec![]).await?;
        if check.len() != 1 || sql::field::<String>(&check[0], "quick_check")? != "ok" {
            return Err(restore_bundle::invalid(
                "Recovery database integrity check failed",
            ));
        }
        restore_bundle::load(&txn, key).await
    }
    .await;
    db.close()
        .await
        .map_err(super::index_checkpoint::database_error)?;
    result
}

async fn open_readonly(path: &Path) -> Result<DatabaseConnection, AppCommandError> {
    let path = path.to_path_buf();
    let mut options = sea_orm::ConnectOptions::new("sqlite::memory:");
    options
        .max_connections(1)
        .min_connections(1)
        .sqlx_logging(false);
    options.map_sqlx_sqlite_opts(move |sqlite| {
        sqlite
            .filename(&path)
            .in_memory(false)
            .read_only(true)
            .create_if_missing(false)
    });
    sea_orm::Database::connect(options)
        .await
        .map_err(super::index_checkpoint::database_error)
}
