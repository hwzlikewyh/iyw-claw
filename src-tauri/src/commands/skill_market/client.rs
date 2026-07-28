use std::sync::LazyLock;
use std::time::Duration;

use base64::Engine;
use reqwest::{redirect::Policy, Method, RequestBuilder, Url};
use sea_orm::DatabaseConnection;
use serde::Deserialize;

use crate::app_error::AppCommandError;
use crate::commands::iyw_account::iyw_account_access_token_core;

use super::types::SkillMarketUploadFile;

const BASE_URL_ENV: &str = "IYW_CLAW_FUSION_API_BASE_URL";
const MAX_FILES: usize = 512;
const MAX_RAW_BYTES: usize = 25 * 1024 * 1024;
const MAX_BASE64_BYTES: usize = 36 * 1024 * 1024;
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(10 * 60);
#[cfg(debug_assertions)]
const DEFAULT_BASE_URL: &str = "http://127.0.0.1:6001";
#[cfg(all(not(debug_assertions), feature = "test-gateway"))]
const DEFAULT_BASE_URL: &str = "http://192.168.1.86:3201/ai-application";
#[cfg(all(not(debug_assertions), not(feature = "test-gateway")))]
const DEFAULT_BASE_URL: &str = "https://gateway.iyw.cn/iyw-fusion-api";

static HTTP_CLIENT: LazyLock<Result<reqwest::Client, String>> = LazyLock::new(|| {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        // The iyw access token uses a custom header, which reqwest would keep
        // on a cross-origin redirect. API calls must never follow redirects.
        .redirect(Policy::none())
        .build()
        .map_err(|error| error.to_string())
});

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    code: i32,
    data: serde_json::Value,
    message: String,
}

pub fn http_client() -> Result<reqwest::Client, AppCommandError> {
    HTTP_CLIENT.as_ref().cloned().map_err(|error| {
        AppCommandError::configuration_invalid("Failed to initialize Skill market client")
            .with_detail(error.clone())
    })
}

pub fn transfer_timeout() -> Duration {
    TRANSFER_TIMEOUT
}

pub async fn request(
    conn: &DatabaseConnection,
    method: Method,
    path: &str,
) -> Result<RequestBuilder, AppCommandError> {
    let token = iyw_account_access_token_core(conn)
        .await?
        .ok_or_else(|| AppCommandError::authentication_failed("Sign in to iyw-claw first"))?;
    Ok(http_client()?
        .request(method, endpoint(path)?)
        .header("token", token.expose()))
}

pub async fn send(builder: RequestBuilder) -> Result<serde_json::Value, AppCommandError> {
    let response = builder.send().await.map_err(|error| {
        AppCommandError::network("Skill market request failed").with_detail(error.to_string())
    })?;
    if !response.status().is_success() {
        return Err(
            AppCommandError::network("Skill market gateway rejected the request")
                .with_detail(response.status().to_string()),
        );
    }
    let envelope = response.json::<Envelope>().await.map_err(|error| {
        AppCommandError::configuration_invalid("Invalid Skill market response")
            .with_detail(error.to_string())
    })?;
    if envelope.code != 1 {
        return Err(remote_error(envelope.message, &envelope.data));
    }
    Ok(envelope.data)
}

pub fn upload_form(
    fields: Vec<(&'static str, String)>,
    tags: &[String],
    files: Vec<SkillMarketUploadFile>,
) -> Result<reqwest::multipart::Form, AppCommandError> {
    let files = decode_upload_files(files)?;
    let manifest = serde_json::json!({
        "files": files.iter().enumerate().map(|(index, file)| serde_json::json!({
            "field": format!("file_{index:04}"), "path": file.path,
        })).collect::<Vec<_>>()
    });
    let mut form = reqwest::multipart::Form::new().text("manifest", manifest.to_string());
    for (key, value) in fields {
        form = form.text(key, value);
    }
    for tag in tags {
        form = form.text("tags", tag.clone());
    }
    for (index, file) in files.into_iter().enumerate() {
        let part = reqwest::multipart::Part::bytes(file.bytes).file_name("skill-file");
        form = form.part(format!("file_{index:04}"), part);
    }
    Ok(form)
}

struct DecodedUploadFile {
    path: String,
    bytes: Vec<u8>,
}

fn decode_upload_files(
    files: Vec<SkillMarketUploadFile>,
) -> Result<Vec<DecodedUploadFile>, AppCommandError> {
    if files.is_empty() || files.len() > MAX_FILES {
        return Err(AppCommandError::invalid_input(
            "Skill upload must contain 1 to 512 files",
        ));
    }
    let encoded_bytes = files.iter().try_fold(0_usize, |total, file| {
        total.checked_add(file.content_base64.len()).ok_or_else(|| {
            AppCommandError::invalid_input("Skill upload encoded payload is too large")
        })
    })?;
    if encoded_bytes > MAX_BASE64_BYTES {
        return Err(AppCommandError::invalid_input(
            "Skill upload encoded payload exceeds the allowed size",
        ));
    }

    let mut decoded = Vec::with_capacity(files.len());
    let mut total = 0_usize;
    for file in files {
        let expected_size = usize::try_from(file.size)
            .map_err(|_| AppCommandError::invalid_input("Skill file size is invalid"))?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(file.content_base64)
            .map_err(|_| AppCommandError::invalid_input("Skill upload contains invalid Base64"))?;
        if bytes.len() != expected_size {
            return Err(AppCommandError::invalid_input(
                "Skill upload file size does not match its content",
            ));
        }
        total = total
            .checked_add(bytes.len())
            .ok_or_else(|| AppCommandError::invalid_input("Skill upload is too large"))?;
        if total > MAX_RAW_BYTES {
            return Err(AppCommandError::invalid_input(
                "Skill upload exceeds 25 MiB",
            ));
        }
        decoded.push(DecodedUploadFile {
            path: file.path,
            bytes,
        });
    }
    Ok(decoded)
}

fn endpoint(path: &str) -> Result<Url, AppCommandError> {
    let base = std::env::var(BASE_URL_ENV)
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    let base = Url::parse(&format!("{base}/")).map_err(|error| {
        AppCommandError::configuration_invalid("Invalid Skill market base URL")
            .with_detail(error.to_string())
    })?;
    let insecure_dev_allowed = cfg!(debug_assertions) || cfg!(feature = "test-gateway");
    if base.host_str().is_none()
        || !base.username().is_empty()
        || base.password().is_some()
        || base.query().is_some()
        || base.fragment().is_some()
        || (base.scheme() != "https" && !(insecure_dev_allowed && base.scheme() == "http"))
    {
        return Err(AppCommandError::configuration_invalid(
            "Skill market base URL must be an HTTPS origin",
        ));
    }
    base.join(path.trim_start_matches('/')).map_err(|error| {
        AppCommandError::configuration_invalid("Invalid Skill market endpoint")
            .with_detail(error.to_string())
    })
}

fn remote_error(message: String, data: &serde_json::Value) -> AppCommandError {
    let error_code = data
        .get("errorCode")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("SKILL_REMOTE_ERROR");
    let error = match error_code {
        "SKILL_NOT_FOUND" => AppCommandError::not_found(message),
        "SKILL_FORBIDDEN" => AppCommandError::permission_denied(message),
        "SKILL_SLUG_CONFLICT" | "SKILL_VERSION_CONFLICT" | "SKILL_VERSION_NOT_GREATER" => {
            AppCommandError::already_exists(message)
        }
        "SKILL_STORAGE_UNAVAILABLE" => AppCommandError::configuration_missing(message),
        "SKILL_STORAGE_FAILED" => AppCommandError::network(message),
        _ => AppCommandError::invalid_input(message),
    };
    error.with_detail(error_code)
}
