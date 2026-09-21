use super::authority_types::AuthoritySnapshot;
use super::{authority_sql as sql, structured_file, UserMemoryService};
use crate::app_error::AppCommandError;
use sea_orm::TransactionTrait;
use serde::{Deserialize, Serialize};

const PENDING_FILE: &str = ".memory-authority-pending.json";
const PENDING_LIMIT: usize = 20_971_520;

pub(super) fn has_pending(root: &std::path::Path) -> Result<bool, AppCommandError> {
    root.join(PENDING_FILE)
        .try_exists()
        .map_err(AppCommandError::io)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PendingAuthority {
    previous_digest: String,
    next: AuthoritySnapshot,
    identity: Option<(String, String)>,
}

impl UserMemoryService {
    pub(super) fn prepare_authority_commit(
        &self,
        next: &AuthoritySnapshot,
        identity: Option<(&str, &str)>,
    ) -> Result<(), AppCommandError> {
        let current = self
            .authority_snapshot()
            .ok_or_else(|| super::helpers::conflict("Memory authority is missing"))?;
        let pending = PendingAuthority {
            previous_digest: current.digest,
            next: next.clone(),
            identity: identity.map(|(old, new)| (old.into(), new.into())),
        };
        structured_file::write_json_atomic(self.resolved_root()?, PENDING_FILE, &pending)
    }

    pub(super) fn finish_authority_commit(&self) {
        if let Err(error) = structured_file::remove_optional(
            self.resolved_root().expect("validated memory root"),
            PENDING_FILE,
        ) {
            tracing::warn!(code = ?error.code, "[memory-authority] committed journal cleanup deferred");
        }
    }

    pub(super) async fn recover_authority_commit(
        &self,
        snapshot: Option<AuthoritySnapshot>,
    ) -> Result<Option<AuthoritySnapshot>, AppCommandError> {
        let Some(pending) = structured_file::read_json_optional::<PendingAuthority>(
            self.resolved_root()?,
            PENDING_FILE,
            PENDING_LIMIT,
        )?
        else {
            return Ok(snapshot);
        };
        let current = snapshot.ok_or_else(|| {
            super::helpers::conflict(
                "Pending memory commit has no database; reconcile restore first",
            )
        })?;
        validate_pending(&pending, &current)?;
        if super::authority_export::marker(self.resolved_root()?)?.is_some_and(|marker| {
            marker.store_id != pending.next.store_id
                || marker.epoch > pending.next.epoch
                || (marker.epoch == pending.next.epoch && marker.digest != pending.next.digest)
        }) {
            return Err(super::helpers::conflict(
                "Memory journal is older than its authority marker",
            ));
        }
        if current.epoch < pending.next.epoch {
            self.replay_authority_commit(&pending).await?;
        }
        super::authority_export::write_fence(self.resolved_root()?, &pending.next)?;
        self.finish_authority_commit();
        tracing::info!(
            epoch = pending.next.epoch,
            "[memory-authority] recovered durable commit"
        );
        Ok(Some(pending.next))
    }

    async fn replay_authority_commit(
        &self,
        pending: &PendingAuthority,
    ) -> Result<(), AppCommandError> {
        let key = self.authority_key()?;
        let txn = self
            .db
            .begin()
            .await
            .map_err(super::index_checkpoint::database_error)?;
        let next = &pending.next;
        let changed = sql::execute(&txn,"UPDATE memory_authority SET mode='active',epoch=?,snapshot_json=?,source_digest=?,updated_at=? WHERE root_key=? AND epoch=? AND source_digest=?",
            vec![next.epoch.into(),super::authority::encode(&next.data)?.into(),next.digest.clone().into(),chrono::Utc::now().to_rfc3339().into(),key.clone().into(),(next.epoch-1).into(),pending.previous_digest.clone().into()]).await?;
        if changed != 1 {
            return Err(super::helpers::conflict(
                "Memory changed during commit recovery",
            ));
        }
        let rename = pending
            .identity
            .as_ref()
            .map(|(old, new)| (old.as_str(), new.as_str()));
        super::authority_records::persist(&txn, self, (&next.data, rename)).await?;
        super::authority::queue_projections(&txn, &key, next.epoch).await?;
        txn.commit()
            .await
            .map_err(super::index_checkpoint::database_error)
    }
}

fn validate_pending(
    pending: &PendingAuthority,
    current: &AuthoritySnapshot,
) -> Result<(), AppCommandError> {
    let next = &pending.next;
    super::authority_validation::validate(&next.data)?;
    let digest_matches = super::authority_types::digest(&next.data)? == next.digest;
    let committed =
        current.epoch == next.epoch && current.digest == next.digest && current.mode == "active";
    let prepared = current.epoch + 1 == next.epoch && current.digest == pending.previous_digest;
    if current.store_id != next.store_id
        || next.mode != "active"
        || !digest_matches
        || !(committed || prepared)
    {
        return Err(super::helpers::conflict(
            "Memory journal and database disagree; restore requires reconciliation",
        ));
    }
    Ok(())
}
