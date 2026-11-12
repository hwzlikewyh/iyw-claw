use super::supervisor::PluginRuntimeSupervisor;
use super::types::{PluginInvokeError, RuntimeLaunchSpec};

impl PluginRuntimeSupervisor {
    pub(crate) async fn check_connector(
        &self,
        spec: RuntimeLaunchSpec,
    ) -> Result<usize, PluginInvokeError> {
        if self
            .inner
            .quiescing
            .load(std::sync::atomic::Ordering::Acquire)
            || !super::policy::authority_current(&spec).await
        {
            return Err(unavailable());
        }
        self.retry_quarantined(&spec.key).await;
        let instance = self.ensure_instance(&spec).await?;
        instance.client.recheck(&spec).await?;
        super::authentication::require(&instance.client, &spec, "").await?;
        if instance.client.is_closed().await || !super::policy::authority_current(&spec).await {
            return Err(unavailable());
        }
        Ok(spec.expected_tools.len())
    }

    pub(crate) async fn stop_workspace(&self, plugin: &str, workspace: &str) {
        let slots = {
            let mut slots = self.inner.slots.lock().await;
            let keys = slots
                .keys()
                .filter(|key| key.plugin_slug == plugin && key.workspace_key == workspace)
                .cloned()
                .collect::<Vec<_>>();
            keys.into_iter()
                .filter_map(|key| slots.remove(&key))
                .collect::<Vec<_>>()
        };
        for slot in slots {
            let instance = slot.lock().await.instance.take();
            if let Some(instance) = instance {
                instance.client.shutdown().await;
            }
        }
    }
}

fn unavailable() -> PluginInvokeError {
    PluginInvokeError::before_effect(
        "plugin_authority_revoked",
        "Plugin authorization changed during connection preparation",
    )
}
