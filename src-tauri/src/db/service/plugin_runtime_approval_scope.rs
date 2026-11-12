use super::*;

pub(super) async fn current_grant(
    transaction: &sea_orm::DatabaseTransaction,
    scope: &PluginApprovalScope<'_>,
    digest: &str,
) -> Result<bool, DbError> {
    let grant = plugin_permission_grant::Entity::find()
        .filter(plugin_permission_grant::Column::PluginSlug.eq(scope.plugin_slug))
        .filter(plugin_permission_grant::Column::Scope.eq("workspace"))
        .filter(plugin_permission_grant::Column::WorkspaceKey.eq(scope.workspace_key))
        .one(transaction)
        .await?;
    Ok(grant
        .is_some_and(|grant| grant.grant_state == "granted" && grant.permissions_digest == digest))
}

pub(super) async fn apply(
    transaction: &sea_orm::DatabaseTransaction,
    scope: &PluginApprovalScope<'_>,
    input: (&[String], chrono::DateTime<Utc>, bool, bool),
) -> Result<(), DbError> {
    let (connectors, now, enabled, reset) = input;
    if reset {
        disable_workspace(transaction, scope).await?;
    }
    for connector in connectors {
        if reset {
            upsert_activation(
                transaction,
                ActivationApproval {
                    plugin_slug: scope.plugin_slug,
                    connector_key: connector,
                    workspace_key: scope.workspace_key,
                    agent_type: "",
                    now,
                    enabled: false,
                },
            )
            .await?;
        }
        upsert_activation(
            transaction,
            ActivationApproval {
                plugin_slug: scope.plugin_slug,
                connector_key: connector,
                workspace_key: scope.workspace_key,
                agent_type: scope.agent_type,
                now,
                enabled,
            },
        )
        .await?;
    }
    Ok(())
}
