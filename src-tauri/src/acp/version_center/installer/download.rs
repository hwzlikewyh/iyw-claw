//! 下载票据校验。
//!
//! 实际下载统一走 `super::resumable`（Range / If-Range / `.part` 断点续传），
//! 本模块只保留票据的静态校验：URL 来源、大小与 SHA-256 必须与 offer
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
    _signature: &str,
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
    {
        return Err(AppCommandError::invalid_input(
            "Managed tool download ticket was rejected",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_ticket;
    use crate::acp::version_center::types::{ToolArtifact, ToolOffer};

    #[test]
    fn accepts_unsigned_ticket_with_matching_integrity_metadata() {
        let offer = ToolOffer {
            revision: 1,
            tool_id: "node".into(),
            version_id: "version-1".into(),
            version: "24.18.1".into(),
            channel: "stable".into(),
            security_status: "normal".into(),
            selection_reason: "required".into(),
            effective_update_policy: "auto".into(),
            required: true,
            artifact: ToolArtifact {
                id: "artifact-1".into(),
                runtime: "node".into(),
                target: "windows".into(),
                arch: "x86_64".into(),
                package_kind: "zip".into(),
                size: 7,
                sha256: "a".repeat(64),
            },
        };

        validate_ticket(
            &offer,
            "https://vol-ai.iywtu.com/artifact.zip",
            7,
            &"a".repeat(64),
            "",
        )
        .expect("unsigned ticket should rely on size and SHA-256 validation");
    }
}
