use std::io;
use std::sync::Arc;

use codex_core::config::Config;
use codex_login::{
    AuthManager, CodexAuth, ExternalAuth, ExternalAuthFuture, ExternalAuthRefreshContext,
};

pub(super) async fn attach(
    manager: Arc<AuthManager>,
    host: (&Config, Option<String>),
) -> io::Result<Arc<AuthManager>> {
    let (config, api_key) = host;
    if let Some(key) = api_key {
        if key.trim().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "empty host API key",
            ));
        }
        // 上游策略认证继续使用已存储账号；宿主 key 只属于独立 serving 实例。
        let manager = AuthManager::shared_from_config(config, false)
            .await
            .map_err(io::Error::other)?;
        // 沿用上游策略校验与实例认证，不写认证文件或修改进程环境。
        manager
            .set_external_auth(Arc::new(HostApiKey(key)))
            .await
            .map_err(io::Error::other)?;
        return Ok(manager);
    }
    Ok(manager)
}

struct HostApiKey(String);

impl ExternalAuth for HostApiKey {
    fn resolve(&self) -> ExternalAuthFuture<'_, CodexAuth> {
        Box::pin(async { Ok(CodexAuth::from_api_key(&self.0)) })
    }

    fn refresh(&self, _context: ExternalAuthRefreshContext) -> ExternalAuthFuture<'_, CodexAuth> {
        Box::pin(async {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "host credentials expired; reconnect with refreshed credentials",
            ))
        })
    }
}
