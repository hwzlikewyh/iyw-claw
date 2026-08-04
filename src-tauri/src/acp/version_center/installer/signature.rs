use base64::Engine;
use minisign_verify::{PublicKey, Signature};

use crate::app_error::AppCommandError;

// This is a build-time value, intentionally independent from the application
// updater and Agent artifact keys. Signed artifacts still fail closed when the
// release key is unavailable; unsigned artifacts rely on size and SHA-256.
const TOOLCHAIN_RELEASE_PUBLIC_KEY: Option<&str> = option_env!("IYW_TOOLCHAIN_RELEASE_PUBLIC_KEY");

pub fn verify_tool_signature(bytes: &[u8], signature_text: &str) -> Result<(), AppCommandError> {
    if signature_text.trim().is_empty() {
        tracing::warn!("[managed-install] unsigned artifact accepted with SHA-256 verification");
        return Ok(());
    }
    let public_key = parse_public_key(required_public_key()?)?;
    let signature = Signature::decode(signature_text.trim()).map_err(|error| {
        AppCommandError::invalid_input("Managed tool signature is invalid")
            .with_detail(error.to_string())
    })?;
    public_key.verify(bytes, &signature, true).map_err(|error| {
        AppCommandError::invalid_input("Managed tool signature verification failed")
            .with_detail(error.to_string())
    })
}

fn required_public_key() -> Result<&'static str, AppCommandError> {
    TOOLCHAIN_RELEASE_PUBLIC_KEY
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppCommandError::configuration_invalid(
                "Managed tool signing key is not compiled into this build",
            )
        })
}

fn parse_public_key(value: &str) -> Result<PublicKey, AppCommandError> {
    let value = unwrap_base64(value).unwrap_or_else(|_| value.to_string());
    let key_line = value
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("untrusted comment:"))
        .ok_or_else(|| {
            AppCommandError::configuration_invalid("Managed tool signing key is malformed")
        })?;
    PublicKey::from_base64(key_line).map_err(|error| {
        AppCommandError::configuration_invalid("Managed tool signing key is malformed")
            .with_detail(error.to_string())
    })
}

fn unwrap_base64(value: &str) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value.trim())
        .map_err(|error| error.to_string())?;
    String::from_utf8(bytes).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::verify_tool_signature;

    #[test]
    fn accepts_empty_signature() {
        verify_tool_signature(b"artifact", "")
            .expect("unsigned artifacts should rely on SHA-256 verification");
    }
}
