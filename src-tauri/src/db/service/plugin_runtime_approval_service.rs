use chrono::Utc;
use sea_orm::sea_query::{Expr, OnConflict};
use sea_orm::{
    ActiveValue::NotSet, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
    TransactionTrait,
};

use crate::commands::skill_market::SkillPluginManifest;
use crate::db::entities::{plugin_activation_policy, plugin_installation, plugin_permission_grant};
use crate::db::error::DbError;

#[path = "plugin_runtime_approval_scope.rs"]
mod scoped_activation;

pub struct PluginApprovalScope<'a> {
    pub plugin_slug: &'a str,
    pub workspace_key: &'a str,
    pub agent_type: &'a str,
}

pub async fn approve_plugin(
    conn: &DatabaseConnection,
    scope: PluginApprovalScope<'_>,
) -> Result<(), DbError> {
    change_scope(
        conn,
        ScopeChange {
            scope,
            expected: None,
            enabled: true,
        },
    )
    .await
}

pub async fn set_plugin_scope(
    conn: &DatabaseConnection,
    scope: PluginApprovalScope<'_>,
    change: (&str, &str, bool),
) -> Result<(), DbError> {
    change_scope(
        conn,
        ScopeChange {
            scope,
            expected: Some((change.0, change.1)),
            enabled: change.2,
        },
    )
    .await
}

struct ScopeChange<'a> {
    scope: PluginApprovalScope<'a>,
    expected: Option<(&'a str, &'a str)>,
    enabled: bool,
}

async fn change_scope(conn: &DatabaseConnection, change: ScopeChange<'_>) -> Result<(), DbError> {
    let scope = change.scope;
    validate_scope(&scope)?;
    let transaction = conn.begin().await?;
    let installation = load_installation(&transaction, scope.plugin_slug).await?;
    if change.expected.is_some_and(|(version, digest)| {
        version != installation.version || digest != installation.permissions_digest
    }) {
        return Err(DbError::Validation(
            "插件版本或权限已变化，请重新核对后确认".into(),
        ));
    }
    let manifest = parse_manifest(&installation.manifest_json)?;
    let connector_keys = host_gateway_connectors(&manifest)?;
    let permissions_json = serde_json::to_string(&manifest.permissions.unwrap_or_default())
        .map_err(|error| DbError::Validation(error.to_string()))?;
    let now = Utc::now();
    let reset = !change.enabled
        || !scoped_activation::current_grant(
            &transaction,
            &scope,
            &installation.permissions_digest,
        )
        .await?;
    scoped_activation::apply(
        &transaction,
        &scope,
        (&connector_keys, now, change.enabled, reset),
    )
    .await?;
    upsert_permission(
        &transaction,
        PermissionApproval {
            plugin_slug: scope.plugin_slug,
            workspace_key: scope.workspace_key,
            permissions_digest: &installation.permissions_digest,
            permissions_json,
            now,
            enabled: change.enabled,
        },
    )
    .await?;
    transaction.commit().await?;
    Ok(())
}

async fn disable_workspace(
    transaction: &sea_orm::DatabaseTransaction,
    scope: &PluginApprovalScope<'_>,
) -> Result<(), DbError> {
    plugin_activation_policy::Entity::update_many()
        .col_expr(
            plugin_activation_policy::Column::RequestedEnabled,
            Expr::value(false),
        )
        .col_expr(
            plugin_activation_policy::Column::PolicySource,
            Expr::value("user_revoked"),
        )
        .filter(plugin_activation_policy::Column::PluginSlug.eq(scope.plugin_slug))
        .filter(plugin_activation_policy::Column::Scope.eq("workspace"))
        .filter(plugin_activation_policy::Column::WorkspaceKey.eq(scope.workspace_key))
        .exec(transaction)
        .await?;
    Ok(())
}

fn validate_scope(scope: &PluginApprovalScope<'_>) -> Result<(), DbError> {
    if scope.plugin_slug.trim().is_empty()
        || scope.workspace_key.trim().is_empty()
        || scope.agent_type.trim().is_empty()
    {
        return Err(DbError::Validation(
            "plugin approval requires plugin, workspace, and agent".to_string(),
        ));
    }
    Ok(())
}

async fn load_installation(
    transaction: &sea_orm::DatabaseTransaction,
    plugin_slug: &str,
) -> Result<plugin_installation::Model, DbError> {
    plugin_installation::Entity::find()
        .filter(plugin_installation::Column::Slug.eq(plugin_slug))
        .one(transaction)
        .await?
        .ok_or_else(|| DbError::NotFound("plugin installation".to_string()))
}

fn parse_manifest(value: &str) -> Result<SkillPluginManifest, DbError> {
    serde_json::from_str(value).map_err(|error| DbError::Validation(error.to_string()))
}

fn host_gateway_connectors(manifest: &SkillPluginManifest) -> Result<Vec<String>, DbError> {
    let keys = manifest
        .components
        .iter()
        .filter(|component| component.kind == "connector")
        .filter(|component| {
            component
                .config
                .as_ref()
                .and_then(|config| config["routing"]["mode"].as_str())
                == Some("host_gateway")
        })
        .map(|component| component.key.clone())
        .collect::<Vec<_>>();
    if keys.is_empty() {
        return Err(DbError::Validation(
            "plugin has no HostGateway connector".to_string(),
        ));
    }
    Ok(keys)
}

async fn upsert_activation(
    transaction: &sea_orm::DatabaseTransaction,
    approval: ActivationApproval<'_>,
) -> Result<(), DbError> {
    plugin_activation_policy::Entity::insert(plugin_activation_policy::ActiveModel {
        id: NotSet,
        plugin_slug: Set(approval.plugin_slug.to_string()),
        component_key: Set(approval.connector_key.to_string()),
        scope: Set("workspace".to_string()),
        workspace_key: Set(approval.workspace_key.to_string()),
        agent_type: Set(approval.agent_type.to_string()),
        requested_enabled: Set(approval.enabled),
        routing_mode: Set("host_gateway".to_string()),
        policy_source: Set(if approval.enabled {
            "user_approval"
        } else {
            "user_revoked"
        }
        .into()),
        updated_at: Set(approval.now),
    })
    .on_conflict(
        OnConflict::columns([
            plugin_activation_policy::Column::PluginSlug,
            plugin_activation_policy::Column::ComponentKey,
            plugin_activation_policy::Column::Scope,
            plugin_activation_policy::Column::WorkspaceKey,
            plugin_activation_policy::Column::AgentType,
        ])
        .update_columns([
            plugin_activation_policy::Column::RequestedEnabled,
            plugin_activation_policy::Column::RoutingMode,
            plugin_activation_policy::Column::PolicySource,
            plugin_activation_policy::Column::UpdatedAt,
        ])
        .to_owned(),
    )
    .exec(transaction)
    .await?;
    Ok(())
}

async fn upsert_permission(
    transaction: &sea_orm::DatabaseTransaction,
    approval: PermissionApproval<'_>,
) -> Result<(), DbError> {
    plugin_permission_grant::Entity::insert(plugin_permission_grant::ActiveModel {
        id: NotSet,
        plugin_slug: Set(approval.plugin_slug.to_string()),
        scope: Set("workspace".to_string()),
        workspace_key: Set(approval.workspace_key.to_string()),
        permissions_digest: Set(approval.permissions_digest.to_string()),
        granted_permissions_json: Set(approval.permissions_json),
        grant_state: Set(if approval.enabled {
            "granted"
        } else {
            "revoked"
        }
        .into()),
        granted_at: Set(approval.enabled.then_some(approval.now)),
        updated_at: Set(approval.now),
    })
    .on_conflict(
        OnConflict::columns([
            plugin_permission_grant::Column::PluginSlug,
            plugin_permission_grant::Column::Scope,
            plugin_permission_grant::Column::WorkspaceKey,
        ])
        .update_columns([
            plugin_permission_grant::Column::PermissionsDigest,
            plugin_permission_grant::Column::GrantedPermissionsJson,
            plugin_permission_grant::Column::GrantState,
            plugin_permission_grant::Column::GrantedAt,
            plugin_permission_grant::Column::UpdatedAt,
        ])
        .to_owned(),
    )
    .exec(transaction)
    .await?;
    Ok(())
}

struct ActivationApproval<'a> {
    enabled: bool,
    plugin_slug: &'a str,
    connector_key: &'a str,
    workspace_key: &'a str,
    agent_type: &'a str,
    now: chrono::DateTime<Utc>,
}

struct PermissionApproval<'a> {
    enabled: bool,
    plugin_slug: &'a str,
    workspace_key: &'a str,
    permissions_digest: &'a str,
    permissions_json: String,
    now: chrono::DateTime<Utc>,
}
