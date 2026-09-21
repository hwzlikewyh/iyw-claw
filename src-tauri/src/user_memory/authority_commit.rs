use super::authority_types::AuthoritySnapshot;
use super::{authority_sql as sql, UserMemoryService};
use crate::app_error::AppCommandError;
use sea_orm::TransactionTrait;

impl UserMemoryService {
    pub(super) async fn save_authority_transaction(
        &self,
        snapshot: &AuthoritySnapshot,
        identity: Option<(&str, &str)>,
    ) -> Result<(), AppCommandError> {
        self.save_authority_transaction_inner(snapshot, identity, None)
            .await
    }

    pub(super) async fn save_authority_forget_transaction(
        &self,
        snapshot: &AuthoritySnapshot,
        stable_id: &str,
        tombstones: &[String],
    ) -> Result<(), AppCommandError> {
        self.save_authority_transaction_inner(snapshot, None, Some((stable_id, tombstones)))
            .await
    }

    async fn save_authority_transaction_inner(
        &self,
        snapshot: &AuthoritySnapshot,
        identity: Option<(&str, &str)>,
        purge: Option<(&str, &[String])>,
    ) -> Result<(), AppCommandError> {
        let key = self.authority_key()?;
        let txn = self
            .db
            .begin()
            .await
            .map_err(super::index_checkpoint::database_error)?;
        let count = sql::execute(&txn,
            "UPDATE memory_authority SET epoch=?,snapshot_json=?,source_digest=?,updated_at=? WHERE root_key=? AND epoch=? AND mode='active'",
            vec![snapshot.epoch.into(), super::authority::encode(&snapshot.data)?.into(),
                snapshot.digest.clone().into(), chrono::Utc::now().to_rfc3339().into(),
                key.clone().into(), (snapshot.epoch - 1).into()]).await?;
        if count != 1 {
            return Err(super::helpers::conflict("Memory authority changed; reload"));
        }
        super::authority_records::persist(&txn, self, (&snapshot.data, identity)).await?;
        if let Some((stable_id, tombstones)) = purge {
            purge_record(&txn, &key, stable_id, tombstones).await?;
        }
        super::authority::queue_projections(&txn, &key, snapshot.epoch).await?;
        self.prepare_authority_commit(snapshot, identity)?;
        self.commit_with_authority_fence(txn, snapshot).await
    }

    pub(super) async fn commit_with_authority_fence(
        &self,
        txn: sea_orm::DatabaseTransaction,
        snapshot: &AuthoritySnapshot,
    ) -> Result<(), AppCommandError> {
        let root = self.resolved_root()?;
        super::authority_export::write_fence(root, snapshot)?;
        txn.commit()
            .await
            .map_err(super::index_checkpoint::database_error)?;
        self.finish_authority_commit();
        Ok(())
    }
}

async fn purge_record(
    txn: &sea_orm::DatabaseTransaction,
    key: &str,
    stable_id: &str,
    tombstones: &[String],
) -> Result<(), AppCommandError> {
    for source_hash in tombstones {
        sql::execute(txn,"INSERT INTO memory_forget_tombstone(root_key,source_hash,created_at,reason) VALUES(?,?,?,'user_requested_forget') ON CONFLICT(root_key,source_hash) DO NOTHING",
            vec![key.into(),source_hash.clone().into(),chrono::Utc::now().to_rfc3339().into()]).await?;
    }
    sql::execute(
        txn,
        "DELETE FROM memory_recall_feedback WHERE root_key=? AND record_id=?",
        vec![key.into(), stable_id.into()],
    )
    .await?;
    sql::execute(
        txn,
        "DELETE FROM memory_recall_receipt WHERE root_key=? AND instr(items_json,?)>0",
        vec![key.into(), stable_id.into()],
    )
    .await?;
    sql::execute(
        txn,
        "DELETE FROM memory_revision WHERE root_key=? AND record_id=?",
        vec![key.into(), stable_id.into()],
    )
    .await?;
    sql::execute(
        txn,
        "DELETE FROM memory_identity WHERE root_key=? AND record_id=?",
        vec![key.into(), stable_id.into()],
    )
    .await?;
    sql::execute(
        txn,
        "DELETE FROM memory_record WHERE root_key=? AND record_id=?",
        vec![key.into(), stable_id.into()],
    )
    .await?;
    Ok(())
}
