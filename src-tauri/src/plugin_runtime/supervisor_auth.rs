use super::authentication::{AccountStatus, AuthOperation};
use super::supervisor::PluginRuntimeSupervisor;
use super::types::{PluginInvokeError, RuntimeLaunchSpec};

impl PluginRuntimeSupervisor {
    pub(crate) async fn authentication(
        &self,
        spec: RuntimeLaunchSpec,
        operation: AuthOperation,
    ) -> Result<AccountStatus, PluginInvokeError> {
        if !super::policy::authority_current(&spec).await {
            return Err(revoked());
        }
        let Some(auth) = super::authentication::declaration(&spec)? else {
            return Ok(AccountStatus {
                state: "not_required".into(),
                authenticated: None,
                authorization_url: None,
            });
        };
        let _operation = super::authentication::reserve(&spec)?;
        if self
            .inner
            .quiescing
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return Err(revoked());
        }
        self.retry_quarantined(&spec.key).await;
        let instance = self.ensure_instance(&spec).await?;
        let _permits = super::lease::acquire(&self.inner.global_calls, &instance).await?;
        if !super::policy::authority_current(&spec).await {
            return Err(revoked());
        }
        let result = super::authentication::execute(&instance.client, &auth, operation).await?;
        if !super::policy::authority_current(&spec).await {
            return Err(revoked());
        }
        Ok(result)
    }
}

fn revoked() -> PluginInvokeError {
    PluginInvokeError::before_effect(
        "plugin_authority_revoked",
        "插件、参数或项目授权已变化，请重新打开账号准备",
    )
}
