use super::{authority_sql as sql, UserMemoryRecallResult, UserMemoryService};
use crate::app_error::AppCommandError;
use serde::{Deserialize, Serialize};

const MAX_RECEIPTS: i64 = 2_048;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecallVersion {
    pub record_id: String,
    pub source_id: String,
    pub revision: i64,
    pub source_revision: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecallReceipt {
    pub id: i64,
    pub conversation_id: Option<String>,
    pub turn_nonce: Option<i64>,
    pub recorded_at: String,
    pub items: Vec<MemoryRecallVersion>,
    pub feedback: Option<String>,
}

impl UserMemoryService {
    pub async fn recalled_versions(
        &self,
        result: &UserMemoryRecallResult,
    ) -> Result<Vec<MemoryRecallVersion>, AppCommandError> {
        let (_guard, _file) = self.acquire_locks().await?;
        if self.active_authority().is_none() || result.items.is_empty() {
            return Ok(Vec::new());
        }
        let policy = self.load_policy_unrecovered().await?;
        let settings = super::index_source::readonly_snapshot(self, &policy)?;
        let learning = self.read_learning_optional()?;
        let digest = self.scoped_source_digest(&super::index_parse::source_digest(
            &settings,
            learning.as_ref(),
        ));
        if result.source_digest.as_deref() != Some(&digest) {
            return Err(super::helpers::conflict(
                "Recalled memory changed before version capture",
            ));
        }
        let key = self.authority_key()?;
        let mut versions = Vec::new();
        for item in &result.items {
            let rows = sql::rows(&self.db,"SELECT record_id,revision FROM memory_record WHERE root_key=? AND source_id=? AND state!='retracted'",vec![key.clone().into(),item.id.clone().into()]).await?;
            if let Some(row) = rows.first() {
                versions.push(MemoryRecallVersion {
                    record_id: sql::field(row, "record_id")?,
                    source_id: item.id.clone(),
                    revision: sql::field(row, "revision")?,
                    source_revision: item.source_revision.clone(),
                });
            }
        }
        Ok(versions)
    }

    pub async fn record_recall_delivery(
        &self,
        versions: Vec<MemoryRecallVersion>,
        context: (Option<i32>, Option<u64>),
    ) -> Result<(), AppCommandError> {
        if versions.is_empty() {
            return Ok(());
        }
        let key = self.authority_key()?;
        let nonce = context
            .1
            .map(i64::try_from)
            .transpose()
            .map_err(|_| AppCommandError::invalid_input("Invalid recall turn"))?;
        sql::execute(&self.db,"INSERT INTO memory_recall_receipt(root_key,conversation_id,turn_nonce,items_json,recorded_at) VALUES(?,?,?,?,?)",
            vec![key.clone().into(),context.0.map(|id|id.to_string()).into(),nonce.into(),super::authority::encode(&versions)?.into(),chrono::Utc::now().to_rfc3339().into()]).await?;
        sql::execute(&self.db,"DELETE FROM memory_recall_receipt WHERE root_key=? AND id NOT IN (SELECT id FROM memory_recall_receipt WHERE root_key=? ORDER BY id DESC LIMIT ?)",vec![key.clone().into(),key.into(),MAX_RECEIPTS.into()]).await?;
        sql::execute(&self.db,"DELETE FROM memory_recall_feedback WHERE root_key=? AND receipt_id NOT IN (SELECT id FROM memory_recall_receipt WHERE root_key=?)",vec![self.authority_key()?.into(),self.authority_key()?.into()]).await?;
        Ok(())
    }

    pub async fn memory_recall_receipts(
        &self,
        id: String,
    ) -> Result<Vec<MemoryRecallReceipt>, AppCommandError> {
        if id.is_empty() || id.len() > 128 {
            return Err(AppCommandError::invalid_input("Invalid memory identity"));
        }
        let (_guard, _file) = self.acquire_locks().await?;
        let key = self.authority_key()?;
        let stable = super::authority_records::identity(&self.db, &key, &id)
            .await?
            .unwrap_or(id);
        let rows = sql::rows(&self.db,"SELECT id,conversation_id,turn_nonce,items_json,recorded_at FROM memory_recall_receipt WHERE root_key=? ORDER BY id DESC LIMIT ?",vec![key.clone().into(),MAX_RECEIPTS.into()]).await?;
        let mut receipts = Vec::new();
        for row in rows {
            let raw: String = sql::field(&row, "items_json")?;
            let items: Vec<MemoryRecallVersion> = serde_json::from_str(&raw)
                .map_err(|_| AppCommandError::configuration_invalid("Invalid memory receipt"))?;
            let items = items
                .into_iter()
                .filter(|item| item.record_id == stable)
                .collect::<Vec<_>>();
            if items.is_empty() {
                continue;
            }
            receipts.push(MemoryRecallReceipt {
                id: sql::field(&row, "id")?,
                conversation_id: sql::field(&row, "conversation_id")?,
                turn_nonce: sql::field(&row, "turn_nonce")?,
                recorded_at: sql::field(&row, "recorded_at")?,
                items,
                feedback: feedback_for(&self.db, &key, sql::field(&row, "id")?, &stable).await?,
            });
            if receipts.len() == 20 {
                break;
            }
        }
        Ok(receipts)
    }
}

async fn feedback_for(
    db: &sea_orm::DatabaseConnection,
    key: &str,
    receipt_id: i64,
    record_id: &str,
) -> Result<Option<String>, AppCommandError> {
    let rows = sql::rows(db,"SELECT verdict FROM memory_recall_feedback WHERE root_key=? AND receipt_id=? AND record_id=? LIMIT 1",vec![key.into(),receipt_id.into(),record_id.into()]).await?;
    rows.first()
        .map(|row| sql::field(row, "verdict"))
        .transpose()
}
