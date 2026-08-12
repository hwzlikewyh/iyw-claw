use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, ColumnTrait, DatabaseConnection, DatabaseTransaction,
    EntityTrait, IntoActiveModel, QueryFilter, Set, TransactionTrait,
};

use crate::db::entities::{plugin_component_ownership, plugin_installation};
use crate::db::error::DbError;

#[derive(Debug, Clone)]
pub struct PluginInstallationRecord {
    pub installation: plugin_installation::Model,
    pub components: Vec<plugin_component_ownership::Model>,
}

#[derive(Debug, Clone)]
pub struct PluginInstallationInput {
    pub market_skill_id: i64,
    pub slug: String,
    pub version: String,
    pub install_root: String,
    pub status: String,
    pub content_sha256: String,
    pub object_sha256: String,
    pub agent_types_json: String,
    pub manifest_json: String,
    pub components: Vec<PluginComponentInput>,
}

#[derive(Debug, Clone)]
pub struct PluginComponentInput {
    pub component_type: String,
    pub component_key: String,
    pub managed_resource_key: String,
    pub relative_path: Option<String>,
    pub server_key: Option<String>,
}

pub async fn find_by_market_skill_id(
    conn: &DatabaseConnection,
    market_skill_id: i64,
) -> Result<Option<PluginInstallationRecord>, DbError> {
    let Some(installation) = plugin_installation::Entity::find()
        .filter(plugin_installation::Column::MarketSkillId.eq(market_skill_id))
        .one(conn)
        .await?
    else {
        return Ok(None);
    };
    let components = plugin_component_ownership::Entity::find()
        .filter(plugin_component_ownership::Column::PluginInstallationId.eq(installation.id))
        .all(conn)
        .await?;
    Ok(Some(PluginInstallationRecord {
        installation,
        components,
    }))
}

pub async fn list_installations(
    conn: &DatabaseConnection,
) -> Result<Vec<plugin_installation::Model>, DbError> {
    plugin_installation::Entity::find()
        .all(conn)
        .await
        .map_err(Into::into)
}

pub async fn replace(
    conn: &DatabaseConnection,
    input: PluginInstallationInput,
) -> Result<PluginInstallationRecord, DbError> {
    let transaction = conn.begin().await?;
    let result = replace_in_transaction(&transaction, input).await?;
    transaction.commit().await?;
    Ok(result)
}

async fn replace_in_transaction(
    transaction: &DatabaseTransaction,
    input: PluginInstallationInput,
) -> Result<PluginInstallationRecord, DbError> {
    let now = Utc::now();
    let existing = plugin_installation::Entity::find()
        .filter(plugin_installation::Column::MarketSkillId.eq(input.market_skill_id))
        .one(transaction)
        .await?;
    let installation = match existing {
        Some(model) => update_installation(transaction, model, &input, now).await?,
        None => insert_installation(transaction, &input, now).await?,
    };
    plugin_component_ownership::Entity::delete_many()
        .filter(plugin_component_ownership::Column::PluginInstallationId.eq(installation.id))
        .exec(transaction)
        .await?;
    let components = insert_components(transaction, installation.id, input.components, now).await?;
    Ok(PluginInstallationRecord {
        installation,
        components,
    })
}

async fn insert_installation(
    transaction: &DatabaseTransaction,
    input: &PluginInstallationInput,
    now: chrono::DateTime<Utc>,
) -> Result<plugin_installation::Model, DbError> {
    plugin_installation::ActiveModel {
        id: NotSet,
        market_skill_id: Set(input.market_skill_id),
        slug: Set(input.slug.clone()),
        version: Set(input.version.clone()),
        install_root: Set(input.install_root.clone()),
        status: Set(input.status.clone()),
        content_sha256: Set(input.content_sha256.clone()),
        object_sha256: Set(input.object_sha256.clone()),
        agent_types_json: Set(input.agent_types_json.clone()),
        manifest_json: Set(input.manifest_json.clone()),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(transaction)
    .await
    .map_err(Into::into)
}

async fn update_installation(
    transaction: &DatabaseTransaction,
    model: plugin_installation::Model,
    input: &PluginInstallationInput,
    now: chrono::DateTime<Utc>,
) -> Result<plugin_installation::Model, DbError> {
    let mut active = model.into_active_model();
    active.slug = Set(input.slug.clone());
    active.version = Set(input.version.clone());
    active.install_root = Set(input.install_root.clone());
    active.status = Set(input.status.clone());
    active.content_sha256 = Set(input.content_sha256.clone());
    active.object_sha256 = Set(input.object_sha256.clone());
    active.agent_types_json = Set(input.agent_types_json.clone());
    active.manifest_json = Set(input.manifest_json.clone());
    active.updated_at = Set(now);
    active.update(transaction).await.map_err(Into::into)
}

async fn insert_components(
    transaction: &DatabaseTransaction,
    installation_id: i32,
    values: Vec<PluginComponentInput>,
    now: chrono::DateTime<Utc>,
) -> Result<Vec<plugin_component_ownership::Model>, DbError> {
    let mut result = Vec::with_capacity(values.len());
    for value in values {
        result.push(
            plugin_component_ownership::ActiveModel {
                id: NotSet,
                plugin_installation_id: Set(installation_id),
                component_type: Set(value.component_type),
                component_key: Set(value.component_key),
                managed_resource_key: Set(value.managed_resource_key),
                relative_path: Set(value.relative_path),
                server_key: Set(value.server_key),
                created_at: Set(now),
            }
            .insert(transaction)
            .await?,
        );
    }
    Ok(result)
}

pub async fn delete_by_market_skill_id(
    conn: &DatabaseConnection,
    market_skill_id: i64,
) -> Result<u64, DbError> {
    let result = plugin_installation::Entity::delete_many()
        .filter(plugin_installation::Column::MarketSkillId.eq(market_skill_id))
        .exec(conn)
        .await?;
    Ok(result.rows_affected)
}
