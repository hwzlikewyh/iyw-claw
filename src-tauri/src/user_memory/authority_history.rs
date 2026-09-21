use super::{authority_sql as sql, MemoryRevisionEntry, UserMemoryService};
use crate::app_error::AppCommandError;

impl UserMemoryService {
    pub async fn memory_revision_history(
        &self,
        id: String,
    ) -> Result<Vec<MemoryRevisionEntry>, AppCommandError> {
        if id.is_empty() || id.len() > 128 {
            return Err(AppCommandError::invalid_input(
                "Invalid memory history identity",
            ));
        }
        let (_guard, _file) = self.acquire_locks().await?;
        let key = self.authority_key()?;
        let stable = super::authority_records::identity(&self.db, &key, &id)
            .await?
            .unwrap_or(id);
        let rows=sql::rows(&self.db,"SELECT record_id,source_id,revision,state,item_json,reason,recorded_at FROM memory_revision WHERE root_key=? AND record_id=? ORDER BY revision DESC LIMIT 100",vec![key.into(),stable.into()]).await?;
        let mut history = Vec::new();
        for row in rows {
            let raw: String = sql::field(&row, "item_json")?;
            let payload: super::authority_records::RecordPayload = serde_json::from_str(&raw)
                .map_err(|_| AppCommandError::configuration_invalid("Invalid memory revision"))?;
            let sources =
                super::authority_sources::history_sources(&self.db, &payload.item.evidence).await?;
            let item = if payload.item.sensitive || super::helpers::contains_potential_secret(&raw)
            {
                serde_json::json!({"redacted":true})
            } else {
                serde_json::to_value(payload)
                    .map_err(|_| AppCommandError::configuration_invalid("Invalid revision"))?
            };
            history.push(MemoryRevisionEntry {
                record_id: sql::field(&row, "record_id")?,
                source_id: sql::field(&row, "source_id")?,
                revision: sql::field(&row, "revision")?,
                state: sql::field(&row, "state")?,
                reason: sql::field(&row, "reason")?,
                recorded_at: sql::field(&row, "recorded_at")?,
                item,
                sources,
            });
        }
        Ok(history)
    }
}
