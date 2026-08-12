#[cfg(not(windows))]
use std::path::Path;
use std::sync::OnceLock;

use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};

const CHANNEL_KEY_ENV: &str = "IYW_CLAW_CHANNEL_TARGET_KEY";
const STORE_KEY_ENV: &str = "IYW_CLAW_SECRET_STORE_KEY";
const CHANNEL_PREFIX: &str = "v1:";
const STORE_PREFIX: &str = "s1:";
const NONCE_BYTES: usize = 12;
const MASTER_KEY_BYTES: usize = 32;

static STORE_MASTER_KEY: OnceLock<Result<[u8; MASTER_KEY_BYTES], String>> = OnceLock::new();
static LEGACY_CHANNEL_TOKENS: OnceLock<Vec<String>> = OnceLock::new();

pub fn initialize_legacy_channel_tokens(tokens: Vec<String>) {
    let _ = LEGACY_CHANNEL_TOKENS.set(tokens);
}

pub fn encrypt(key: &str, value: &str) -> Result<String, String> {
    encrypt_value(channel_master_key()?, CHANNEL_PREFIX, key, value)
}

pub fn decrypt(key: &str, value: &str) -> Result<String, String> {
    decrypt_value(channel_master_key()?, CHANNEL_PREFIX, key, value)
}

pub fn decrypt_legacy(key: &str, value: &str) -> Result<String, String> {
    let tokens = LEGACY_CHANNEL_TOKENS
        .get()
        .ok_or_else(|| "legacy channel target key unavailable".to_string())?;
    for token in tokens {
        let master_key = derive_key(b"iyw-claw:channel-target:v1\0", token);
        if let Ok(plaintext) = decrypt_value(master_key, CHANNEL_PREFIX, key, value) {
            return Ok(plaintext);
        }
    }
    Err("legacy channel target ciphertext invalid".to_string())
}

pub fn encrypt_store_value(key: &str, value: &str) -> Result<String, String> {
    encrypt_value(store_master_key()?, STORE_PREFIX, key, value)
}

pub fn decrypt_store_value(key: &str, value: &str) -> Result<String, String> {
    decrypt_value(store_master_key()?, STORE_PREFIX, key, value)
}

fn encrypt_value(
    master_key: [u8; MASTER_KEY_BYTES],
    prefix: &str,
    key: &str,
    value: &str,
) -> Result<String, String> {
    let cipher = cipher(master_key)?;
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
        .map_err(|_| "secret encryption failed".to_string())?;
    let mut envelope = nonce.to_vec();
    envelope.extend(ciphertext);
    Ok(format!(
        "{prefix}{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(envelope)
    ))
}

fn decrypt_value(
    master_key: [u8; MASTER_KEY_BYTES],
    prefix: &str,
    key: &str,
    value: &str,
) -> Result<String, String> {
    let encoded = value
        .strip_prefix(prefix)
        .ok_or_else(|| "secret ciphertext invalid".to_string())?;
    let envelope = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| "secret ciphertext invalid".to_string())?;
    let (nonce, ciphertext) = envelope
        .split_at_checked(NONCE_BYTES)
        .ok_or_else(|| "secret ciphertext invalid".to_string())?;
    let plaintext = cipher(master_key)?
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad: key.as_bytes(),
            },
        )
        .map_err(|_| "secret ciphertext invalid".to_string())?;
    String::from_utf8(plaintext).map_err(|_| "secret ciphertext invalid".to_string())
}

fn cipher(master_key: [u8; MASTER_KEY_BYTES]) -> Result<Aes256Gcm, String> {
    Aes256Gcm::new_from_slice(&master_key).map_err(|_| "secret encryption unavailable".to_string())
}

fn channel_master_key() -> Result<[u8; MASTER_KEY_BYTES], String> {
    if let Some(secret) = configured_secret(CHANNEL_KEY_ENV) {
        return Ok(derive_key(b"iyw-claw:channel-target:v1\0", &secret));
    }
    store_master_key()
}

fn store_master_key() -> Result<[u8; MASTER_KEY_BYTES], String> {
    STORE_MASTER_KEY.get_or_init(load_store_master_key).clone()
}

fn load_store_master_key() -> Result<[u8; MASTER_KEY_BYTES], String> {
    if let Some(secret) = configured_secret(STORE_KEY_ENV) {
        return Ok(derive_key(b"iyw-claw:secret-store:v1\0", &secret));
    }
    #[cfg(windows)]
    return Err(format!(
        "secret store encryption unavailable; set {STORE_KEY_ENV} on Windows"
    ));
    #[cfg(not(windows))]
    {
        let path = crate::server_secret_store::store_dir().join("secret-store.key");
        match std::fs::read_to_string(&path) {
            Ok(encoded) => {
                secure_file_permissions(&path)?;
                decode_key(encoded.trim())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => create_master_key(&path),
            Err(error) => Err(file_error(error)),
        }
    }
}

#[cfg(not(windows))]
fn create_master_key(path: &Path) -> Result<[u8; MASTER_KEY_BYTES], String> {
    let parent = path
        .parent()
        .ok_or_else(|| "secret store key has no parent directory".to_string())?;
    std::fs::create_dir_all(parent).map_err(file_error)?;
    let mut key = [0u8; MASTER_KEY_BYTES];
    rand::thread_rng().fill_bytes(&mut key);
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key);
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(file_error)?;
    std::io::Write::write_all(&mut temp, encoded.as_bytes()).map_err(file_error)?;
    temp.as_file().sync_all().map_err(file_error)?;
    secure_file_permissions(temp.path())?;
    match temp.persist_noclobber(path) {
        Ok(file) => {
            file.sync_all().map_err(file_error)?;
            Ok(key)
        }
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            decode_key(std::fs::read_to_string(path).map_err(file_error)?.trim())
        }
        Err(error) => Err(file_error(error.error)),
    }
}

#[cfg(not(windows))]
fn decode_key(encoded: &str) -> Result<[u8; MASTER_KEY_BYTES], String> {
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| "secret store key is invalid".to_string())?;
    decoded
        .try_into()
        .map_err(|_| "secret store key is invalid".to_string())
}

fn configured_secret(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn derive_key(domain: &[u8], secret: &str) -> [u8; MASTER_KEY_BYTES] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(secret.as_bytes());
    hasher.finalize().into()
}

#[cfg(unix)]
fn secure_file_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(file_error)
}

#[cfg(not(windows))]
fn file_error(error: impl std::fmt::Display) -> String {
    format!("secret store operation failed: {error}")
}
