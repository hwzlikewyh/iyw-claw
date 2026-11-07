use super::authentication::AuthOperation;
use super::router::PluginRouter;
use super::types::{PluginInvokeError, PluginToolCall};

const DEPENDENCY_ACCOUNT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

impl PluginRouter {
    pub(super) async fn verify_dependency_accounts(
        &self,
        call: &PluginToolCall,
    ) -> Result<(), PluginInvokeError> {
        tokio::time::timeout(
            DEPENDENCY_ACCOUNT_TIMEOUT,
            self.check_dependency_accounts(call),
        )
        .await
        .map_err(|_| {
            PluginInvokeError::before_effect(
                "plugin_dependency_authentication_required",
                "依赖插件账号检查超时，请先在插件准备中确认依赖账号",
            )
        })?
    }

    async fn check_dependency_accounts(
        &self,
        call: &PluginToolCall,
    ) -> Result<(), PluginInvokeError> {
        let snapshot = self.registry.snapshot();
        let plugin = snapshot
            .plugins
            .get(&call.context.plugin_slug)
            .ok_or_else(|| unavailable("插件不可用"))?;
        let report = super::dependencies::inspect(plugin, &snapshot.plugins);
        let ids = report
            .dependencies
            .iter()
            .map(|dependency| dependency.id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        for target in snapshot
            .plugins
            .values()
            .filter(|plugin| ids.contains(plugin.market_skill_id.to_string().as_str()))
        {
            let mut context = call.context.clone();
            context.plugin_slug = target.slug.clone();
            context.permission_revision = target.permissions_digest.clone();
            let dependency_call = PluginToolCall {
                context,
                arguments: serde_json::Map::new(),
            };
            for connector in target
                .manifest
                .components
                .iter()
                .filter(|component| component.kind == "connector")
            {
                let preparation = super::setup_store::contract(target, &connector.key)
                    .map_err(|_| unavailable("依赖插件准备声明无效"))?;
                if preparation.authentication.is_none() {
                    continue;
                }
                let status = self
                    .authentication(&dependency_call, (&connector.key, AuthOperation::Status))
                    .await?;
                if status.authenticated != Some(true) {
                    return Err(unavailable(&format!(
                        "依赖插件 {} 的业务账号需要登录，请先完成其账号准备",
                        target.slug
                    )));
                }
            }
        }
        Ok(())
    }
}

fn unavailable(message: &str) -> PluginInvokeError {
    PluginInvokeError::before_effect("plugin_dependency_authentication_required", message)
}
