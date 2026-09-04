use std::time::Duration;

use serde_json::Value;

use super::error::BrowserError;
use super::opencli::OpencliProvider;

pub(super) const HANDOFF_TIMEOUT: Duration = Duration::from_secs(30);
pub(super) const HANDOFF_TTL_SECONDS: u64 = 120;
const MAX_COOKIE_COUNT: usize = 128;
const MAX_STORAGE_ITEMS: usize = 256;
const AUTH_CAPTURE_SCRIPT: &str = r#"(() => ({
  url: location.href,
  cookies: document.cookie,
  localStorage: Object.fromEntries(Object.entries(localStorage).slice(0, 256)),
  sessionStorage: Object.fromEntries(Object.entries(sessionStorage).slice(0, 256))
}))()"#;

#[derive(Debug, Clone)]
pub(super) struct CookieSeed {
    pub name: String,
    pub value: String,
    pub url: Option<String>,
    pub domain: Option<String>,
    pub path: Option<String>,
    pub http_only: bool,
    pub secure: bool,
    pub same_site: Option<String>,
    pub expires: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct StorageSeed {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Default)]
pub(super) struct HandoffReport {
    pub origin: Option<String>,
    pub cookies: Vec<CookieSeed>,
    pub cookies_seen: usize,
    pub local_storage: Vec<StorageSeed>,
    pub session_storage: Vec<StorageSeed>,
    pub cookies_imported: usize,
    pub storage_imported: usize,
    pub capture_errors: Vec<String>,
}

impl HandoffReport {
    pub fn discard_sensitive_seeds(&mut self) {
        self.cookies.clear();
        self.local_storage.clear();
        self.session_storage.clear();
    }
}

pub(super) async fn current_url(
    session: &str,
    target: Option<&str>,
    input: &Value,
) -> Result<String, BrowserError> {
    let result = OpencliProvider::invoke(
        session,
        "get",
        &["url".to_string()],
        target,
        HANDOFF_TIMEOUT,
    )
    .await;
    result
        .ok()
        .and_then(|result| extract_string(&result.output))
        .or_else(|| input.get("url").and_then(Value::as_str).map(str::to_string))
        .ok_or_else(|| {
            BrowserError::new(
                super::error::BrowserErrorCode::OpencliUserActionRequired,
                "OpenCLI requested user action but did not expose the current page URL",
            )
        })
}

pub(super) async fn capture_auth_state(
    session: &str,
    target: Option<&str>,
    url: &str,
) -> HandoffReport {
    let mut report = HandoffReport::default();
    let Ok(result) = OpencliProvider::invoke(
        session,
        "eval",
        &[AUTH_CAPTURE_SCRIPT.to_string()],
        target,
        HANDOFF_TIMEOUT,
    )
    .await
    else {
        report
            .capture_errors
            .push("cookies_unavailable".to_string());
        return report;
    };
    let captured = captured_object(&result.output);
    let cookie_header = captured
        .and_then(|value| value.get("cookies"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    report.cookies = cookie_seeds(cookie_header, url, MAX_COOKIE_COUNT);
    report.cookies_seen = report.cookies.len();
    report.local_storage = captured
        .and_then(|value| value.get("localStorage"))
        .map(|value| storage_seeds(value, MAX_STORAGE_ITEMS))
        .unwrap_or_default();
    report.session_storage = captured
        .and_then(|value| value.get("sessionStorage"))
        .map(|value| storage_seeds(value, MAX_STORAGE_ITEMS))
        .unwrap_or_default();
    report
}

pub(super) fn storage_seeds(value: &Value, limit: usize) -> Vec<StorageSeed> {
    let map = value
        .as_object()
        .and_then(|map| map.get("data").or_else(|| map.get("output")))
        .and_then(Value::as_object)
        .or_else(|| value.as_object());
    map.into_iter()
        .flat_map(|map| map.iter())
        .filter_map(|(key, value)| {
            value.as_str().map(|value| StorageSeed {
                key: key.clone(),
                value: value.to_string(),
            })
        })
        .take(limit)
        .collect()
}

fn cookie_seeds(header: &str, url: &str, limit: usize) -> Vec<CookieSeed> {
    header
        .split(';')
        .filter_map(|part| {
            let (name, value) = part.trim().split_once('=')?;
            (!name.is_empty()).then(|| CookieSeed {
                name: name.to_string(),
                value: value.to_string(),
                url: Some(url.to_string()),
                domain: None,
                path: None,
                http_only: false,
                secure: url.starts_with("https://"),
                same_site: None,
                expires: None,
            })
        })
        .take(limit)
        .collect()
}

pub(super) fn captured_object(value: &Value) -> Option<&serde_json::Map<String, Value>> {
    let direct = value.as_object();
    if direct.is_some_and(|map| {
        map.contains_key("cookies")
            || map.contains_key("localStorage")
            || map.contains_key("sessionStorage")
    }) {
        return direct;
    }
    value
        .get("value")
        .and_then(Value::as_object)
        .or_else(|| value.get("result").and_then(Value::as_object))
        .or(direct)
}

fn extract_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) if value.starts_with("http") => Some(value.clone()),
        Value::Object(map) => map.values().find_map(extract_string),
        Value::Array(values) => values.iter().find_map(extract_string),
        _ => None,
    }
}

pub(super) fn site_origin(url: &str) -> Option<String> {
    let scheme_end = url.find("://")?;
    let rest = &url[scheme_end + 3..];
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    Some(format!("{}://{}", &url[..scheme_end], &rest[..end]))
}
