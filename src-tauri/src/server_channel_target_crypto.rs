use std::sync::OnceLock;

use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};

const KEY_ENV: &str = "IYW_CLAW_CHANNEL_TARGET_KEY";
const CIPHER_PREFIX: &str = "v1:";
const NONCE_BYTES: usize = 12;

static SERVER_ACCESS_TOKEN: OnceLock<String> = OnceLock::new();

pub fn initialize(access_token: &str) {
    let _ = SERVER_ACCESS_TOKEN.set(access_token.to_string());
}

pub fn encrypt(key: &str, value: &str) -> Result<String, String> {
    let cipher = cipher()?;
    let mut nonce = [0u8; NONCE_BYTES];
    rand::thread_rng().fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: value.as_bytes(),
                aad: key.as_bytes(),
            },
        )
        .map_err(|_| "channel target encryption failed".to_string())?;
    let mut envelope = nonce.to_vec();
    envelope.extend(ciphertext);
    Ok(format!(
        "{CIPHER_PREFIX}{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(envelope)
    ))
}

pub fn decrypt(key: &str, value: &str) -> Result<String, String> {
    let encoded = value
        .strip_prefix(CIPHER_PREFIX)
        .ok_or_else(|| "channel target ciphertext invalid".to_string())?;
    let envelope = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| "channel target ciphertext invalid".to_string())?;
    let (nonce, ciphertext) = envelope
        .split_at_checked(NONCE_BYTES)
        .ok_or_else(|| "channel target ciphertext invalid".to_string())?;
    let plaintext = cipher()?
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad: key.as_bytes(),
            },
        )
        .map_err(|_| "channel target ciphertext invalid".to_string())?;
    String::from_utf8(plaintext).map_err(|_| "channel target ciphertext invalid".to_string())
}

fn cipher() -> Result<Aes256Gcm, String> {
    Aes256Gcm::new_from_slice(&master_key()?)
        .map_err(|_| "channel target encryption unavailable".to_string())
}

fn master_key() -> Result<[u8; 32], String> {
    let secret = configured_secret().ok_or_else(|| {
        "channel target master key unavailable; set IYW_CLAW_CHANNEL_TARGET_KEY".to_string()
    })?;
    let mut hasher = Sha256::new();
    hasher.update(b"iyw-claw:channel-target:v1\0");
    hasher.update(secret.as_bytes());
    Ok(hasher.finalize().into())
}

fn configured_secret() -> Option<String> {
    std::env::var(KEY_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| SERVER_ACCESS_TOKEN.get().cloned())
}
