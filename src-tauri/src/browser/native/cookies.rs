use serde_json::{json, Value};
use tauri_runtime_wry::wry::{
    cookie::{Cookie, SameSite},
    WebView,
};

pub(super) fn replace(view: &WebView, values: &Value) -> Result<Value, String> {
    let cookies = values["cookies"]
        .as_array()
        .ok_or("Cookie transfer is invalid")?;
    let prepared = cookies
        .iter()
        .filter(|value| value.get("partitionKey").is_none() && value["partitioned"] != true)
        .map(cookie_from_value)
        .collect::<Result<Vec<_>, _>>()?;
    let previous = view.cookies().map_err(|error| error.to_string())?;
    let result = apply_cookies(view, &prepared, &previous);
    if let Err(error) = result {
        let rollback = apply_cookies(view, &previous, &prepared);
        return Err(format!(
            "Cookie import failed: {error}; rollback {}",
            if rollback.is_ok() {
                "completed"
            } else {
                "failed"
            }
        ));
    }
    Ok(json!({"imported":prepared.len(),"skipped":cookies.len().saturating_sub(prepared.len())}))
}

pub(super) fn set_value(view: &WebView, value: &Value) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        super::windows_cookies::set(view, value)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let cookie = cookie_from_value(value)?;
        view.set_cookie(&cookie).map_err(|error| error.to_string())
    }
}

fn apply_cookies(
    view: &WebView,
    target: &[Cookie<'_>],
    previous: &[Cookie<'_>],
) -> Result<(), String> {
    for cookie in target {
        view.set_cookie(cookie).map_err(|error| error.to_string())?;
    }
    for old in previous {
        if !target.iter().any(|value| {
            value.name() == old.name()
                && value.domain() == old.domain()
                && value.path() == old.path()
        }) {
            view.delete_cookie(old).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

pub(super) fn cookie_from_value(value: &Value) -> Result<Cookie<'static>, String> {
    let name = value["name"].as_str().ok_or("Cookie name is missing")?;
    let contents = value["value"].as_str().ok_or("Cookie value is missing")?;
    let domain = value["domain"].as_str().ok_or("Cookie domain is missing")?;
    let mut cookie = Cookie::build((name.to_string(), contents.to_string()))
        .domain(domain.to_string())
        .path(value["path"].as_str().unwrap_or("/").to_string())
        .secure(value["secure"].as_bool().unwrap_or(false))
        .http_only(value["httpOnly"].as_bool().unwrap_or(false))
        .build();
    match value["sameSite"].as_str() {
        Some("Strict") => cookie.set_same_site(SameSite::Strict),
        Some("Lax") => cookie.set_same_site(SameSite::Lax),
        Some("None") => cookie.set_same_site(SameSite::None),
        _ => {}
    }
    if let Some(expires) = value["expires"]
        .as_f64()
        .filter(|value| value.is_finite() && *value > 0.0)
    {
        let expires = tauri_runtime_wry::wry::cookie::time::OffsetDateTime::from_unix_timestamp(
            expires as i64,
        )
        .map_err(|error| error.to_string())?;
        cookie.set_expires(expires);
    }
    Ok(cookie)
}

pub(super) fn command(view: &WebView, values: &Value) -> Result<Value, String> {
    let args = values["args"]
        .as_array()
        .ok_or("Cookie command arguments are required")?;
    match args.get(1).and_then(Value::as_str).unwrap_or("get") {
        "get" => Ok(
            json!({"cookies": view.cookies().map_err(|error| error.to_string())?.iter().map(to_value).collect::<Vec<_>>()}),
        ),
        "set" => {
            let name = args
                .get(2)
                .and_then(Value::as_str)
                .ok_or("Cookie name is required")?;
            let value = args
                .get(3)
                .and_then(Value::as_str)
                .ok_or("Cookie value is required")?;
            let url = view.url().map_err(|error| error.to_string())?;
            let url = reqwest::Url::parse(&url).map_err(|_| "Cookie requires an HTTP page")?;
            let domain = url.host_str().ok_or("Cookie requires a host")?;
            set_value(
                view,
                &json!({"name":name,"value":value,"domain":domain,"path":"/","secure":url.scheme() == "https"}),
            )?;
            Ok(json!({"success":true}))
        }
        "clear" => {
            for cookie in view.cookies().map_err(|error| error.to_string())? {
                view.delete_cookie(&cookie)
                    .map_err(|error| error.to_string())?;
            }
            Ok(json!({"success":true}))
        }
        _ => Err("Unsupported cookie command".into()),
    }
}

pub(super) fn to_value(cookie: &Cookie<'_>) -> Value {
    json!({"name":cookie.name(),"value":cookie.value(),"domain":cookie.domain(),"path":cookie.path(),
        "secure":cookie.secure().unwrap_or(false),"httpOnly":cookie.http_only().unwrap_or(false),
        "sameSite":cookie.same_site().map(|value| value.to_string()),"expires":cookie.expires_datetime().map(|value| value.unix_timestamp())})
}

impl super::NativeBrowser {
    pub(in crate::browser) async fn restore_cookies(
        &self,
        id: &str,
    ) -> Result<(), super::super::error::BrowserError> {
        let mut transfer = self.cookie_transfer.lock().await;
        let Some((source, cookies)) = transfer.as_ref() else {
            return Ok(());
        };
        if *source == super::super::types::BrowserEngineKind::Webview {
            return Ok(());
        }
        let value = self
            .command(id, "import_cookies", json!({"cookies":cookies}))
            .await?;
        tracing::info!(
            engine = "webview",
            imported = value["imported"].as_u64(),
            skipped = value["skipped"].as_u64(),
            "browser cookies transferred"
        );
        transfer.take();
        Ok(())
    }
}

impl super::super::manager::BrowserSessionManager {
    pub(in crate::browser) async fn restore_cookies_to_full(
        &self,
    ) -> Result<(), super::super::error::BrowserError> {
        let mut transfer = self.native.cookie_transfer.lock().await;
        let Some((source, cookies)) = transfer.as_ref() else {
            return Ok(());
        };
        if *source != super::super::types::BrowserEngineKind::Webview {
            return Ok(());
        }
        let cookies = full_cookie_values(cookies);
        self.import_cookies_to_full(&cookies).await?;
        transfer.take();
        Ok(())
    }

    async fn import_cookies_to_full(
        &self,
        cookies: &[Value],
    ) -> Result<(), super::super::error::BrowserError> {
        let previous = self.cdp_call("Storage.getCookies", json!({}), None).await?;
        if let Err(error) = self.apply_full_cookies(cookies, &previous).await {
            let original = full_cookie_values(
                previous["cookies"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
            );
            let restored = self
                .apply_full_cookies(&original, &json!({"cookies":cookies}))
                .await;
            tracing::warn!(
                rollback_succeeded = restored.is_ok(),
                "full browser cookie import failed"
            );
            return Err(error);
        }
        tracing::info!(
            engine = "chromix",
            cookie_count = cookies.len(),
            "browser cookies transferred"
        );
        Ok(())
    }

    async fn apply_full_cookies(
        &self,
        cookies: &[Value],
        previous: &Value,
    ) -> Result<(), super::super::error::BrowserError> {
        self.cdp_call("Storage.setCookies", json!({"cookies":cookies}), None)
            .await?;
        self.delete_stale_full_cookies(cookies, previous).await
    }

    async fn delete_stale_full_cookies(
        &self,
        cookies: &[Value],
        previous: &Value,
    ) -> Result<(), super::super::error::BrowserError> {
        for old in previous["cookies"].as_array().into_iter().flatten() {
            if old.get("partitionKey").is_some() {
                continue;
            }
            let exists = cookies.iter().any(|value| {
                value["name"] == old["name"]
                    && value["domain"] == old["domain"]
                    && value["path"] == old["path"]
            });
            if !exists {
                self.cdp_call(
                    "Network.deleteCookies",
                    json!({"name":old["name"],"domain":old["domain"],"path":old["path"]}),
                    None,
                )
                .await?;
            }
        }
        Ok(())
    }
}

fn full_cookie_values(cookies: &[Value]) -> Vec<Value> {
    cookies
        .iter()
        .filter(|value| value.get("partitionKey").is_none() && value["partitioned"] != true)
        .map(|cookie| {
            let fields = [
                "name", "value", "domain", "path", "secure", "httpOnly", "sameSite", "expires",
            ];
            Value::Object(
                fields
                    .into_iter()
                    .filter_map(|key| {
                        cookie
                            .get(key)
                            .filter(|value| {
                                !value.is_null()
                                    && (key != "expires"
                                        || value.as_f64().is_some_and(|value| value > 0.0))
                            })
                            .map(|value| (key.to_string(), value.clone()))
                    })
                    .collect(),
            )
        })
        .collect::<Vec<_>>()
}
