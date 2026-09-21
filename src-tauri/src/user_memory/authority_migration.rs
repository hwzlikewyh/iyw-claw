use super::authority_types::{AuthorityData, AuthoritySnapshot};
use super::{
    authority_sql as sql, ActivateMemoryAuthorityRequest, MemoryAuthorityStatus,
    ResourceGeneration, UserMemoryDocumentId, UserMemoryService,
};
use crate::app_error::AppCommandError;
use sea_orm::TransactionTrait;

impl UserMemoryService {
    pub async fn prepare_memory_authority(&self) -> Result<MemoryAuthorityStatus, AppCommandError> {
        let (_guard, _file) = self.acquire_locks().await?;
        self.recover_pending_transaction().await?;
        if self.active_authority().is_some() {
            return self.authority_status_locked().await;
        }
        let data = self.legacy_authority_data().await?;
        super::authority_validation::validate_import(&data)?;
        let digest = super::authority_types::digest(&data)?;
        let key = self.authority_key()?;
        let previous = self.authority_snapshot();
        if previous
            .as_ref()
            .is_some_and(|snapshot| snapshot.digest == digest)
        {
            super::authority_records::verify(&self.db, self, &data).await?;
            return self.authority_status_locked().await;
        }
        let backup = self.backup_authority_source(&data).await?;
        let snapshot = AuthoritySnapshot {
            store_id: previous
                .as_ref()
                .map(|value| value.store_id.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            mode: "shadow".into(),
            epoch: previous.map(|value| value.epoch + 1).unwrap_or(1),
            digest: digest.clone(),
            backup_path: backup,
            data,
        };
        let txn = self
            .db
            .begin()
            .await
            .map_err(super::index_checkpoint::database_error)?;
        sql::execute(&txn,"INSERT INTO memory_authority(root_key,store_id,mode,epoch,snapshot_json,source_digest,backup_path,updated_at) VALUES(?,?,'shadow',?,?,?,?,?) ON CONFLICT(root_key) DO UPDATE SET epoch=excluded.epoch,snapshot_json=excluded.snapshot_json,source_digest=excluded.source_digest,backup_path=excluded.backup_path,updated_at=excluded.updated_at WHERE memory_authority.mode='shadow'",
            vec![key.into(),snapshot.store_id.clone().into(),snapshot.epoch.into(),super::authority::encode(&snapshot.data)?.into(),digest.into(),snapshot.backup_path.clone().into(),chrono::Utc::now().to_rfc3339().into()]).await?;
        super::authority_records::persist(&txn, self, (&snapshot.data, None)).await?;
        super::authority_records::verify(&txn, self, &snapshot.data).await?;
        txn.commit()
            .await
            .map_err(super::index_checkpoint::database_error)?;
        *self
            .authority
            .write()
            .unwrap_or_else(|error| error.into_inner()) = Some(snapshot);
        tracing::info!("[memory-authority] backed up and imported shadow records");
        self.authority_status_locked().await
    }

    pub async fn activate_memory_authority(
        &self,
        request: ActivateMemoryAuthorityRequest,
    ) -> Result<MemoryAuthorityStatus, AppCommandError> {
        let (_guard, _file) = self.acquire_locks().await?;
        let mut snapshot = self
            .authority_snapshot()
            .ok_or_else(|| AppCommandError::invalid_input("Prepare shadow import first"))?;
        if snapshot.mode == "active" {
            return self.authority_status_locked().await;
        }
        if snapshot.digest != request.expected_revision
            || super::authority_types::digest(&self.legacy_authority_data().await?)?
                != snapshot.digest
        {
            return Err(super::helpers::conflict(
                "Memory changed after shadow import; prepare again",
            ));
        }
        super::authority_records::verify(&self.db, self, &snapshot.data).await?;
        self.activate_authority_locked(&mut snapshot).await?;
        self.authority_status_locked().await
    }

    async fn activate_authority_locked(
        &self,
        snapshot: &mut AuthoritySnapshot,
    ) -> Result<(), AppCommandError> {
        let txn = self
            .db
            .begin()
            .await
            .map_err(super::index_checkpoint::database_error)?;
        let count = sql::execute(&txn,"UPDATE memory_authority SET mode='active',epoch=epoch+1,updated_at=? WHERE root_key=? AND mode='shadow' AND source_digest=? AND epoch=?",
            vec![chrono::Utc::now().to_rfc3339().into(),self.authority_key()?.into(),snapshot.digest.clone().into(),snapshot.epoch.into()]).await?;
        if count != 1 {
            return Err(super::helpers::conflict(
                "Memory shadow import changed; reload",
            ));
        }
        snapshot.epoch += 1;
        snapshot.mode = "active".into();
        super::authority::queue_projections(&txn, &self.authority_key()?, snapshot.epoch).await?;
        self.prepare_authority_commit(&snapshot, None)?;
        self.commit_with_authority_fence(txn, &snapshot).await?;
        *self
            .authority
            .write()
            .unwrap_or_else(|error| error.into_inner()) = Some(snapshot.clone());
        if let Err(error) = self.export_authority_locked(&snapshot).await {
            tracing::warn!(code = ?error.code, "[memory-authority] activated; compatibility export deferred");
        }
        self.schedule_index_refresh();
        tracing::info!(
            epoch = snapshot.epoch,
            "[memory-authority] SQLite authority activated"
        );
        Ok(())
    }

    async fn legacy_authority_data(&self) -> Result<AuthorityData, AppCommandError> {
        let mut documents = std::collections::BTreeMap::new();
        for id in UserMemoryDocumentId::ALL {
            documents.insert(
                id,
                super::fs::read_document_optional(self.resolved_root()?, id)?
                    .map(super::transaction::document_resource)
                    .unwrap_or(ResourceGeneration::Absent),
            );
        }
        let learning = super::candidate_store::read_optional(self.resolved_root()?)?
            .map(super::transaction::candidate_resource)
            .transpose()?
            .unwrap_or(ResourceGeneration::Absent);
        Ok(AuthorityData {
            documents,
            learning,
            policy: self.load_policy_unrecovered().await?,
        })
    }

    pub async fn memory_authority_status(&self) -> Result<MemoryAuthorityStatus, AppCommandError> {
        let (_guard, _file) = self.acquire_locks().await?;
        self.authority_status_locked().await
    }

    async fn authority_status_locked(&self) -> Result<MemoryAuthorityStatus, AppCommandError> {
        let snapshot = self.authority_snapshot();
        let key = self.authority_key()?;
        let mut counts = Vec::new();
        for query in ["SELECT COUNT(*) AS count FROM memory_record WHERE root_key=?","SELECT COUNT(*) AS count FROM memory_revision WHERE root_key=?","SELECT COUNT(*) AS count FROM memory_projection_job WHERE root_key=? AND state='pending'"] {
            let rows=sql::rows(&self.db,query,vec![key.clone().into()]).await?;
            counts.push(sql::field::<i64>(&rows[0],"count")? as usize);
        }
        let external_changes = match snapshot.as_ref().filter(|value| value.mode == "active") {
            Some(value) => super::authority_export::external_changes(self.resolved_root()?, value)?,
            None => Vec::new(),
        };
        let mut kinds = std::collections::BTreeMap::new();
        for row in sql::rows(&self.db,"SELECT kind,COUNT(*) AS count FROM memory_record WHERE root_key=? AND state!='retracted' GROUP BY kind",vec![key.into()]).await? {
            kinds.insert(sql::field::<String>(&row,"kind")?,sql::field::<i64>(&row,"count")? as usize);
        }
        Ok(MemoryAuthorityStatus {
            mode: snapshot
                .as_ref()
                .map(|value| value.mode.clone())
                .unwrap_or("legacy".into()),
            epoch: snapshot.as_ref().map(|value| value.epoch),
            revision: snapshot.as_ref().map(|value| value.digest.clone()),
            backup_path: snapshot
                .map(|value| value.backup_path)
                .filter(|path| std::path::Path::new(path).is_file()),
            records: counts[0],
            revisions: counts[1],
            pending_projections: counts[2],
            external_changes,
            counts: kinds,
        })
    }
}
