use super::router::PluginRouter;
use super::types::{PluginInvokeError, PluginToolCall};

impl PluginRouter {
    pub(crate) async fn authentication(
        &self,
        call: &PluginToolCall,
        context: (&str, super::authentication::AuthOperation),
    ) -> Result<super::authentication::AccountStatus, PluginInvokeError> {
        let snapshot = self.registry.snapshot();
        let plugin = snapshot
            .plugins
            .get(&call.context.plugin_slug)
            .filter(|plugin| plugin.available)
            .ok_or_else(|| PluginInvokeError::before_effect("plugin_unavailable", "插件不可用"))?;
        super::router::validate_permission(plugin, call)?;
        super::router::validate_activation(
            plugin,
            context.0,
            &call.context.workspace_key,
            call.context.agent_type,
        )?;
        let spec = super::runtime_spec::launch_spec(plugin, context.0, call)?;
        self.supervisor.authentication(spec, context.1).await
    }
    pub(crate) async fn check_connector(
        &self,
        call: &PluginToolCall,
        connector: &str,
    ) -> Result<usize, PluginInvokeError> {
        self.verify_dependency_accounts(call).await?;
        let snapshot = self.registry.snapshot();
        let plugin = snapshot
            .plugins
            .get(&call.context.plugin_slug)
            .filter(|plugin| plugin.available)
            .ok_or_else(|| {
                PluginInvokeError::before_effect("plugin_unavailable", "Plugin is unavailable")
            })?;
        super::router::validate_permission(plugin, call)?;
        super::router::validate_activation(
            plugin,
            connector,
            &call.context.workspace_key,
            call.context.agent_type,
        )?;
        super::dependency_scope::require(
            plugin,
            &snapshot.plugins,
            (
                &call.context.workspace_key,
                call.context.agent_type.as_wire().as_ref(),
            ),
        )?;
        let spec = super::runtime_spec::launch_spec(plugin, connector, call)?;
        self.supervisor.check_connector(spec).await
    }
}
