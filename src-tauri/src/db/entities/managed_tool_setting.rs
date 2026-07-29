use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "managed_tool_setting")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub tool_id: String,
    pub update_channel: String,
    pub pinned_version: Option<String>,
    pub active_version: Option<String>,
    pub last_known_good_version: Option<String>,
    pub update_policy: String,
    pub catalog_revision: i64,
    pub activation_generation: i64,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
