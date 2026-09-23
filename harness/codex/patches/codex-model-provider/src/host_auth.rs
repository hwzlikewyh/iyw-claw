use std::sync::Arc;

use codex_api::{AuthProvider, SharedAuthProvider};
use codex_login::CodexAuth;
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
    if provider.name != "iyw-claw" || provider.env_key.as_deref() != Some("CODEX_API_KEY") {
        return None;
    }
    auth?.api_key()
}

struct HostGatewayAuth {
    bearer: BearerAuthProvider,
    token: String,
}

impl AuthProvider for HostGatewayAuth {
    fn add_auth_headers(&self, headers: &mut HeaderMap) {
        self.bearer.add_auth_headers(headers);
        if let Ok(mut token) = HeaderValue::from_str(&self.token) {
            token.set_sensitive(true);
            headers.insert("token", token);
        }
    }
}
