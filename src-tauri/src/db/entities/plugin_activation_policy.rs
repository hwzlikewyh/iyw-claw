use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "plugin_activation_policy")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub plugin_slug: String,
    pub component_key: String,
    pub scope: String,
    pub workspace_key: String,
    pub agent_type: String,
    pub requested_enabled: bool,
    pub routing_mode: String,
    pub policy_source: String,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
