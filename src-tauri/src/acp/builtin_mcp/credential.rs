use std::fmt;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};

use super::session::SessionIssueError;

const TOKEN_BYTES: usize = 32;
const TOKEN_CHARS: usize = 43;
pub(super) type TokenDigest = [u8; 32];

pub struct SessionToken(String);

impl SessionToken {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SessionToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SessionToken([REDACTED])")
    }
}

pub(super) fn mint_token() -> Result<(SessionToken, TokenDigest), SessionIssueError> {
    let mut bytes = [0_u8; TOKEN_BYTES];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(SessionIssueError::Entropy)?;
    let token = SessionToken(URL_SAFE_NO_PAD.encode(bytes));
    let digest = hash_secret(token.as_str().as_bytes());
    Ok((token, digest))
}

pub(super) fn digest_token(token: &str) -> Option<TokenDigest> {
    if token.len() != TOKEN_CHARS {
        return None;
    }
    let decoded = URL_SAFE_NO_PAD.decode(token).ok()?;
    (decoded.len() == TOKEN_BYTES).then(|| hash_secret(token.as_bytes()))
}

fn hash_secret(secret: &[u8]) -> TokenDigest {
    Sha256::digest(secret).into()
}
