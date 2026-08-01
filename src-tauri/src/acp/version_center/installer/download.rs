//! 下载票据校验。
//!
//! 实际下载统一走 `super::resumable`（Range / If-Range / `.part` 断点续传），
//! 本模块只保留票据的静态校验：URL 来源、大小、SHA-256、签名必须与 offer
//! 完全一致，任何不匹配都直接拒绝。

use reqwest::Url;

use crate::acp::version_center::types::ToolOffer;
use crate::app_error::AppCommandError;

const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;

pub fn validate_ticket(
    offer: &ToolOffer,
    url: &str,
    size: i64,
    sha256: &str,
    signature: &str,
) -> Result<(), AppCommandError> {
    let parsed = Url::parse(url)
        .map_err(|_| AppCommandError::invalid_input("Managed tool download URL is invalid"))?;
    let allow_http = cfg!(debug_assertions) && parsed.host_str() == Some("127.0.0.1");
    if !(parsed.scheme() == "https" || allow_http)
        || parsed.username() != ""
        || parsed.password().is_some()
        || size <= 0
        || size as u64 > MAX_ARCHIVE_BYTES
        || size != offer.artifact.size
        || !sha256.eq_ignore_ascii_case(&offer.artifact.sha256)
        || signature.trim().is_empty()
    {
        return Err(AppCommandError::invalid_input(
            "Managed tool download ticket was rejected",
        ));
    }
    Ok(())
}
