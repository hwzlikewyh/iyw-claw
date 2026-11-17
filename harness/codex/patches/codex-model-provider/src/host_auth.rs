use std::sync::Arc;

use codex_api::{AuthProvider, SharedAuthProvider};
use codex_login::{AuthManager, CodexAuth};
use codex_model_provider_info::ModelProviderInfo;
use http::{HeaderMap, HeaderValue};

use crate::bearer_auth_provider::BearerAuthProvider;

pub(super) fn resolve(
    auth: Option<&CodexAuth>,
    provider: &ModelProviderInfo,
) -> Option<SharedAuthProvider> {
    let token = api_key(auth, provider)?.to_owned();
    Some(Arc::new(HostGatewayAuth {
        bearer: BearerAuthProvider::new(token.clone()),
        token,
    }))
}

pub fn api_key<'a>(auth: Option<&'a CodexAuth>, provider: &ModelProviderInfo) -> Option<&'a str> {
    if !is_managed_provider(provider) {
        return None;
    }
    auth?.api_key()
}

pub(crate) fn is_managed_provider(provider: &ModelProviderInfo) -> bool {
    provider.name == "iyw-claw" && provider.env_key.as_deref() == Some("CODEX_API_KEY")
}

pub(crate) async fn resolve_request(
    provider: &ModelProviderInfo,
    manager: Option<&Arc<AuthManager>>,
) -> codex_protocol::error::Result<Option<crate::ResolvedProviderAuth>> {
    let Some(manager) =
        manager.filter(|manager| is_managed_provider(provider) && manager.has_external_auth())
    else {
        return Ok(None);
    };
    // 新请求入口必须先使用实例凭据，不能落入进程环境变量读取。
    let credential = manager.auth().await;
    let auth = resolve(credential.as_ref(), provider).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "host credentials unavailable; reconnect with refreshed credentials",
        )
    })?;
    let mut resolved = crate::ResolvedProviderAuth::new(auth);
    resolved.credential = credential;
    Ok(Some(resolved))
}

struct HostGatewayAuth {
    bearer: BearerAuthProvider,
    token: String,
}

impl AuthProvider for HostGatewayAuth {
    fn add_auth_headers(&self, headers: &mut HeaderMap) -> Result<(), codex_api::AuthError> {
        self.bearer.add_auth_headers(headers)?;
        if let Ok(mut token) = HeaderValue::from_str(&self.token) {
            token.set_sensitive(true);
            headers.insert("token", token);
        }
        Ok(())
    }
}
