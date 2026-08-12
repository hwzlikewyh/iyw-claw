use chrono::Utc;
use sea_orm::{ActiveModelTrait, ActiveValue::NotSet, DatabaseConnection, Set};

use crate::db::entities::chat_channel_agent_audit;
use crate::db::error::DbError;

pub struct AuditRecord<'a> {
    pub agent_type: &'a str,
    pub session_ref: &'a str,
    pub operation: &'a str,
    pub channel_id: Option<i32>,
    pub target_id: Option<&'a str>,
    pub target_label: Option<&'a str>,
    pub file_summary_json: Option<String>,
    pub status: &'a str,
    pub error_code: Option<&'a str>,
    pub request_id: &'a str,
}

pub async fn create(conn: &DatabaseConnection, record: AuditRecord<'_>) -> Result<(), DbError> {
    chat_channel_agent_audit::ActiveModel {
        id: NotSet,
        agent_type: Set(record.agent_type.to_string()),
        session_ref: Set(record.session_ref.to_string()),
        operation: Set(record.operation.to_string()),
        channel_id: Set(record.channel_id),
        target_id: Set(record.target_id.map(str::to_string)),
        target_label: Set(record.target_label.map(str::to_string)),
        file_summary_json: Set(record.file_summary_json),
        status: Set(record.status.to_string()),
        error_code: Set(record.error_code.map(str::to_string)),
        request_id: Set(record.request_id.to_string()),
        created_at: Set(Utc::now()),
    }
    .insert(conn)
    .await?;
    Ok(())
}
