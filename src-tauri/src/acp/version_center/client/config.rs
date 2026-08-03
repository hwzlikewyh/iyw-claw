use std::sync::LazyLock;
use std::time::Duration;

use reqwest::{redirect::Policy, Url};

use crate::app_error::AppCommandError;

const BASE_URL_ENV: &str = "IYW_CLAW_AGENT_PLATFORM_BASE_URL";
const FUSION_BASE_URL_ENV: &str = "IYW_CLAW_FUSION_API_BASE_URL";

#[cfg(debug_assertions)]
const DEFAULT_BASE_URL: &str = "http://127.0.0.1:6001";
#[cfg(all(not(debug_assertions), feature = "test-gateway"))]
const DEFAULT_BASE_URL: &str = "http://192.168.1.86:3201/ai-application";
#[cfg(all(not(debug_assertions), not(feature = "test-gateway")))]
const DEFAULT_BASE_URL: &str = "https://gateway.iyw.cn/iyw-fusion-api";

pub(super) const INSTALLATION_HEADER: &str = "X-IYW-Installation-ID";

static HTTP_CLIENT: LazyLock<Result<reqwest::Client, String>> = LazyLock::new(|| {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .redirect(Policy::none())
        .build()
        .map_err(|error| error.to_string())
});

pub(super) fn http_client() -> Result<reqwest::Client, AppCommandError> {
    HTTP_CLIENT.as_ref().cloned().map_err(|error| {
        AppCommandError::configuration_invalid("Failed to initialize Agent platform client")
            .with_detail(error)
    })
}

pub(super) fn endpoint(path: &str) -> Result<Url, AppCommandError> {
    let parsed = Url::parse(&format!("{}/", configured_base_url())).map_err(|error| {
        AppCommandError::configuration_invalid("Invalid Agent platform base URL")
            .with_detail(error.to_string())
    })?;
    let insecure_dev_allowed = cfg!(debug_assertions) || cfg!(feature = "test-gateway");
    let valid = parsed.host_str().is_some()
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.query().is_none()
        && parsed.fragment().is_none()
        && (parsed.scheme() == "https" || (insecure_dev_allowed && parsed.scheme() == "http"));
    if !valid {
        return Err(AppCommandError::configuration_invalid(
            "Agent platform base URL must be an HTTPS origin",
        ));
    }
    parsed.join(path.trim_start_matches('/')).map_err(|error| {
        AppCommandError::configuration_invalid("Invalid Agent platform endpoint")
            .with_detail(error.to_string())
    })
}

fn configured_base_url() -> String {
    for key in [BASE_URL_ENV, FUSION_BASE_URL_ENV] {
        if let Ok(value) = std::env::var(key) {
            let value = value.trim().trim_end_matches('/');
            if !value.is_empty() {
                return value.to_string();
            }
        }
    }
    DEFAULT_BASE_URL.to_string()
}
