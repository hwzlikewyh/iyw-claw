use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "chat_channel_message_log")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub channel_id: i32,
    pub direction: String,
    pub message_type: String,
    pub content_preview: String,
    pub status: String,
    pub error_detail: Option<String>,
    /// End-to-end trace id: one value spans inbound → dispatcher → agent →
    /// outbound so a single log filter can reconstruct the whole round trip.
    pub trace_id: Option<String>,
    /// Provider-side message id (outbound idempotency key / inbound dedupe).
    pub provider_message_id: Option<String>,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::chat_channel::Entity",
        from = "Column::ChannelId",
        to = "super::chat_channel::Column::Id"
    )]
    ChatChannel,
}

impl Related<super::chat_channel::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ChatChannel.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
