use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "chat_channel_agent_audit")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub agent_type: String,
    pub session_ref: String,
    pub operation: String,
    pub channel_id: Option<i32>,
    pub target_id: Option<String>,
    pub target_label: Option<String>,
    pub file_summary_json: Option<String>,
    pub status: String,
    pub error_code: Option<String>,
    pub request_id: String,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
