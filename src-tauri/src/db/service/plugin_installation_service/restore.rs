use super::*;

pub(crate) async fn finish_installations(
    conn: &DatabaseConnection,
    versions: &[(i64, String)],
) -> Result<(), DbError> {
    let transaction = conn.begin().await?;
    for (id, version) in versions {
        let result = plugin_installation::Entity::update_many()
            .col_expr(
                plugin_installation::Column::ReconcileState,
                sea_orm::sea_query::Expr::value("ready"),
            )
            .filter(plugin_installation::Column::MarketSkillId.eq(*id))
            .filter(plugin_installation::Column::Version.eq(version.clone()))
            .filter(plugin_installation::Column::ReconcileState.eq("installing"))
            .exec(&transaction)
            .await?;
        if result.rows_affected != 1 {
            return Err(DbError::Validation(
                "Plugin installation changed before commit".into(),
            ));
        }
    }
    transaction.commit().await?;
    Ok(())
}

/// 回退使用原快照，不能将失败版本的启用或授权再次合并进旧版本。
pub(crate) async fn restore(
    conn: &DatabaseConnection,
    input: PluginInstallationInput,
) -> Result<PluginInstallationRecord, DbError> {
    let transaction = conn.begin().await?;
    let result = replace_in_transaction(&transaction, input, false).await?;
    transaction.commit().await?;
    Ok(result)
}

pub async fn mark_repair_required(
    conn: &DatabaseConnection,
    market_skill_id: i64,
) -> Result<(), DbError> {
    plugin_installation::Entity::update_many()
        .col_expr(
            plugin_installation::Column::Status,
            sea_orm::sea_query::Expr::value("repair_required"),
        )
        .col_expr(
            plugin_installation::Column::ReconcileState,
            sea_orm::sea_query::Expr::value("repair_required"),
        )
        .filter(plugin_installation::Column::MarketSkillId.eq(market_skill_id))
        .exec(conn)
        .await?;
    Ok(())
}
