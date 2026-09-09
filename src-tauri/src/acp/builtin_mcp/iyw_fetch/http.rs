use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Url};
use rmcp::ErrorData;
use serde_json::json;

const HTTP_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_REDIRECTS: usize = 5;
const DEFAULT_HEADERS: [(&str, &str); 13] = [
    ("accept", "application/json, text/plain, */*"),
    ("accept-language", "zh-CN,zh;q=0.9"),
    ("content-type", "application/json"),
    ("origin", "https://tu.iyw.cn"),
    ("priority", "u=1, i"),
    ("referer", "https://tu.iyw.cn/trendPop?from=portal"),
    ("sec-ch-ua", "\"Chromium\";v=\"152\", \"Not?A_Brand\";v=\"24\", \"Google Chrome\";v=\"152\""),
    ("sec-ch-ua-mobile", "?0"),
    ("sec-ch-ua-platform", "\"Windows\""),
    ("sec-fetch-dest", "empty"),
    ("sec-fetch-mode", "cors"),
    ("sec-fetch-site", "same-site"),
    ("user-agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/152.0.0.0 Safari/537.36"),
];

pub(super) fn allowed_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
        && url
            .domain()
            .is_some_and(|host| host == "iyw.cn" || host.ends_with(".iyw.cn"))
}

pub(super) fn client() -> Result<Client, ErrorData> {
    let headers: HeaderMap = DEFAULT_HEADERS
        .into_iter()
        .map(|(name, value)| {
            (
                HeaderName::from_static(name),
                HeaderValue::from_static(value),
            )
        })
        .collect();
    Client::builder()
        .default_headers(headers)
        .timeout(HTTP_TIMEOUT)
        .referer(false)
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if !allowed_url(attempt.url()) || attempt.previous().len() > MAX_REDIRECTS {
                tracing::warn!(target: "builtin_mcp", status = attempt.status().as_u16(),
                    hops = attempt.previous().len(), "[iyw-fetch] redirect blocked");
                return attempt.stop();
            }
            attempt.follow()
        }))
        .build()
        .map_err(|_| {
            ErrorData::internal_error(
                "Failed to initialize IYW HTTP client",
                Some(json!({"code": "client_error", "execution_status": "not_started"})),
            )
        })
}
