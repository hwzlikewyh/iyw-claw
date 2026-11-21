use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};

// 与腾讯发布的稳定插件基线同步。宿主版本只放在后缀里。
const PLUGIN_VERSION: [u8; 3] = [2, 4, 10];
const HOST_VERSION: &str = env!("CARGO_PKG_VERSION");

pub(super) fn text_item(text: &str) -> Value {
    json!({ "type": 1, "text_item": { "text": text } })
}

pub(super) fn base_info() -> Value {
    let [major, minor, patch] = PLUGIN_VERSION;
    json!({
        "channel_version": format!("{major}.{minor}.{patch}/iyw-claw-{HOST_VERSION}"),
        "bot_agent": concat!("iyw-claw/", env!("CARGO_PKG_VERSION")),
    })
}

pub(super) fn app_headers() -> HeaderMap {
    let [major, minor, patch] = PLUGIN_VERSION;
    let version = u32::from_be_bytes([0, major, minor, patch]);
    let mut headers = HeaderMap::new();
    headers.insert("ilink-app-id", HeaderValue::from_static("bot"));
    headers.insert("ilink-app-clientversion", HeaderValue::from(version));
    headers
}

pub(super) fn post_headers(token: Option<&str>) -> HeaderMap {
    let mut headers = app_headers();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        "authorizationtype",
        HeaderValue::from_static("ilink_bot_token"),
    );
    let uin = STANDARD.encode(rand::random::<u32>().to_string());
    headers.insert(
        "x-wechat-uin",
        HeaderValue::from_str(&uin).expect("base64 header"),
    );
    if let Some(token) = token {
        if let Ok(mut value) = HeaderValue::from_str(&format!("Bearer {}", token.trim())) {
            value.set_sensitive(true);
            headers.insert(AUTHORIZATION, value);
        }
    }
    headers
}
