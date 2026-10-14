//! 保留 WebView2 Cookie 的原始 domain，避免通用 Cookie 类型丢失前导点。
use serde_json::{json, Value};
use tauri_runtime_wry::wry::{WebView, WebViewExtWindows};
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use windows::core::{Interface, HSTRING};

pub(super) fn get(
    view: &WebView,
    done: impl FnOnce(Result<Value, String>) + 'static,
) -> Result<(), String> {
    super::cdp::call(&view.webview(), "Network.getAllCookies", json!({}), done)
}

pub(super) fn set(view: &WebView, value: &Value) -> Result<(), String> {
    super::cookies::cookie_from_value(value)?;
    let manager = manager(view)?;
    unsafe {
        let cookie = manager
            .CreateCookie(
                &HSTRING::from(value["name"].as_str().unwrap_or("")),
                &HSTRING::from(value["value"].as_str().unwrap_or("")),
                &HSTRING::from(value["domain"].as_str().unwrap_or("")),
                &HSTRING::from(value["path"].as_str().unwrap_or("/")),
            )
            .map_err(|error| error.to_string())?;
        configure(&cookie, value).map_err(|error| error.to_string())?;
        manager
            .AddOrUpdateCookie(&cookie)
            .map_err(|error| error.to_string())
    }
}

unsafe fn configure(cookie: &ICoreWebView2Cookie, value: &Value) -> windows::core::Result<()> {
    cookie.SetIsSecure(value["secure"].as_bool().unwrap_or(false))?;
    cookie.SetIsHttpOnly(value["httpOnly"].as_bool().unwrap_or(false))?;
    let same_site = match value["sameSite"].as_str() {
        Some("Strict") => COREWEBVIEW2_COOKIE_SAME_SITE_KIND_STRICT,
        Some("None") => COREWEBVIEW2_COOKIE_SAME_SITE_KIND_NONE,
        _ => COREWEBVIEW2_COOKIE_SAME_SITE_KIND_LAX,
    };
    cookie.SetSameSite(same_site)?;
    if let Some(expires) = value["expires"]
        .as_f64()
        .filter(|value| value.is_finite() && *value > 0.0)
    {
        cookie.SetExpires(expires)?;
    }
    Ok(())
}

pub(super) fn replace(view: &WebView, values: &Value, previous: &Value) -> Result<Value, String> {
    let cookies = values["cookies"]
        .as_array()
        .ok_or("Invalid Cookie transfer")?;
    let prepared = cookies
        .iter()
        .filter(|value| value.get("partitionKey").is_none() && value["partitioned"] != true)
        .cloned()
        .collect::<Vec<_>>();
    for cookie in &prepared {
        super::cookies::cookie_from_value(cookie)?;
    }
    let old = previous["cookies"]
        .as_array()
        .ok_or("Invalid Cookie snapshot")?;
    if let Err(error) = apply(view, &prepared, old) {
        let rollback = apply(view, old, &prepared);
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

fn apply(view: &WebView, target: &[Value], previous: &[Value]) -> Result<(), String> {
    for cookie in target
        .iter()
        .filter(|value| value.get("partitionKey").is_none())
    {
        set(view, cookie)?;
    }
    let manager = manager(view)?;
    for cookie in previous
        .iter()
        .filter(|value| value.get("partitionKey").is_none())
    {
        if target.iter().any(|value| {
            value["name"] == cookie["name"]
                && value["domain"] == cookie["domain"]
                && value["path"] == cookie["path"]
        }) {
            continue;
        }
        unsafe {
            manager.DeleteCookiesWithDomainAndPath(
                &HSTRING::from(cookie["name"].as_str().unwrap_or("")),
                &HSTRING::from(cookie["domain"].as_str().unwrap_or("")),
                &HSTRING::from(cookie["path"].as_str().unwrap_or("/")),
            )
        }
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn manager(view: &WebView) -> Result<ICoreWebView2CookieManager, String> {
    unsafe {
        view.webview()
            .cast::<ICoreWebView2_2>()
            .and_then(|core| core.CookieManager())
    }
    .map_err(|error| error.to_string())
}
