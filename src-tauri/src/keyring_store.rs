#[cfg(feature = "tauri-runtime")]
const SERVICE_NAME: &str = "iyw-claw";

fn token_key(account_id: &str) -> String {
    format!("github-token:{}", account_id)
}

fn channel_token_key(channel_id: i32) -> String {
    format!("chat-channel:{}", channel_id)
}

fn chat_router_token_key() -> &'static str {
    "chat-natural-router"
}

fn channel_target_key(target_id: &str) -> String {
    format!("chat-channel-target:{target_id}")
}

fn channel_target_secret_key() -> &'static str {
    "chat-channel-target-secret"
}

#[cfg(not(feature = "tauri-runtime"))]
pub fn initialize_legacy_server_channel_target_crypto(tokens: Vec<String>) {
    crate::server_channel_target_crypto::initialize_legacy_channel_tokens(tokens);
}

// ── Tauri mode: OS keyring ──

#[cfg(feature = "tauri-runtime")]
pub fn set_token(account_id: &str, token: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, &token_key(account_id))
        .map_err(|e| format!("keyring init error: {e}"))?;
    entry
        .set_password(token)
        .map_err(|e| format!("keyring set error: {e}"))
}

#[cfg(feature = "tauri-runtime")]
pub fn get_token(account_id: &str) -> Option<String> {
    let entry = keyring::Entry::new(SERVICE_NAME, &token_key(account_id)).ok()?;
    entry.get_password().ok()
}

#[cfg(feature = "tauri-runtime")]
pub fn delete_token(account_id: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, &token_key(account_id))
        .map_err(|e| format!("keyring init error: {e}"))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("keyring delete error: {e}")),
    }
}

#[cfg(not(feature = "tauri-runtime"))]
pub fn set_token(account_id: &str, token: &str) -> Result<(), String> {
    crate::server_secret_store::set(&token_key(account_id), token)
}

#[cfg(not(feature = "tauri-runtime"))]
pub fn get_token(account_id: &str) -> Option<String> {
    crate::server_secret_store::get(&token_key(account_id))
}

#[cfg(not(feature = "tauri-runtime"))]
pub fn delete_token(account_id: &str) -> Result<(), String> {
    crate::server_secret_store::delete(&token_key(account_id))
}

// ── Chat channel token helpers ──
// Reuse the same storage mechanism (keyring or file) with a different key prefix.

#[cfg(feature = "tauri-runtime")]
pub fn set_channel_token(channel_id: i32, token: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, &channel_token_key(channel_id))
        .map_err(|e| format!("keyring init error: {e}"))?;
    entry
        .set_password(token)
        .map_err(|e| format!("keyring set error: {e}"))
}

#[cfg(feature = "tauri-runtime")]
pub fn get_channel_token(channel_id: i32) -> Option<String> {
    try_get_channel_token(channel_id).ok().flatten()
}

#[cfg(feature = "tauri-runtime")]
pub fn try_get_channel_token(channel_id: i32) -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(SERVICE_NAME, &channel_token_key(channel_id))
        .map_err(|e| format!("keyring init error: {e}"))?;
    match entry.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("keyring read error: {e}")),
    }
}

#[cfg(feature = "tauri-runtime")]
pub fn delete_channel_token(channel_id: i32) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, &channel_token_key(channel_id))
        .map_err(|e| format!("keyring init error: {e}"))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("keyring delete error: {e}")),
    }
}

#[cfg(not(feature = "tauri-runtime"))]
pub fn set_channel_token(channel_id: i32, token: &str) -> Result<(), String> {
    crate::server_secret_store::set(&channel_token_key(channel_id), token)
}

#[cfg(not(feature = "tauri-runtime"))]
pub fn get_channel_token(channel_id: i32) -> Option<String> {
    try_get_channel_token(channel_id).ok().flatten()
}

#[cfg(not(feature = "tauri-runtime"))]
pub fn try_get_channel_token(channel_id: i32) -> Result<Option<String>, String> {
    crate::server_secret_store::try_get(&channel_token_key(channel_id))
}

#[cfg(not(feature = "tauri-runtime"))]
pub fn delete_channel_token(channel_id: i32) -> Result<(), String> {
    crate::server_secret_store::delete(&channel_token_key(channel_id))
}

// ── Chat natural router token helpers ──
// One global OpenAI-compatible API key used by the channel-agnostic router.

#[cfg(feature = "tauri-runtime")]
pub fn set_chat_router_token(token: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, chat_router_token_key())
        .map_err(|e| format!("keyring init error: {e}"))?;
    entry
        .set_password(token)
        .map_err(|e| format!("keyring set error: {e}"))
}

#[cfg(feature = "tauri-runtime")]
pub fn get_chat_router_token() -> Option<String> {
    let entry = keyring::Entry::new(SERVICE_NAME, chat_router_token_key()).ok()?;
    entry.get_password().ok()
}

#[cfg(feature = "tauri-runtime")]
pub fn delete_chat_router_token() -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, chat_router_token_key())
        .map_err(|e| format!("keyring init error: {e}"))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("keyring delete error: {e}")),
    }
}

// ── Chat channel target helpers ──

#[cfg(feature = "tauri-runtime")]
fn set_secure_value(key: &str, value: &str) -> Result<(), String> {
    let entry =
        keyring::Entry::new(SERVICE_NAME, key).map_err(|e| format!("keyring init error: {e}"))?;
    entry
        .set_password(value)
        .map_err(|e| format!("keyring set error: {e}"))
}

#[cfg(feature = "tauri-runtime")]
fn get_secure_value(key: &str) -> Option<String> {
    keyring::Entry::new(SERVICE_NAME, key)
        .ok()?
        .get_password()
        .ok()
}

#[cfg(feature = "tauri-runtime")]
fn delete_secure_value(key: &str) -> Result<(), String> {
    let entry =
        keyring::Entry::new(SERVICE_NAME, key).map_err(|e| format!("keyring init error: {e}"))?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("keyring delete error: {e}")),
    }
}

#[cfg(not(feature = "tauri-runtime"))]
fn set_secure_value(key: &str, value: &str) -> Result<(), String> {
    let encrypted = crate::server_channel_target_crypto::encrypt(key, value)?;
    crate::server_secret_store::set(key, &encrypted)
}

#[cfg(not(feature = "tauri-runtime"))]
fn get_secure_value(key: &str) -> Option<String> {
    let encrypted = crate::server_secret_store::get(key)?;
    match crate::server_channel_target_crypto::decrypt(key, &encrypted) {
        Ok(value) => Some(value),
        Err(_) => {
            let value =
                crate::server_channel_target_crypto::decrypt_legacy(key, &encrypted).ok()?;
            if let Err(error) = set_secure_value(key, &value) {
                tracing::warn!(error = %error, "legacy channel target re-encryption failed");
            }
            Some(value)
        }
    }
}

#[cfg(not(feature = "tauri-runtime"))]
fn delete_secure_value(key: &str) -> Result<(), String> {
    crate::server_secret_store::delete(key)
}

pub fn set_channel_target(target_id: &str, payload: &str) -> Result<(), String> {
    set_secure_value(&channel_target_key(target_id), payload)
}

pub fn get_channel_target(target_id: &str) -> Option<String> {
    get_secure_value(&channel_target_key(target_id))
}

pub fn delete_channel_target(target_id: &str) -> Result<(), String> {
    delete_secure_value(&channel_target_key(target_id))
}

pub fn get_or_create_channel_target_secret() -> Result<String, String> {
    static CREATE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = CREATE_LOCK
        .lock()
        .map_err(|_| "channel target secret lock unavailable".to_string())?;
    if let Some(secret) = get_secure_value(channel_target_secret_key()) {
        return Ok(secret);
    }
    #[cfg(not(feature = "tauri-runtime"))]
    if crate::server_secret_store::get(channel_target_secret_key()).is_some() {
        return Err("channel target secret unavailable".to_string());
    }
    let secret = format!("{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
    #[cfg(feature = "tauri-runtime")]
    {
        set_secure_value(channel_target_secret_key(), &secret)?;
        Ok(secret)
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let key = channel_target_secret_key();
        let encrypted = crate::server_channel_target_crypto::encrypt(key, &secret)?;
        let stored = crate::server_secret_store::get_or_insert(key, &encrypted)?;
        crate::server_channel_target_crypto::decrypt(key, &stored)
    }
}

#[cfg(not(feature = "tauri-runtime"))]
pub fn set_chat_router_token(token: &str) -> Result<(), String> {
    crate::server_secret_store::set(chat_router_token_key(), token)
}

#[cfg(not(feature = "tauri-runtime"))]
pub fn get_chat_router_token() -> Option<String> {
    crate::server_secret_store::get(chat_router_token_key())
}

#[cfg(not(feature = "tauri-runtime"))]
pub fn delete_chat_router_token() -> Result<(), String> {
    crate::server_secret_store::delete(chat_router_token_key())
}
