use super::authority_types::AuthoritySnapshot;
use super::{authority_sql as sql, UserMemoryService};
use crate::app_error::AppCommandError;
use sea_orm::TransactionTrait;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AuthorityPurge {
    pub record_ids: Vec<String>,
    pub tombstones: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harvest_cutoff: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_harvest: Vec<super::MemoryHarvestRequest>,
}

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
        purge: &AuthorityPurge,
    ) -> Result<(), AppCommandError> {
        self.save_authority_transaction_inner(snapshot, None, Some(purge))
            .await
    }

    async fn save_authority_transaction_inner(
        &self,
        snapshot: &AuthoritySnapshot,
        identity: Option<(&str, &str)>,
        purge: Option<&AuthorityPurge>,
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
        if let Some(purge) = purge {
            apply_purge(&txn, &key, purge).await?;
        }
        super::authority::queue_projections(&txn, &key, snapshot.epoch).await?;
        self.prepare_authority_purge_commit(snapshot, (identity, purge))?;
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

pub(super) async fn apply_purge(
    txn: &sea_orm::DatabaseTransaction,
    key: &str,
    purge: &AuthorityPurge,
) -> Result<(), AppCommandError> {
    for source_hash in &purge.tombstones {
        sql::execute(txn,"INSERT INTO memory_forget_tombstone(root_key,source_hash,created_at,reason) VALUES(?,?,?,'user_requested_forget') ON CONFLICT(root_key,source_hash) DO NOTHING",
            vec![key.into(),source_hash.clone().into(),chrono::Utc::now().to_rfc3339().into()]).await?;
    }
    for stable_id in &purge.record_ids {
        purge_record(txn, key, stable_id).await?;
    }
    if let Some(cutoff) = purge.harvest_cutoff {
        sql::execute(txn,
            "UPDATE memory_harvest_outbox SET state='noop', noop_reason='memory_cleared', user_input_ref=NULL, assistant_input_ref=NULL, tool_outcome_ref=NULL, candidate_ids=NULL, experience_ids=NULL, failure_kind=NULL, failure_detail=NULL, next_attempt_at=NULL, updated_at=? WHERE id<=?",
            vec![chrono::Utc::now().to_rfc3339().into(), cutoff.into()]).await?;
    }
    for request in &purge.pending_harvest {
        sql::execute(txn,
            "INSERT INTO memory_harvest_outbox(dedup_key,conversation_id,turn_nonce,agent_type,submitted_at,state,noop_reason,updated_at) VALUES(?,?,?,?,?,'noop','memory_cleared',?) ON CONFLICT(dedup_key) DO NOTHING",
            vec![request.dedup_key().into(), request.conversation.clone().into(),
                (request.turn_nonce as i64).into(), super::harvest_store_sql::agent_name(request.agent_type).into(),
                request.submitted_at.clone().into(), chrono::Utc::now().to_rfc3339().into()]).await?;
    }
    super::forget_projection::clear_fts(txn).await
}

async fn purge_record(
    txn: &sea_orm::DatabaseTransaction,
    key: &str,
    stable_id: &str,
) -> Result<(), AppCommandError> {
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
