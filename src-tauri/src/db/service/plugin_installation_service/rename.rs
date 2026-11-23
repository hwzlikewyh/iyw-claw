use super::*;

pub(super) fn previous_slug(
    existing: Option<&plugin_installation::Model>,
    input: &PluginInstallationInput,
) -> Result<Option<String>, DbError> {
    let previous = existing.map(|value| value.slug.clone());
    if previous.as_ref().is_some_and(|slug| {
        !crate::models::plugin_names::same_plugin(input.market_skill_id, slug, &input.slug)
    }) {
        return Err(DbError::Validation(
            "Plugin name changed without an approved migration".into(),
        ));
    }
    Ok(previous)
}

pub(super) async fn replace_runtime_state(
    transaction: &DatabaseTransaction,
    installation: &plugin_installation::Model,
    context: (
        bool,
        Option<&str>,
        plugin_runtime_state_service::PluginRuntimeStateInput,
    ),
) -> Result<(), DbError> {
    let (preserve, previous, mut state) = context;
    if preserve {
        state = plugin_runtime_state_service::preserve_existing_in_transaction(
            transaction,
            previous.unwrap_or(&installation.slug),
            state,
        )
        .await?;
    }
    plugin_runtime_state_service::replace_in_transaction(transaction, &installation.slug, state)
        .await?;
    if let Some(previous) = previous.filter(|slug| *slug != installation.slug) {
        plugin_runtime_state_service::replace_in_transaction(
            transaction,
            previous,
            Default::default(),
        )
        .await?;
        tracing::info!(market_skill_id=installation.market_skill_id, old_slug=%previous, slug=%installation.slug,
            "[plugin-install] 已迁移插件授权与启用状态");
    }
    Ok(())
}
