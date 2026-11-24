//! 本机模型网关的边界日志；只记录 origin 和错误分类，禁止记录随机路径及凭据。
use std::error::Error;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use axum::http::Method;

const MAX_ERROR_CHAIN_DEPTH: usize = 16;
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

pub(super) fn request_span(runtime: &super::Runtime, method: &Method) -> tracing::Span {
    let method = match *method {
        Method::GET => "GET",
        Method::POST => "POST",
        _ => "other",
    };
    tracing::info_span!(target: "model_gateway", "model_gateway_request",
        gateway_request_id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed),
        listen_address = %runtime.authority, method)
}

pub(super) fn ready(runtime: &super::Runtime) {
    let upstream = reqwest::Url::parse(&runtime.upstream).ok();
    tracing::info!(target: "model_gateway", phase = "ready", proxy_mode = "direct",
        listen_address = %runtime.authority,
        upstream_scheme = upstream.as_ref().map(|url| url.scheme()),
        upstream_host = upstream.as_ref().and_then(|url| url.host_str()),
        upstream_port = upstream.as_ref().and_then(|url| url.port_or_known_default()),
        "process local model gateway ready");
}

pub(super) struct CloudAttempt {
    scheme: String,
    host: String,
    port: Option<u16>,
    started: Instant,
}

impl CloudAttempt {
    pub(super) fn new(url: &reqwest::Url) -> Self {
        Self {
            scheme: url.scheme().to_owned(),
            host: url.host_str().unwrap_or_default().to_owned(),
            port: url.port_or_known_default(),
            started: Instant::now(),
        }
    }

    pub(super) fn failed(&self, error: &reqwest::Error) {
        let mut source: Option<&(dyn Error + 'static)> = Some(error);
        let mut os_codes = Vec::new();
        let mut io_kinds = Vec::new();
        let mut classes = Vec::new();
        for _ in 0..MAX_ERROR_CHAIN_DEPTH {
            let Some(cause) = source else { break };
            classes.push(cause_class(cause));
            if let Some(error) = cause.downcast_ref::<std::io::Error>() {
                io_kinds.push(format!("{:?}", error.kind()));
                if let Some(code) = error.raw_os_error() {
                    os_codes.push(code);
                }
            }
            source = cause.source();
        }
        tracing::warn!(target: "model_gateway", phase = "cloud_send_failed",
            upstream_scheme = %self.scheme, upstream_host = %self.host,
            upstream_port = self.port, proxy_mode = "direct",
            elapsed_ms = self.started.elapsed().as_millis() as u64,
            is_connect = error.is_connect(), is_timeout = error.is_timeout(),
            status = error.status().map(|status| status.as_u16()),
            os_error_codes = ?os_codes, io_error_kinds = ?io_kinds, cause_classes = ?classes,
            cause_chain_truncated = source.is_some(),
            "cloud model request transport failed");
    }

    pub(super) fn timed_out(&self) {
        tracing::warn!(target: "model_gateway", phase = "cloud_headers_timeout",
            upstream_scheme = %self.scheme, upstream_host = %self.host,
            upstream_port = self.port, proxy_mode = "direct",
            elapsed_ms = self.started.elapsed().as_millis() as u64,
            "cloud model response headers timed out");
    }

    pub(super) fn responded(&self, status: u16) {
        tracing::info!(target: "model_gateway", phase = "cloud_response",
            upstream_scheme = %self.scheme, upstream_host = %self.host,
            upstream_port = self.port, proxy_mode = "direct", status,
            elapsed_ms = self.started.elapsed().as_millis() as u64,
            "cloud model response headers received");
    }
}

fn cause_class(error: &(dyn Error + 'static)) -> &'static str {
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
        ("connect", "connect"),
    ];
    patterns
        .into_iter()
        .find_map(|(needle, class)| message.contains(needle).then_some(class))
        .unwrap_or("unclassified")
}
