//! 模型连接诊断只输出目标 origin、代理策略和错误类别，不输出凭据及请求内容。
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use codex_api::{Provider, RequestTelemetry, TransportError};
use codex_http_client::{HttpClientFactory, OutboundProxyPolicy};
use http::StatusCode;
use url::Url;

const MAX_ERROR_CHAIN_DEPTH: usize = 16;

pub(super) struct ConnectionDiagnostics {
    scheme: String,
    host: String,
    port: Option<u16>,
    proxy_policy: &'static str,
    managed_network: bool,
    proxy_env_present: bool,
    no_proxy_env_present: bool,
}

impl ConnectionDiagnostics {
    pub(super) fn new(provider: &Provider, factory: &HttpClientFactory) -> Self {
        let url = Url::parse(&provider.url_for_path("/responses")).ok();
        let proxy_policy = match factory.outbound_proxy_policy() {
            OutboundProxyPolicy::ReqwestDefault => "transport_default",
            OutboundProxyPolicy::RespectSystemProxy => "respect_system_proxy",
        };
        let result = Self {
            scheme: url
                .as_ref()
                .map(|url| url.scheme().to_owned())
                .unwrap_or_default(),
            host: url
                .as_ref()
                .and_then(|url| url.host_str())
                .unwrap_or_default()
                .to_owned(),
            port: url.as_ref().and_then(Url::port_or_known_default),
            proxy_policy,
            managed_network: factory.network_policy().is_managed(),
            proxy_env_present: env_present(&[
                "HTTP_PROXY",
                "HTTPS_PROXY",
                "ALL_PROXY",
                "http_proxy",
                "https_proxy",
                "all_proxy",
            ]),
            no_proxy_env_present: env_present(&["NO_PROXY", "no_proxy"]),
        };
        tracing::info!(target: "xinghe_model_connection",
            phase = "route_prepared", request_scheme = %result.scheme,
            request_host = %result.host, request_port = result.port,
            endpoint = "/responses", proxy_policy = result.proxy_policy,
            managed_network = result.managed_network,
            proxy_env_present = result.proxy_env_present,
            no_proxy_env_present = result.no_proxy_env_present,
            proxy_route_observed = false,
            "model connection target prepared");
        result
    }

    pub(super) fn wrap(self, inner: Arc<dyn RequestTelemetry>) -> Arc<dyn RequestTelemetry> {
        Arc::new(ConnectionTelemetry {
            inner,
            diagnostics: self,
        })
    }

    fn failed(&self, error: &TransportError, attempt: (u64, Duration, Option<StatusCode>)) {
        let details = ErrorDetails::new(error);
        tracing::warn!(target: "xinghe_model_connection",
            phase = "request_failed", endpoint = "/responses",
            request_scheme = %self.scheme, request_host = %self.host,
            request_port = self.port, proxy_policy = self.proxy_policy,
            managed_network = self.managed_network,
            proxy_env_present = self.proxy_env_present,
            no_proxy_env_present = self.no_proxy_env_present,
            proxy_route_observed = false, attempt = attempt.0,
            elapsed_ms = attempt.1.as_millis() as u64,
            status = attempt.2.map(|status| status.as_u16()),
            transport_error = transport_category(error),
            cause_classes = ?details.classes, os_error_codes = ?details.os_codes,
            io_error_kinds = ?details.io_kinds, cause_chain_truncated = details.truncated,
            "model connection request failed");
    }
}

struct ConnectionTelemetry {
    inner: Arc<dyn RequestTelemetry>,
    diagnostics: ConnectionDiagnostics,
}

impl RequestTelemetry for ConnectionTelemetry {
    fn on_request(
        &self,
        attempt: u64,
        status: Option<StatusCode>,
        error: Option<&TransportError>,
        duration: Duration,
    ) {
        if let Some(error) = error {
            self.diagnostics.failed(error, (attempt, duration, status));
        }
        self.inner.on_request(attempt, status, error, duration);
    }
}

fn env_present(names: &[&str]) -> bool {
    names
        .iter()
        .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
}

fn transport_category(error: &TransportError) -> &'static str {
    match error {
        TransportError::Connection(_) => "connection",
        TransportError::Timeout => "timeout",
        TransportError::Network(_) => "network",
        TransportError::Policy(_) => "policy",
        TransportError::Build(_) => "build",
        TransportError::Http { .. } => "http_response",
        _ => "other",
    }
}

#[derive(Default)]
struct ErrorDetails {
    classes: Vec<&'static str>,
    os_codes: Vec<i32>,
    io_kinds: Vec<String>,
    truncated: bool,
}

impl ErrorDetails {
    fn new(error: &(dyn Error + 'static)) -> Self {
        let mut result = Self::default();
        // 跳过可能携带 HTTP 响应正文的顶层 Display，只检查 source 链。
        let mut source = error.source();
        for _ in 0..MAX_ERROR_CHAIN_DEPTH {
            let Some(cause) = source else { break };
            result.classes.push(cause_class(cause));
            if let Some(error) = cause.downcast_ref::<std::io::Error>() {
                result.io_kinds.push(format!("{:?}", error.kind()));
                if let Some(code) = error.raw_os_error() {
                    result.os_codes.push(code);
                }
            }
            source = cause.source();
        }
        result.truncated = source.is_some();
        result
    }
}

fn cause_class(error: &(dyn Error + 'static)) -> &'static str {
    if let Some(error) = error.downcast_ref::<codex_http_client::HttpError>() {
        if error.is_timeout() {
            return "timeout";
        }
        if error
            .failure_class()
            .is_some_and(|class| class.to_string() == "tls_error")
        {
            return "tls";
        }
    }
    // 仅匹配白名单类别；原始字符串可能含 URL、证书身份或凭据，不进入日志。
    let message = error.to_string().to_ascii_lowercase();
    let patterns = [
        ("certificate", "certificate"),
        ("tls", "tls"),
        ("ssl", "tls"),
        ("dns", "dns"),
        ("resolve", "dns"),
        ("timed out", "timeout"),
        ("timeout", "timeout"),
        ("refused", "connection_refused"),
        ("reset", "connection_reset"),
        ("proxy", "proxy"),
        ("tunnel", "proxy_tunnel"),
        ("permission", "permission_denied"),
        ("access denied", "permission_denied"),
        ("unreachable", "unreachable"),
        ("connect", "connect"),
    ];
    patterns
        .into_iter()
        .find_map(|(needle, class)| message.contains(needle).then_some(class))
        .unwrap_or("unclassified")
}
