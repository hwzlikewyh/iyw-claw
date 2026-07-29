use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "managed_tool_installation")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub tool_id: String,
    pub version: String,
    pub runtime: String,
    pub target: String,
    pub arch: String,
    pub origin: String,
    pub status: String,
    pub artifact_id: Option<String>,
    pub expected_sha256: Option<String>,
    pub verified: bool,
    pub failure_code: Option<String>,
    pub installed_at: Option<DateTimeUtc>,
    pub verified_at: Option<DateTimeUtc>,
    pub activated_at: Option<DateTimeUtc>,
    pub last_successful_use_at: Option<DateTimeUtc>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
