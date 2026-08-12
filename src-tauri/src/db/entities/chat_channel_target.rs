use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "chat_channel_target")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub channel_id: i32,
    pub target_id: String,
    pub target_kind: String,
    pub source: String,
    pub display_name: String,
    pub fingerprint: String,
    pub is_default: bool,
    pub first_seen_at: DateTimeUtc,
    pub last_seen_at: DateTimeUtc,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::chat_channel::Entity",
        from = "Column::ChannelId",
        to = "super::chat_channel::Column::Id",
        on_delete = "Cascade"
    )]
    ChatChannel,
}

impl Related<super::chat_channel::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ChatChannel.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
