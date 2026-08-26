use base64::Engine;
use minisign_verify::{PublicKey, Signature};

use crate::app_error::AppCommandError;

use super::types::SkillDownloadInfo;

const PLUGIN_RELEASE_PUBLIC_KEY: Option<&str> = option_env!("IYW_PLUGIN_RELEASE_PUBLIC_KEY");
const PLUGIN_RELEASE_PUBLIC_KEY_ID: Option<&str> = option_env!("IYW_PLUGIN_RELEASE_PUBLIC_KEY_ID");

pub(super) fn verify_v2_plugin_signature(
    bytes: &[u8],
    download: &SkillDownloadInfo,
) -> Result<(), AppCommandError> {
    let signature_text = download.signature.trim();
    let key_id = download.signature_key_id.trim();
    if signature_text.is_empty() || key_id.is_empty() {
        return Err(AppCommandError::configuration_invalid(
            "Plugin v2 artifact signature is required",
        ));
    }
    let expected_key_id = PLUGIN_RELEASE_PUBLIC_KEY_ID
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppCommandError::configuration_invalid(
                "Plugin release signing key ID is not compiled into this build",
            )
        })?;
    if key_id != expected_key_id {
        return Err(AppCommandError::invalid_input(
            "Plugin v2 artifact signing key is not trusted",
        ));
    }
    let public_key = parse_public_key(required_public_key()?)?;
    let signature = Signature::decode(signature_text).map_err(|error| {
        AppCommandError::invalid_input("Plugin v2 artifact signature is invalid")
            .with_detail(error.to_string())
    })?;
    public_key
        .verify(bytes, &signature, false)
        .map_err(|error| {
            AppCommandError::invalid_input("Plugin v2 artifact signature verification failed")
                .with_detail(error.to_string())
        })
}

fn required_public_key() -> Result<&'static str, AppCommandError> {
    PLUGIN_RELEASE_PUBLIC_KEY
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppCommandError::configuration_invalid(
                "Plugin release signing key is not compiled into this build",
            )
        })
}

fn parse_public_key(value: &str) -> Result<PublicKey, AppCommandError> {
    let value = unwrap_base64(value).unwrap_or_else(|| value.to_string());
    let key_line = value
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("untrusted comment:"))
        .ok_or_else(|| {
            AppCommandError::configuration_invalid("Plugin release signing key is malformed")
        })?;
    PublicKey::from_base64(key_line).map_err(|error| {
        AppCommandError::configuration_invalid("Plugin release signing key is malformed")
            .with_detail(error.to_string())
    })
}

fn unwrap_base64(value: &str) -> Option<String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value.trim())
        .ok()?;
    String::from_utf8(bytes).ok()
}
