use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "plugin_installation")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub market_skill_id: i64,
    pub slug: String,
    pub version: String,
    pub install_root: String,
    pub status: String,
    pub content_sha256: String,
    pub object_sha256: String,
    pub agent_types_json: String,
    pub manifest_json: String,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::plugin_component_ownership::Entity")]
    Components,
}

impl Related<super::plugin_component_ownership::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Components.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
