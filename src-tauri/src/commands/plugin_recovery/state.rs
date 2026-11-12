use crate::app_error::AppCommandError;
use crate::db::entities::plugin_installation;
use crate::db::service::{
    plugin_installation_service::PluginInstallationRecord,
    plugin_runtime_state_service::{
        self, PluginActivationInput, PluginPermissionGrantInput, PluginRuntimeStateInput,
    },
};
use sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, Set, TransactionTrait};

pub(super) async fn apply(
    conn: &sea_orm::DatabaseConnection,
    record: &PluginInstallationRecord,
    reset: bool,
) -> Result<(), AppCommandError> {
    let transaction = conn.begin().await.map_err(db_error)?;
    let current = plugin_installation::Entity::find_by_id(record.installation.id)
        .one(&transaction)
        .await
        .map_err(db_error)?
        .ok_or_else(|| super::invalid("插件在恢复期间被卸载"))?;
    if current != record.installation {
        return Err(super::invalid("插件安装记录在恢复期间被修改，请重新检查"));
    }
    let mut active = current.into_active_model();
    active.status = Set("installed".into());
    active.reconcile_state = Set("recovery_pending".into());
    active.updated_at = Set(chrono::Utc::now());
    active.update(&transaction).await.map_err(db_error)?;
    if reset {
        plugin_runtime_state_service::replace_in_transaction(
            &transaction,
            &record.installation.slug,
            runtime_state(record, true),
        )
        .await
        .map_err(AppCommandError::from)?;
    }
    transaction.commit().await.map_err(db_error)
}

pub(super) async fn finish(
    conn: &sea_orm::DatabaseConnection,
    record: &PluginInstallationRecord,
) -> Result<(), AppCommandError> {
    let current = plugin_installation::Entity::find_by_id(record.installation.id)
        .one(conn)
        .await
        .map_err(db_error)?
        .ok_or_else(|| super::invalid("插件恢复记录缺失"))?;
    if current.version != record.installation.version
        || current.reconcile_state != "recovery_pending"
    {
        return Err(super::invalid("插件恢复状态发生变化，请重新检查"));
    }
    let mut active = current.into_active_model();
    active.reconcile_state = Set("ready".into());
    active.updated_at = Set(chrono::Utc::now());
    active.update(conn).await.map_err(db_error)?;
    Ok(())
}

pub(super) async fn suspend(
    conn: &sea_orm::DatabaseConnection,
    record: &PluginInstallationRecord,
) -> Result<(), AppCommandError> {
    let current = plugin_installation::Entity::find_by_id(record.installation.id)
        .one(conn)
        .await
        .map_err(db_error)?
        .ok_or_else(|| super::invalid("插件恢复记录缺失"))?;
    if current.version != record.installation.version {
        return Err(super::invalid("插件恢复版本已变化"));
    }
    let mut active = current.into_active_model();
    active.reconcile_state = Set("recovery_pending".into());
    active.update(conn).await.map_err(db_error)?;
    Ok(())
}

pub(super) async fn restore(
    conn: &sea_orm::DatabaseConnection,
    record: &PluginInstallationRecord,
) -> Result<(), AppCommandError> {
    let transaction = conn.begin().await.map_err(db_error)?;
    let mut active = record.installation.clone().into_active_model();
    active.status = Set(record.installation.status.clone());
    active.reconcile_state = Set(record.installation.reconcile_state.clone());
    active.updated_at = Set(record.installation.updated_at);
    active.update(&transaction).await.map_err(db_error)?;
    plugin_runtime_state_service::replace_in_transaction(
        &transaction,
        &record.installation.slug,
        runtime_state(record, false),
    )
    .await
    .map_err(AppCommandError::from)?;
    transaction.commit().await.map_err(db_error)
}

fn db_error(error: sea_orm::DbErr) -> AppCommandError {
    AppCommandError::db(error.into())
}

fn runtime_state(record: &PluginInstallationRecord, reset: bool) -> PluginRuntimeStateInput {
    PluginRuntimeStateInput {
        activations: record
            .activations
            .iter()
            .map(|value| PluginActivationInput {
                component_key: value.component_key.clone(),
                scope: value.scope.clone(),
                workspace_key: value.workspace_key.clone(),
                agent_type: value.agent_type.clone(),
                requested_enabled: !reset && value.requested_enabled,
                routing_mode: value.routing_mode.clone(),
                policy_source: if reset {
                    "recovery_pending".into()
                } else {
                    value.policy_source.clone()
                },
            })
            .collect(),
        permission_grants: grants(record, reset),
    }
}

fn grants(record: &PluginInstallationRecord, reset: bool) -> Vec<PluginPermissionGrantInput> {
    record
        .permission_grants
        .iter()
        .map(|value| PluginPermissionGrantInput {
            scope: value.scope.clone(),
            workspace_key: value.workspace_key.clone(),
            permissions_digest: value.permissions_digest.clone(),
            granted_permissions_json: if reset {
                "{}".into()
            } else {
                value.granted_permissions_json.clone()
            },
            permission_ceiling_json: value.granted_permissions_json.clone(),
            grant_state: if reset {
                "pending".into()
            } else {
                value.grant_state.clone()
            },
            granted_at: if reset { None } else { value.granted_at },
        })
        .collect()
}
