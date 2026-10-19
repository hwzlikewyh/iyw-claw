//! 本地只认可当前登录会话的 Fusion token，不签发另一套本地凭据。
use std::sync::{OnceLock, RwLock};

use sha2::{Digest, Sha256};

use super::error::GatewayError;

struct Credential {
    digest: [u8; 32],
    expires_at_ms: i64,
}

fn credentials() -> &'static RwLock<Option<Credential>> {
    static CURRENT: OnceLock<RwLock<Option<Credential>>> = OnceLock::new();
    CURRENT.get_or_init(|| RwLock::new(None))
}

pub(crate) fn update_credentials(token: Option<&str>, expires_at_ms: Option<i64>, ready: bool) {
    let next = token
        .zip(expires_at_ms)
        .filter(|(token, _)| ready && !token.trim().is_empty())
        .map(|(token, expires_at_ms)| Credential {
            digest: Sha256::digest(token.trim().as_bytes()).into(),
            expires_at_ms,
        });
    match credentials().write() {
        Ok(mut current) => *current = next,
        Err(_) => {
            tracing::error!(target: "model_gateway", "local credential state unavailable; requests remain blocked")
        }
    }
}

pub(super) fn check(token: &str) -> Result<(), GatewayError> {
    if crate::commands::iyw_account::workspace_switching() {
        return Err(GatewayError::unauthorized());
    }
    let digest: [u8; 32] = Sha256::digest(token.as_bytes()).into();
    let current = credentials()
        .read()
        .map_err(|_| GatewayError::unauthorized())?;
    if current.as_ref().is_some_and(|credential| {
        credential.digest == digest
            && chrono::Utc::now().timestamp_millis() < credential.expires_at_ms
    }) {
        Ok(())
    } else {
        Err(GatewayError::unauthorized())
    }
}
