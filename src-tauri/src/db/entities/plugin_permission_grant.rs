use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "plugin_permission_grant")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub plugin_slug: String,
    pub scope: String,
    pub workspace_key: String,
    pub permissions_digest: String,
    pub granted_permissions_json: String,
    pub grant_state: String,
    pub granted_at: Option<DateTimeUtc>,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
