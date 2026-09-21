use super::{
    authority_export as export, authority_sql as sql, helpers, structured_file,
    MemoryConflictAction, MemoryReconciliation, MemoryReconciliationResult,
    ResolveMemoryFileRequest, UserMemoryService,
};
use crate::app_error::AppCommandError;

impl UserMemoryService {
    pub async fn memory_reconciliation(&self) -> Result<MemoryReconciliation, AppCommandError> {
        let (_guard, _file) = self.acquire_reconciliation_locks().await?;
        self.reconciliation_locked().await
    }

    pub(super) async fn reconciliation_locked(
        &self,
    ) -> Result<MemoryReconciliation, AppCommandError> {
        let snapshot = sql::load(&self.db, &self.authority_key()?).await?;
        let marker = export::marker(self.resolved_root()?)?;
        let blocked = export::validate_marker(self.resolved_root()?, snapshot.as_ref()).is_err();
        let files = match snapshot.as_ref().filter(|value| value.mode == "active") {
            Some(snapshot) if !blocked => super::reconcile_files::conflicts(self, snapshot)?,
            _ => Vec::new(),
        };
        let recovery_sources = if blocked {
            self.memory_recovery_sources().await?
        } else {
            Vec::new()
        };
        let revision = helpers::hash_parts(&[super::authority::encode(&serde_json::json!({
            "database":snapshot.as_ref().map(|s|(&s.store_id,s.epoch,&s.digest)), "marker":marker,
            "files":files.iter().map(|f|(&f.name,&f.external_digest)).collect::<Vec<_>>()
        }))?
        .as_bytes()]);
        Ok(MemoryReconciliation {
            revision,
            mode: if blocked {
                "restore_required"
            } else if files.is_empty() {
                "healthy"
            } else {
                "external_conflict"
            }
            .into(),
            database_epoch: snapshot.map(|s| s.epoch),
            required_epoch: marker.map(|m| m.epoch),
            files,
            warnings: if blocked && recovery_sources.is_empty() {
                vec!["recovery_evidence_missing".into()]
            } else {
                Vec::new()
            },
            recovery_sources,
        })
    }

    pub async fn resolve_memory_file(
        &self,
        request: ResolveMemoryFileRequest,
    ) -> Result<MemoryReconciliationResult, AppCommandError> {
        let (_guard, _file) = self.acquire_locks().await?;
        let preview = self.reconciliation_locked().await?;
        if preview.revision != request.expected_revision {
            return Err(helpers::conflict(
                "Memory or external files changed; refresh reconciliation",
            ));
        }
        let conflict = preview
            .files
            .iter()
            .find(|file| file.name == request.name)
            .ok_or_else(|| AppCommandError::not_found("Memory file conflict no longer exists"))?;
        let snapshot = self
            .active_authority()
            .ok_or_else(|| helpers::conflict("Memory authority is not active"))?;
        let external = export::read_export(self.resolved_root()?, &request.name)?;
        if super::reconcile_files::digest(external.as_deref()) != conflict.external_digest {
            return Err(helpers::conflict("External file changed after preview"));
        }
        let change = self.prepare_file_resolution(
            &snapshot,
            conflict,
            (&request.action, external.as_deref()),
        )?;
        let backup_path = self.archive_memory_conflict(&request.name, external.as_deref())?;
        if export::read_export(self.resolved_root()?, &request.name)? != external {
            return Err(helpers::conflict("External file changed during backup"));
        }
        if let Some((previous, next)) = change {
            self.commit_authority_change(&previous, &next, None).await?;
        }
        let snapshot = self.active_authority().expect("validated authority");
        self.resolve_export_file(&snapshot, (&request.name, external.as_deref()))?;
        self.export_after_resolution(&snapshot).await?;
        tracing::info!(
            epoch = snapshot.epoch,
            "[memory-reconcile] external file resolution completed"
        );
        Ok(MemoryReconciliationResult { backup_path })
    }

    async fn export_after_resolution(
        &self,
        snapshot: &super::authority_types::AuthoritySnapshot,
    ) -> Result<(), AppCommandError> {
        self.export_authority_locked(snapshot)
            .await
            .or_else(|error| {
                if export::external_changes(self.resolved_root()?, snapshot)?.is_empty() {
                    Err(error)
                } else {
                    Ok(())
                }
            })
    }

    fn prepare_file_resolution(
        &self,
        snapshot: &super::authority_types::AuthoritySnapshot,
        conflict: &super::MemoryFileConflict,
        input: (&MemoryConflictAction, Option<&str>),
    ) -> Result<Option<(super::UserMemoryGeneration, super::UserMemoryGeneration)>, AppCommandError>
    {
        match input.0 {
            MemoryConflictAction::KeepCurrent => Ok(None),
            MemoryConflictAction::ImportPaused => {
                let id = conflict
                    .document
                    .filter(|_| conflict.import_allowed)
                    .ok_or_else(|| {
                        AppCommandError::invalid_input(
                            "This external file requires dedicated memory editing",
                        )
                    })?;
                Ok(Some(super::reconcile_files::prepare_import(
                    self,
                    snapshot,
                    (id, input.1.unwrap_or_default()),
                )?))
            }
        }
    }

    pub(super) fn archive_memory_conflict(
        &self,
        name: &str,
        external: Option<&str>,
    ) -> Result<String, AppCommandError> {
        let file = format!(".memory-conflict-{}.json", uuid::Uuid::new_v4().simple());
        let payload = serde_json::json!({"file":name,"content":external,"recordedAt":chrono::Utc::now().to_rfc3339()});
        structured_file::install_new_private(
            self.resolved_root()?,
            &file,
            super::authority::encode(&payload)?.as_bytes(),
        )?;
        Ok(self
            .resolved_root()?
            .join(file)
            .to_string_lossy()
            .into_owned())
    }

    fn resolve_export_file(
        &self,
        snapshot: &super::authority_types::AuthoritySnapshot,
        file: (&str, Option<&str>),
    ) -> Result<(), AppCommandError> {
        let (name, expected) = file;
        if export::read_export(self.resolved_root()?, name)?.as_deref() != expected {
            return Err(helpers::conflict(
                "External file changed during reconciliation; original edit preserved",
            ));
        }
        let contents = export::export_contents(snapshot)?;
        match contents.get(name).and_then(|value| value.as_deref()) {
            Some(text) => {
                structured_file::write_bytes_atomic(self.resolved_root()?, name, text.as_bytes())
            }
            None => structured_file::remove_optional(self.resolved_root()?, name),
        }
    }
}
