//! Preserve routing helpers referenced by the upstream production library.

use crate::AuthManager;
use crate::AuthRouteConfig;
use crate::CodexAuth;
use codex_http_client::HttpClientFactory;
use codex_http_client::OutboundProxyPolicy;
use std::sync::Arc;

pub fn auth_manager_from_optional_auth(auth: Option<CodexAuth>) -> Arc<AuthManager> {
    AuthManager::from_optional_auth_for_testing(auth)
}

pub fn transport_default_auth_route_config() -> AuthRouteConfig {
    AuthRouteConfig::from_http_client_factory(HttpClientFactory::new(
        OutboundProxyPolicy::ReqwestDefault,
    ))
}
