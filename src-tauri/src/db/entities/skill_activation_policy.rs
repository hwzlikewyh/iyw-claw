use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "skill_activation_policy")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub skill_id: String,
    pub scope: String,
    pub workspace_key: String,
    pub agent_type: String,
    pub requested_enabled: bool,
    pub policy_source: String,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
