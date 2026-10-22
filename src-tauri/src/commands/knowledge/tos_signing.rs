use std::collections::BTreeMap;

use hmac::{Hmac, Mac};
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Url,
};
use sha2::{Digest, Sha256};

use super::upload_models::{UploadCredentials, UploadedObject};
use crate::app_error::AppCommandError;

const ALGORITHM: &str = "TOS4-HMAC-SHA256";
const PAYLOAD_HASH: &str = "UNSIGNED-PAYLOAD";

pub fn object(
    credentials: &UploadCredentials,
    file_name: &str,
) -> Result<(Url, UploadedObject), AppCommandError> {
    let host = upload_host(credentials)?;
    let key = object_key(credentials, file_name)?;
    let path = key
        .split('/')
        .map(|part| urlencoding::encode(part).into_owned())
        .collect::<Vec<_>>()
        .join("/");
    let url = Url::parse(&format!("https://{host}/{path}")).map_err(|_| invalid_credentials())?;
    let public_url = public_url(&credentials.domain, &path)?;
    Ok((
        url,
        UploadedObject {
            key: key.clone(),
            public_url,
            tos_url: format!("tos://{}/{key}", credentials.bucket),
        },
    ))
}

fn upload_host(credentials: &UploadCredentials) -> Result<String, AppCommandError> {
    let raw = credentials.endpoint.trim().trim_end_matches('/');
    let raw = if raw.starts_with("https://") {
        raw.to_string()
    } else {
        format!("https://{raw}")
    };
    let url = Url::parse(&raw).map_err(|_| invalid_credentials())?;
    let host = url.host_str().ok_or_else(invalid_credentials)?;
    let valid_bucket = (3..=63).contains(&credentials.bucket.len())
        && credentials
            .bucket
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-');
    if !valid_bucket || !clean_https_origin(&url) || !host.ends_with(".volces.com") {
        return Err(invalid_credentials());
    }
    Ok(format!("{}.{host}", credentials.bucket))
}

fn clean_https_origin(url: &Url) -> bool {
    url.scheme() == "https"
        && url.port().is_none()
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none()
        && url.username().is_empty()
        && url.password().is_none()
}

fn object_key(credentials: &UploadCredentials, file_name: &str) -> Result<String, AppCommandError> {
    let prefix = credentials.object_prefix.trim_start_matches('/');
    if prefix.is_empty()
        || prefix.len() > 2048
        || prefix.chars().any(|ch| ch.is_control() || ch == '\\')
        || prefix.split('/').any(|part| matches!(part, "." | ".."))
    {
        return Err(invalid_credentials());
    }
    let lower = file_name.to_ascii_lowercase();
    let extension = if lower.ends_with(".faq.xlsx") {
        "faq.xlsx"
    } else {
        lower.rsplit('.').next().unwrap_or_default()
    };
    Ok(format!(
        "{}/{}.{extension}",
        prefix.trim_end_matches('/'),
        uuid::Uuid::new_v4().simple()
    ))
}

fn public_url(domain: &str, path: &str) -> Result<String, AppCommandError> {
    let domain = domain
        .trim()
        .trim_end_matches('/')
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let base = Url::parse(&format!("https://{domain}/")).map_err(|_| invalid_credentials())?;
    let host = base.host_str().ok_or_else(invalid_credentials)?;
    if !clean_https_origin(&base) || !(host.ends_with(".iyw.cn") || host.ends_with(".volces.com")) {
        return Err(invalid_credentials());
    }
    Ok(format!("{base}{path}"))
}

pub fn headers(credentials: &UploadCredentials, url: &Url) -> Result<HeaderMap, AppCommandError> {
    let date = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let mut values = signable_headers(credentials, url, &date)?;
    let authorization = authorization(credentials, &values, url.path())?;
    values.insert("authorization", authorization);
    values
        .into_iter()
        .map(|(name, value)| {
            let name =
                HeaderName::from_bytes(name.as_bytes()).map_err(|_| invalid_credentials())?;
            let mut value = HeaderValue::from_str(&value).map_err(|_| invalid_credentials())?;
            value.set_sensitive(true);
            Ok((name, value))
        })
        .collect()
}

fn signable_headers(
    credentials: &UploadCredentials,
    url: &Url,
    date: &str,
) -> Result<BTreeMap<&'static str, String>, AppCommandError> {
    let token = if credentials.sts_token.is_empty() {
        &credentials.security_token
    } else {
        &credentials.sts_token
    };
    if token.is_empty()
        || credentials.access_key_id.is_empty()
        || credentials.access_key_secret.is_empty()
    {
        return Err(invalid_credentials());
    }
    Ok(BTreeMap::from([
        (
            "host",
            url.host_str().ok_or_else(invalid_credentials)?.to_string(),
        ),
        ("x-tos-content-sha256", PAYLOAD_HASH.to_string()),
        ("x-tos-date", date.to_string()),
        ("x-tos-security-token", token.clone()),
    ]))
}

fn authorization(
    credentials: &UploadCredentials,
    values: &BTreeMap<&str, String>,
    path: &str,
) -> Result<String, AppCommandError> {
    let date = &values["x-tos-date"];
    let signed = values.keys().copied().collect::<Vec<_>>().join(";");
    let canonical = values
        .iter()
        .map(|(key, value)| format!("{key}:{}\n", value.trim()))
        .collect::<String>();
    let canonical = format!("PUT\n{path}\n\n{canonical}\n{signed}\n{PAYLOAD_HASH}");
    let scope = format!("{}/{}/tos/request", &date[..8], credentials.region);
    let to_sign = format!(
        "{ALGORITHM}\n{date}\n{scope}\n{:x}",
        Sha256::digest(canonical.as_bytes())
    );
    // TOS 官方派生过程直接以 secret 为初始密钥，不添加 AWS4 前缀。
    let mut key = credentials.access_key_secret.as_bytes().to_vec();
    for part in [&date[..8], &credentials.region, "tos", "request"] {
        key = hmac(&key, part)?;
    }
    let signature = hmac(&key, &to_sign)?
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!(
        "{ALGORITHM} Credential={}/{scope}, SignedHeaders={signed}, Signature={signature}",
        credentials.access_key_id
    ))
}

fn hmac(key: &[u8], value: &str) -> Result<Vec<u8>, AppCommandError> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).map_err(|_| invalid_credentials())?;
    mac.update(value.as_bytes());
    Ok(mac.finalize().into_bytes().to_vec())
}

fn invalid_credentials() -> AppCommandError {
    AppCommandError::network("云端上传凭证无效，请重新上传")
}
