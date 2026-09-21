use serde::{Deserialize, Serialize};

use crate::app_error::AppCommandError;

use super::{authority_sql as sql, MemoryRetention, UserMemoryService};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordMemoryRecallFeedbackRequest {
    pub receipt_id: i64,
    pub record_id: String,
    pub verdict: String,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEffectivenessStatus {
    pub deliveries: i64,
    pub reviewed: i64,
    pub used: i64,
    pub irrelevant: i64,
    pub outdated: i64,
    pub unreviewed: i64,
}

impl UserMemoryService {
    pub async fn record_memory_recall_feedback(
        &self,
        request: RecordMemoryRecallFeedbackRequest,
    ) -> Result<(), AppCommandError> {
        validate_request(&request)?;
        let (_guard, _file) = self.acquire_locks().await?;
        let key = self.authority_key()?;
        let context = receipt_context(self, &key, &request).await?;
        if request.verdict == "outdated" {
            self.retire_outdated_feedback(&request.record_id).await?;
        }
        sql::execute(&self.db,"INSERT INTO memory_recall_feedback(root_key,receipt_id,record_id,verdict,note,conversation_id,turn_nonce,recorded_at) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(root_key,receipt_id,record_id) DO UPDATE SET verdict=excluded.verdict,note=excluded.note,recorded_at=excluded.recorded_at",
            vec![key.into(),request.receipt_id.into(),request.record_id.into(),request.verdict.clone().into(),request.note.into(),context.0.into(),context.1.into(),chrono::Utc::now().to_rfc3339().into()]).await?;
        tracing::info!(verdict=%request.verdict,"[memory-feedback] recall feedback recorded");
        Ok(())
    }

    pub async fn memory_effectiveness_status(
        &self,
    ) -> Result<MemoryEffectivenessStatus, AppCommandError> {
        let key = self.authority_key()?;
        let receipts = sql::rows(
            &self.db,
            "SELECT items_json FROM memory_recall_receipt WHERE root_key=?",
            vec![key.clone().into()],
        )
        .await?;
        let deliveries = receipts.into_iter().try_fold(0i64, |count, row| {
            let raw: String = sql::field(&row, "items_json")?;
            let items: Vec<super::MemoryRecallVersion> = serde_json::from_str(&raw)
                .map_err(|_| AppCommandError::configuration_invalid("Invalid recall receipt"))?;
            Ok::<_, AppCommandError>(count.saturating_add(items.len() as i64))
        })?;
        let rows = sql::rows(&self.db,"SELECT COUNT(*) AS reviewed, SUM(CASE WHEN verdict='used' THEN 1 ELSE 0 END) AS used, SUM(CASE WHEN verdict='irrelevant' THEN 1 ELSE 0 END) AS irrelevant, SUM(CASE WHEN verdict='outdated' THEN 1 ELSE 0 END) AS outdated FROM memory_recall_feedback WHERE root_key=?",vec![key.into()]).await?;
        let row = &rows[0];
        let reviewed = sql::field::<i64>(row, "reviewed")?.max(0);
        Ok(MemoryEffectivenessStatus {
            deliveries,
            reviewed,
            used: optional_count(row, "used"),
            irrelevant: optional_count(row, "irrelevant"),
            outdated: optional_count(row, "outdated"),
            unreviewed: deliveries.saturating_sub(reviewed),
        })
    }

    async fn retire_outdated_feedback(&self, record_id: &str) -> Result<(), AppCommandError> {
        let policy = self.load_policy_unrecovered().await?;
        let settings = super::index_source::readonly_snapshot(self, &policy)?;
        let mut learning = self.read_learning_state()?;
        let key = self.authority_key()?;
        let rows = sql::rows(
            &self.db,
            "SELECT source_id FROM memory_record WHERE root_key=? AND record_id=? LIMIT 1",
            vec![key.into(), record_id.into()],
        )
        .await?;
        let source_id = rows
            .first()
            .map(|row| sql::field::<String>(row, "source_id"))
            .transpose()?
            .unwrap_or_else(|| record_id.to_string());
        let snapshot = super::index_parse::build_index_snapshot(&settings, Some(&learning));
        let Some(item) = snapshot.items.iter().find(|item| item.id == source_id) else {
            return Ok(());
        };
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        learning.retention.insert(
            item.id.clone(),
            MemoryRetention {
                content_digest: item.content_digest.clone(),
                source_revision: item.source_revision.clone(),
                expires_at: now.clone(),
                reason: "Marked outdated from a recall receipt".into(),
                updated_at: now,
            },
        );
        self.persist_learning_state(&learning).await?;
        self.schedule_index_refresh();
        Ok(())
    }
}

async fn receipt_context(
    service: &UserMemoryService,
    key: &str,
    request: &RecordMemoryRecallFeedbackRequest,
) -> Result<(Option<String>, Option<i64>), AppCommandError> {
    let rows = sql::rows(&service.db,"SELECT conversation_id,turn_nonce,items_json FROM memory_recall_receipt WHERE root_key=? AND id=?",vec![key.into(),request.receipt_id.into()]).await?;
    let row = rows
        .first()
        .ok_or_else(|| AppCommandError::not_found("Recall receipt not found"))?;
    let raw: String = sql::field(row, "items_json")?;
    let items: Vec<super::MemoryRecallVersion> = serde_json::from_str(&raw)
        .map_err(|_| AppCommandError::configuration_invalid("Invalid recall receipt"))?;
    if !items.iter().any(|item| item.record_id == request.record_id) {
        return Err(AppCommandError::invalid_input(
            "Memory was not delivered in this receipt",
        ));
    }
    Ok((
        sql::field(row, "conversation_id")?,
        sql::field(row, "turn_nonce")?,
    ))
}

fn validate_request(request: &RecordMemoryRecallFeedbackRequest) -> Result<(), AppCommandError> {
    let note = request.note.as_deref().unwrap_or("");
    if request.receipt_id <= 0
        || request.record_id.is_empty()
        || !matches!(request.verdict.as_str(), "used" | "irrelevant" | "outdated")
        || note.chars().count() > 500
        || super::helpers::contains_potential_secret(note)
    {
        return Err(AppCommandError::invalid_input(
            "Invalid memory recall feedback",
        ));
    }
    Ok(())
}

fn optional_count(row: &sea_orm::QueryResult, key: &str) -> i64 {
    sql::field::<i64>(row, key).unwrap_or(0).max(0)
}
