//! Centralized resolution of iyw-claw-owned filesystem paths.
//!
//! Mirrors the conventions already used by `preferences.rs` (`~/.iyw-claw/`)
//! and `experts.rs` (`~/.iyw-claw/skills/`). New features that need a
//! user-scoped persistent directory should call into this module instead of
//! re-deriving `dirs::home_dir().join(".iyw-claw")` themselves.

use std::path::{Path, PathBuf};

const APP_DIR_NAME: &str = ".iyw-claw";
const PETS_DIR_NAME: &str = "pets";
const UPLOADS_DIR_NAME: &str = "uploads";
const LOGS_DIR_NAME: &str = "logs";
const LOG_DIR_ENV: &str = "IYW_CLAW_LOG_DIR";

/// `$IYW_CLAW_HOME` if set (and non-empty), else `~/.iyw-claw/`.
///
/// Returns the relative `.iyw-claw` path when no home directory is available;
/// callers must still handle creation failures gracefully.
pub fn iyw_claw_home_dir() -> PathBuf {
    if let Some(custom) = std::env::var_os("IYW_CLAW_HOME").filter(|s| !s.is_empty()) {
        return PathBuf::from(custom);
    }
    dirs::home_dir()
        .map(|h| h.join(APP_DIR_NAME))
        .unwrap_or_else(|| PathBuf::from(APP_DIR_NAME))
}

/// Root directory for desktop-pet assets.
///
/// Resolution order:
/// 1. `$IYW_CLAW_HOME/pets` (explicit override, used in tests and custom installs)
/// 2. `$IYW_CLAW_DATA_DIR/pets` (server-mode data directory, populated by
///    `iyw-claw-server` from the corresponding env var)
/// 3. `~/.iyw-claw/pets` (default for the desktop app)
pub fn iyw_claw_pets_root() -> PathBuf {
    if let Some(custom) = std::env::var_os("IYW_CLAW_HOME").filter(|s| !s.is_empty()) {
        return PathBuf::from(custom).join(PETS_DIR_NAME);
    }
    if let Some(data) = std::env::var_os("IYW_CLAW_DATA_DIR").filter(|s| !s.is_empty()) {
        return PathBuf::from(data).join(PETS_DIR_NAME);
    }
    dirs::home_dir()
        .map(|h| h.join(APP_DIR_NAME).join(PETS_DIR_NAME))
        .unwrap_or_else(|| PathBuf::from(APP_DIR_NAME).join(PETS_DIR_NAME))
}

/// Root directory for attachments uploaded from the web client.
///
/// Resolution order matches `iyw_claw_pets_root()`:
/// 1. `$IYW_CLAW_HOME/uploads`
/// 2. `$IYW_CLAW_DATA_DIR/uploads` (server-mode data directory)
/// 3. `~/.iyw-claw/uploads` (desktop default)
///
/// Files in this directory are not garbage-collected by iyw-claw itself —
/// later conversations may still reference them via `file://` URIs
/// embedded in session history. To bound the long-term footprint on
/// shared / multi-tenant servers, operators can set
/// `IYW_CLAW_UPLOAD_MAX_TOTAL_BYTES` (see `web::handlers::files`): new
/// uploads beyond the cap are rejected at the API boundary while
/// existing files stay readable.
///
/// **Concurrency contract:** the quota check uses a process-local
/// in-flight reservation counter to make `IYW_CLAW_UPLOAD_MAX_TOTAL_BYTES`
/// a hard ceiling within one `iyw-claw-server` process. Multiple
/// `iyw-claw-server` processes sharing the same uploads root (e.g.
/// horizontally-scaled containers mounted on the same volume) will
/// each enforce the cap independently and can collectively exceed it.
/// iyw-claw is designed for single-process deployments; horizontal
/// scaling would require external coordination (file lock, Redis,
/// reverse-proxy quota) that this codebase does not provide.
pub fn iyw_claw_uploads_root() -> PathBuf {
    if let Some(custom) = std::env::var_os("IYW_CLAW_HOME").filter(|s| !s.is_empty()) {
        return PathBuf::from(custom).join(UPLOADS_DIR_NAME);
    }
    if let Some(data) = std::env::var_os("IYW_CLAW_DATA_DIR").filter(|s| !s.is_empty()) {
        return PathBuf::from(data).join(UPLOADS_DIR_NAME);
    }
    dirs::home_dir()
        .map(|h| h.join(APP_DIR_NAME).join(UPLOADS_DIR_NAME))
        .unwrap_or_else(|| PathBuf::from(APP_DIR_NAME).join(UPLOADS_DIR_NAME))
}

/// Root directory for application diagnostic logs (rotating files written by
/// the `tracing` file appender; see `crate::logging`).
///
/// Resolution keeps installed desktop logs beside app/runtime/config/data while
/// preserving the existing server layout:
/// 1. `$IYW_CLAW_LOG_DIR` (installed desktop override)
/// 2. `$IYW_CLAW_HOME/logs`
/// 3. `$IYW_CLAW_DATA_DIR/logs` (server-mode data directory)
/// 4. `~/.iyw-claw/logs` (fallback)
///
/// Pure env + `dirs::home_dir()`, so it is callable at the very start of a
/// process — before the database (or, in `iyw-claw-server`, the tokio runtime)
/// exists — which is exactly when the subscriber must be installed.
pub fn iyw_claw_logs_root() -> PathBuf {
    if let Some(custom) = std::env::var_os(LOG_DIR_ENV).filter(|s| !s.is_empty()) {
        return PathBuf::from(custom);
    }
    if let Some(custom) = std::env::var_os("IYW_CLAW_HOME").filter(|s| !s.is_empty()) {
        return PathBuf::from(custom).join(LOGS_DIR_NAME);
    }
    if let Some(data) = std::env::var_os("IYW_CLAW_DATA_DIR").filter(|s| !s.is_empty()) {
        return PathBuf::from(data).join(LOGS_DIR_NAME);
    }
    dirs::home_dir()
        .map(|h| h.join(APP_DIR_NAME).join(LOGS_DIR_NAME))
        .unwrap_or_else(|| PathBuf::from(APP_DIR_NAME).join(LOGS_DIR_NAME))
}

/// Single source of truth for "where does the database live, and where
/// do `paths::*` resolve their roots against."
///
/// Resolution:
/// 1. If `IYW_CLAW_DATA_DIR` is set and non-empty, return its absolutized
///    form. Honors the operator's choice even on desktop, where a
///    pre-set env var should override Tauri's identifier-derived path.
/// 2. Otherwise return the absolutized form of `tauri_fallback` —
///    typically `app.path().app_data_dir()` on desktop or the
///    server's default data dir.
///
/// Always returns an absolute path (`absolutize` re-bases against the
/// process CWD if needed). Callers should treat the result as
/// authoritative and not re-read `IYW_CLAW_DATA_DIR` themselves; the
/// startup code in `lib.rs` / `bin/iyw_claw_server.rs` writes the
/// resolved value back to the env so subprocess inheritance works,
/// but the in-process source of truth is this function.
///
/// This exists because Tauri's `app.path().app_data_dir()` does **not**
/// consult `IYW_CLAW_DATA_DIR` — it returns the identifier-derived path
/// unconditionally. Call sites that pass `app_data_dir()` straight
/// into git credential helpers, ACP, terminal sessions, etc. would
/// otherwise generate scripts pointing at an empty DB when the
/// operator pre-set `IYW_CLAW_DATA_DIR` to a custom location.
pub fn resolve_effective_data_dir(tauri_fallback: &Path) -> PathBuf {
    if let Some(custom) = std::env::var_os("IYW_CLAW_DATA_DIR").filter(|s| !s.is_empty()) {
        return crate::git_credential::absolutize(Path::new(&custom));
    }
    crate::git_credential::absolutize(tauri_fallback)
}

// Path resolution depends on global env vars (`IYW_CLAW_HOME`, `IYW_CLAW_DATA_DIR`),
// so unit tests would need cross-test serialization to avoid races. The
// behaviour is covered end-to-end by `pets::*` tests which set `IYW_CLAW_HOME`
// inside a serialized test mutex; we deliberately don't duplicate that here.
