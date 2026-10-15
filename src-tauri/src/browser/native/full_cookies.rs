use serde_json::{json, Map, Value};

use super::super::error::BrowserError;
use super::super::manager::BrowserSessionManager;
use super::super::types::BrowserEngineKind;

const COOKIE_EXPIRY: f64 = 1.0;
const COOKIE_FALLBACK_LIMIT: usize = 32;
const FULL_ERROR_MESSAGE: &str = "跨内核 Cookie 同步未完成；完整浏览器仍可使用";

impl BrowserSessionManager {
    pub(in crate::browser) async fn restore_cookies_to_full(&self) -> Result<(), BrowserError> {
        let transfer = self.native.cookie_transfer.lock().await.clone();
        let Some((source, cookies)) = transfer else {
            return Ok(());
        };
        if source != BrowserEngineKind::Webview {
            return Ok(());
        }
        self.import_cookies_to_full(&cookies).await?;
        let mut pending = self.native.cookie_transfer.lock().await;
        let same_transfer = pending
            .as_ref()
            .is_some_and(|(pending_source, pending_cookies)| {
                *pending_source == source && *pending_cookies == cookies
            });
        if same_transfer {
            pending.take();
        }
        *self.native.full_error.lock().await = None;
        Ok(())
    }

    async fn import_cookies_to_full(&self, source: &[Value]) -> Result<(), BrowserError> {
        let previous = self.cookie_snapshot().await;
        let cookies = full_cookie_values(source);
        if cookies.is_empty() {
            tracing::info!(
                target: "iyw_claw_browser",
                source_cookie_count = source.len(),
                "browser cookie transfer had no compatible unpartitioned cookies"
            );
            return Ok(());
        }
        let complete = self.write_source_cookies(&cookies).await;
        let stale = self
            .remove_stale_cookies(&cookies, previous.as_ref(), complete)
            .await;
        tracing::info!(
            target: "iyw_claw_browser",
            engine = "chromix",
            attempted = cookies.len(),
            complete,
            skipped = source.len().saturating_sub(cookies.len()),
            stale,
            "browser cookies transferred"
        );
        if !complete {
            *self.native.full_error.lock().await = Some(FULL_ERROR_MESSAGE.to_string());
            return Err(BrowserError::new(
                super::super::error::BrowserErrorCode::BrowserInvalidArgument,
                FULL_ERROR_MESSAGE,
            ));
        }
        Ok(())
    }

    async fn cookie_snapshot(&self) -> Option<Value> {
        match self.cdp_call("Storage.getCookies", json!({}), None).await {
            Ok(value) => Some(value),
            Err(error) => {
                tracing::warn!(
                    target: "iyw_claw_browser",
                    cdp_method = "Storage.getCookies",
                    error_code = ?error.code,
                    "browser cookie snapshot unavailable; stale cleanup was skipped"
                );
                None
            }
        }
    }

    async fn write_source_cookies(&self, cookies: &[Value]) -> bool {
        if self
            .cdp_call("Storage.setCookies", json!({ "cookies": cookies }), None)
            .await
            .is_ok()
        {
            return true;
        }
        tracing::warn!(
            target: "iyw_claw_browser",
            cdp_method = "Storage.setCookies",
            cookie_count = cookies.len(),
            "browser cookie batch rejected; retrying cookies individually"
        );
        self.import_cookies_individually(cookies).await
    }

    async fn remove_stale_cookies(
        &self,
        cookies: &[Value],
        previous: Option<&Value>,
        complete: bool,
    ) -> usize {
        let stale = previous
            .filter(|_| complete)
            .map(|value| stale_cookie_values(cookies, value))
            .unwrap_or_default();
        if !stale.is_empty() {
            if let Err(error) = self
                .cdp_call("Storage.setCookies", json!({ "cookies": stale }), None)
                .await
            {
                tracing::warn!(
                    target: "iyw_claw_browser",
                    error_code = ?error.code,
                    stale_cookie_count = stale.len(),
                    "browser stale cookie cleanup was skipped"
                );
            }
        }
        stale.len()
    }

    async fn import_cookies_individually(&self, cookies: &[Value]) -> bool {
        let mut rejected = 0_usize;
        for cookie in cookies.iter().take(COOKIE_FALLBACK_LIMIT) {
            if self
                .cdp_call("Storage.setCookies", json!({ "cookies": [cookie] }), None)
                .await
                .is_err()
            {
                rejected = rejected.saturating_add(1);
            }
        }
        rejected = rejected.saturating_add(cookies.len().saturating_sub(COOKIE_FALLBACK_LIMIT));
        tracing::info!(
            target: "iyw_claw_browser",
            attempted = cookies.len().min(COOKIE_FALLBACK_LIMIT),
            imported = cookies.len().saturating_sub(rejected),
            rejected,
            "browser cookie fallback completed"
        );
        rejected == 0
    }
}

fn full_cookie_values(cookies: &[Value]) -> Vec<Value> {
    cookies.iter().filter_map(cookie_param).collect()
}

fn cookie_param(cookie: &Value) -> Option<Value> {
    if cookie.get("partitionKey").is_some() || cookie["partitioned"] == true {
        return None;
    }
    let name = cookie["name"].as_str()?.to_string();
    let value = cookie["value"].as_str()?.to_string();
    let domain = cookie["domain"].as_str()?.trim().to_string();
    if !valid_prefix_cookie(&name, &domain, cookie) {
        return None;
    }
    let path = cookie["path"]
        .as_str()
        .filter(|path| !path.is_empty())
        .unwrap_or("/");
    let mut result = Map::new();
    result.insert("name".into(), Value::String(name));
    result.insert("value".into(), Value::String(value));
    result.insert("path".into(), Value::String(path.to_string()));
    result.insert(
        "secure".into(),
        json!(cookie["secure"].as_bool().unwrap_or(false)),
    );
    result.insert(
        "httpOnly".into(),
        json!(cookie["httpOnly"].as_bool().unwrap_or(false)),
    );
    if let Some(same_site) = cookie["sameSite"].as_str().filter(|value| {
        matches!(*value, "Strict" | "Lax")
            || (*value == "None" && cookie["secure"].as_bool().unwrap_or(false))
    }) {
        result.insert("sameSite".into(), Value::String(same_site.to_string()));
    }
    if let Some(expires) = cookie["expires"]
        .as_f64()
        .filter(|value| value.is_finite() && *value > 0.0)
    {
        result.insert("expires".into(), json!(expires));
    }
    if domain.starts_with('.') {
        result.insert("domain".into(), Value::String(domain));
    } else {
        result.insert(
            "url".into(),
            Value::String(cookie_url(
                &domain,
                cookie["secure"].as_bool().unwrap_or(false),
            )),
        );
    }
    Some(Value::Object(result))
}

fn valid_prefix_cookie(name: &str, domain: &str, cookie: &Value) -> bool {
    if domain.is_empty() {
        return false;
    }
    let secure = cookie["secure"].as_bool().unwrap_or(false);
    if name.starts_with("__Host-") {
        return secure && cookie["path"].as_str().unwrap_or("/") == "/" && !domain.starts_with('.');
    }
    secure || !name.starts_with("__Secure-")
}

fn cookie_url(domain: &str, secure: bool) -> String {
    let host = domain.trim_start_matches('.');
    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    format!("{}://{host}/", if secure { "https" } else { "http" })
}

fn stale_cookie_values(target: &[Value], previous: &Value) -> Vec<Value> {
    previous["cookies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|cookie| {
            cookie_param(cookie).is_some() && !target.iter().any(|value| same_cookie(value, cookie))
        })
        .filter_map(cookie_param)
        .map(expire_cookie)
        .collect()
}

fn same_cookie(target: &Value, previous: &Value) -> bool {
    let Some(previous) = cookie_param(previous) else {
        return false;
    };
    target["name"] == previous["name"]
        && target["path"] == previous["path"]
        && target.get("domain") == previous.get("domain")
        && target.get("url") == previous.get("url")
}

fn expire_cookie(mut cookie: Value) -> Value {
    cookie["value"] = Value::String(String::new());
    cookie["expires"] = json!(COOKIE_EXPIRY);
    cookie
}
