use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "plugin_component_ownership")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub plugin_installation_id: i32,
    pub component_type: String,
    pub component_key: String,
    pub managed_resource_key: String,
    pub relative_path: Option<String>,
    pub server_key: Option<String>,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::plugin_installation::Entity",
        from = "Column::PluginInstallationId",
        to = "super::plugin_installation::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    PluginInstallation,
}

impl Related<super::plugin_installation::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::PluginInstallation.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
