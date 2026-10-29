use std::sync::Mutex;

use tauri::{AppHandle, Manager};
use tauri_plugin_deep_link::DeepLinkExt;

static PENDING: Mutex<Option<String>> = Mutex::new(None);

fn share_fragment(url: &tauri::Url) -> Option<String> {
    if url.scheme() != "iyw-assistant" || url.host_str() != Some("share") ||
        !matches!(url.path(), "" | "/") || url.query().is_some() {
        return None;
    }
    let fragment = url.fragment()?;
    let params: Vec<_> = tauri::Url::parse(&format!("https://share.invalid/?{fragment}")).ok()?
        .query_pairs().map(|(key, value)| (key.into_owned(), value.into_owned())).collect();
    let token = params.iter().find(|(key, _)| key == "token")?.1.clone();
    let api = params.iter().find(|(key, _)| key == "api")?.1.clone();
    if params.len() != 2 || token.len() != 64 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let expected = crate::commands::skill_market::client::endpoint("/").ok()?;
    if api.trim_end_matches('/') != expected.as_str().trim_end_matches('/') { return None; }
    let mut target = tauri::Url::parse("https://share.invalid/").ok()?;
    target.query_pairs_mut().append_pair("token", &token).append_pair("api", &api);
    Some(target.query()?.to_owned())
}

pub fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    // 安装包负责系统注册；不在调试启动时抢占已安装客户端的协议。
    #[cfg(all(target_os = "linux", not(debug_assertions)))]
    app.deep_link().register_all()?;
    if let Some(urls) = app.deep_link().get_current()? {
        for url in urls { queue(app, &url); }
    }
    let handle = app.clone();
    app.deep_link().on_open_url(move |event| {
        for url in event.urls() { queue(&handle, &url); }
    });
    Ok(())
}

fn queue(app: &AppHandle, url: &tauri::Url) {
    let Some(fragment) = share_fragment(url) else { return; };
    if let Ok(mut pending) = PENDING.lock() { *pending = Some(fragment); }
    navigate_pending(app);
}

pub fn navigate_pending(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else { return; };
    let fragment = PENDING.lock().ok().and_then(|mut pending| pending.take());
    let Some(fragment) = fragment else { return; };
    let Ok(mut url) = window.url() else { return; };
    url.set_path("/share");
    url.set_query(None);
    url.set_fragment(Some(&fragment));
    if window.navigate(url).is_ok() {
        crate::commands::windows::show_main_window(app);
    }
}
