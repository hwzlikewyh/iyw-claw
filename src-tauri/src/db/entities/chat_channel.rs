use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "chat_channel")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub name: String,
    pub channel_type: String,
    pub enabled: bool,
    pub config_json: String,
    pub event_filter_json: Option<String>,
    pub daily_report_enabled: bool,
    pub daily_report_time: Option<String>,
    /// Desired/configured intent is `enabled`; `runtime_status` records the
    /// last observed runtime state so "已启用但未连接" is representable.
    pub runtime_status: String,
    pub last_error: Option<String>,
    pub last_error_at: Option<DateTimeUtc>,
    pub last_connected_at: Option<DateTimeUtc>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::chat_channel_message_log::Entity")]
    MessageLogs,
}

impl Related<super::chat_channel_message_log::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::MessageLogs.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
