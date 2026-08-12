use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "chat_channel_tool_request")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub caller_scope: String,
    pub operation: String,
    pub request_id: String,
    pub input_digest: String,
    pub status: String,
    pub result_json: Option<String>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
