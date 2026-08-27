use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "plugin_app_instance")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub instance_id: String,
    pub conversation_id: i64,
    pub tool_call_id: String,
    pub plugin_slug: String,
    pub plugin_version: String,
    pub app_key: String,
    pub workspace_key: String,
    pub launch_payload_json: String,
    pub state: String,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
