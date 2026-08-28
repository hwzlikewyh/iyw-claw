use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, ColumnTrait, DatabaseTransaction, EntityTrait,
    QueryFilter, Set,
};

use crate::db::entities::{plugin_activation_policy, plugin_permission_grant};
use crate::db::error::DbError;

#[derive(Debug, Clone, Default)]
pub struct PluginRuntimeStateInput {
    pub activations: Vec<PluginActivationInput>,
    pub permission_grants: Vec<PluginPermissionGrantInput>,
}

#[derive(Debug, Clone)]
pub struct PluginActivationInput {
    pub component_key: String,
    pub scope: String,
    pub workspace_key: String,
    pub agent_type: String,
    pub requested_enabled: bool,
    pub routing_mode: String,
    pub policy_source: String,
}

#[derive(Debug, Clone)]
pub struct PluginPermissionGrantInput {
    pub scope: String,
    pub workspace_key: String,
    pub permissions_digest: String,
    pub granted_permissions_json: String,
    pub permission_ceiling_json: String,
    pub grant_state: String,
    pub granted_at: Option<chrono::DateTime<Utc>>,
}

pub(crate) async fn preserve_existing_in_transaction(
    transaction: &DatabaseTransaction,
    plugin_slug: &str,
    mut input: PluginRuntimeStateInput,
) -> Result<PluginRuntimeStateInput, DbError> {
    let activations = plugin_activation_policy::Entity::find()
        .filter(plugin_activation_policy::Column::PluginSlug.eq(plugin_slug))
        .all(transaction)
        .await?;
    for value in &mut input.activations {
        if let Some(existing) = activations
            .iter()
            .find(|existing| same_activation(value, existing))
        {
            value.requested_enabled = existing.requested_enabled;
            value.policy_source = existing.policy_source.clone();
        }
    }
    for existing in &activations {
        if !input
            .activations
            .iter()
            .any(|value| same_activation(value, existing))
        {
            input.activations.push(activation_from_model(existing));
        }
    }
    let grants = plugin_permission_grant::Entity::find()
        .filter(plugin_permission_grant::Column::PluginSlug.eq(plugin_slug))
        .all(transaction)
        .await?;
    for value in &mut input.permission_grants {
        if let Some(existing) = grants.iter().find(|existing| {
            existing.scope == value.scope
                && existing.workspace_key == value.workspace_key
                && existing.permissions_digest == value.permissions_digest
        }) {
            value.granted_permissions_json = existing.granted_permissions_json.clone();
            value.grant_state = existing.grant_state.clone();
            value.granted_at = existing.granted_at;
            continue;
        }
        if let Some(existing) = grants.iter().find(|existing| {
            existing.scope == value.scope
                && existing.workspace_key == value.workspace_key
                && existing.grant_state == "granted"
                && permission_subset(
                    &value.permission_ceiling_json,
                    &existing.granted_permissions_json,
                )
        }) {
            value.granted_permissions_json = value.permission_ceiling_json.clone();
            value.grant_state = "granted".to_string();
            value.granted_at = existing.granted_at;
        }
    }
    for existing in &grants {
        if !input
            .permission_grants
            .iter()
            .any(|value| same_grant_scope(value, existing))
        {
            input.permission_grants.push(grant_from_model(existing));
        }
    }
    Ok(input)
}

fn same_activation(
    input: &PluginActivationInput,
    existing: &plugin_activation_policy::Model,
) -> bool {
    input.component_key == existing.component_key
        && input.scope == existing.scope
        && input.workspace_key == existing.workspace_key
        && input.agent_type == existing.agent_type
}

fn same_grant_scope(
    input: &PluginPermissionGrantInput,
    existing: &plugin_permission_grant::Model,
) -> bool {
    input.scope == existing.scope && input.workspace_key == existing.workspace_key
}

fn activation_from_model(value: &plugin_activation_policy::Model) -> PluginActivationInput {
    PluginActivationInput {
        component_key: value.component_key.clone(),
        scope: value.scope.clone(),
        workspace_key: value.workspace_key.clone(),
        agent_type: value.agent_type.clone(),
        requested_enabled: value.requested_enabled,
        routing_mode: value.routing_mode.clone(),
        policy_source: value.policy_source.clone(),
    }
}

fn grant_from_model(value: &plugin_permission_grant::Model) -> PluginPermissionGrantInput {
    PluginPermissionGrantInput {
        scope: value.scope.clone(),
        workspace_key: value.workspace_key.clone(),
        permissions_digest: value.permissions_digest.clone(),
        granted_permissions_json: value.granted_permissions_json.clone(),
        permission_ceiling_json: value.granted_permissions_json.clone(),
        grant_state: value.grant_state.clone(),
        granted_at: value.granted_at,
    }
}

fn permission_subset(requested: &str, granted: &str) -> bool {
    let requested = serde_json::from_str::<serde_json::Value>(requested).ok();
    let granted = serde_json::from_str::<serde_json::Value>(granted).ok();
    requested
        .zip(granted)
        .is_some_and(|(requested, granted)| json_subset(&requested, &granted))
}

fn json_subset(requested: &serde_json::Value, granted: &serde_json::Value) -> bool {
    match (requested, granted) {
        (serde_json::Value::Object(left), serde_json::Value::Object(right)) => left
            .iter()
            .all(|(key, value)| right.get(key).is_some_and(|item| json_subset(value, item))),
        (serde_json::Value::Array(left), serde_json::Value::Array(right)) => {
            left.iter().all(|value| right.contains(value))
        }
        _ => requested == granted,
    }
}

pub(crate) async fn replace_in_transaction(
    transaction: &DatabaseTransaction,
    plugin_slug: &str,
    input: PluginRuntimeStateInput,
) -> Result<(), DbError> {
    plugin_activation_policy::Entity::delete_many()
        .filter(plugin_activation_policy::Column::PluginSlug.eq(plugin_slug))
        .exec(transaction)
        .await?;
    plugin_permission_grant::Entity::delete_many()
        .filter(plugin_permission_grant::Column::PluginSlug.eq(plugin_slug))
        .exec(transaction)
        .await?;
    let now = Utc::now();
    for value in input.activations {
        plugin_activation_policy::ActiveModel {
            id: NotSet,
            plugin_slug: Set(plugin_slug.to_string()),
            component_key: Set(value.component_key),
            scope: Set(value.scope),
            workspace_key: Set(value.workspace_key),
            agent_type: Set(value.agent_type),
            requested_enabled: Set(value.requested_enabled),
            routing_mode: Set(value.routing_mode),
            policy_source: Set(value.policy_source),
            updated_at: Set(now),
        }
        .insert(transaction)
        .await?;
    }
    for value in input.permission_grants {
        plugin_permission_grant::ActiveModel {
            id: NotSet,
            plugin_slug: Set(plugin_slug.to_string()),
            scope: Set(value.scope),
            workspace_key: Set(value.workspace_key),
            permissions_digest: Set(value.permissions_digest),
            granted_permissions_json: Set(value.granted_permissions_json),
            grant_state: Set(value.grant_state),
            granted_at: Set(value.granted_at),
            updated_at: Set(now),
        }
        .insert(transaction)
        .await?;
    }
    Ok(())
}

pub async fn list_activations(
    conn: &sea_orm::DatabaseConnection,
) -> Result<Vec<plugin_activation_policy::Model>, DbError> {
    plugin_activation_policy::Entity::find()
        .all(conn)
        .await
        .map_err(Into::into)
}

pub async fn list_permission_grants(
    conn: &sea_orm::DatabaseConnection,
) -> Result<Vec<plugin_permission_grant::Model>, DbError> {
    plugin_permission_grant::Entity::find()
        .all(conn)
        .await
        .map_err(Into::into)
}
