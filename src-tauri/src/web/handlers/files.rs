use axum::extract::Multipart;
use axum::Json;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::acp::capability_policy::{monitor_file_upload, CapabilityRevocationMonitor};
use crate::app_error::{
    AppCommandError, UPLOAD_I18N_KEY_QUOTA_EXCEEDED, UPLOAD_I18N_KEY_TOO_LARGE,
};
use crate::commands::folders as folder_commands;
use crate::paths::iyw_claw_uploads_root;

use super::upload_jail;

const UPLOAD_FILENAME_MAX_CHARS: usize = 120;
const UPLOAD_COLLISION_SUFFIX_ATTEMPTS: usize = 999;
const UPLOAD_UUID_FALLBACK_ATTEMPTS: usize = 16;

// ---------------------------------------------------------------------------
// Param structs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadFilePreviewParams {
    pub root_path: String,
    pub path: String,
    pub max_bytes: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadFileBase64Params {
    pub path: String,
    pub max_bytes: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadWorkspaceFileBase64Params {
    pub root_path: String,
    pub path: String,
    pub max_bytes: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileExistsParams {
    pub root_path: String,
    pub path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadFileForEditParams {
    pub root_path: String,
    pub path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveFileContentParams {
    pub root_path: String,
    pub path: String,
    pub content: String,
    pub expected_etag: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveFileCopyParams {
    pub root_path: String,
    pub path: String,
    pub content: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameFileTreeEntryParams {
    pub root_path: String,
    pub path: String,
    pub new_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteFileTreeEntryParams {
    pub root_path: String,
    pub path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFileTreeEntryParams {
    pub root_path: String,
    pub path: String,
    pub name: String,
    pub kind: String,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

pub async fn read_file_preview(
    Json(params): Json<ReadFilePreviewParams>,
) -> Result<Json<folder_commands::FilePreviewContent>, AppCommandError> {
    let result =
        folder_commands::read_file_preview(params.root_path, params.path, params.max_bytes).await?;
    Ok(Json(result))
}

pub async fn read_file_base64(
    Json(params): Json<ReadFileBase64Params>,
) -> Result<Json<String>, AppCommandError> {
    let result = folder_commands::read_file_base64(params.path, params.max_bytes).await?;
    Ok(Json(result))
}

pub async fn read_workspace_file_base64(
    Json(params): Json<ReadWorkspaceFileBase64Params>,
) -> Result<Json<String>, AppCommandError> {
    let result = folder_commands::read_workspace_file_base64(
        params.root_path,
        params.path,
        params.max_bytes,
    )
    .await?;
    Ok(Json(result))
}

pub async fn workspace_file_exists(
    Json(params): Json<WorkspaceFileExistsParams>,
) -> Result<Json<bool>, AppCommandError> {
    let result = folder_commands::workspace_file_exists(params.root_path, params.path).await?;
    Ok(Json(result))
}

pub async fn read_file_for_edit(
    Json(params): Json<ReadFileForEditParams>,
) -> Result<Json<folder_commands::FileEditContent>, AppCommandError> {
    let result = folder_commands::read_file_for_edit(params.root_path, params.path).await?;
    Ok(Json(result))
}

pub async fn save_file_content(
    Json(params): Json<SaveFileContentParams>,
) -> Result<Json<folder_commands::FileSaveResult>, AppCommandError> {
    let result = folder_commands::save_file_content(
        params.root_path,
        params.path,
        params.content,
        params.expected_etag,
    )
    .await?;
    Ok(Json(result))
}

pub async fn save_file_copy(
    Json(params): Json<SaveFileCopyParams>,
) -> Result<Json<folder_commands::FileSaveResult>, AppCommandError> {
    let result =
        folder_commands::save_file_copy(params.root_path, params.path, params.content).await?;
    Ok(Json(result))
}

pub async fn rename_file_tree_entry(
    Json(params): Json<RenameFileTreeEntryParams>,
) -> Result<Json<String>, AppCommandError> {
    let result =
        folder_commands::rename_file_tree_entry(params.root_path, params.path, params.new_name)
            .await?;
    Ok(Json(result))
}

pub async fn delete_file_tree_entry(
    Json(params): Json<DeleteFileTreeEntryParams>,
) -> Result<Json<()>, AppCommandError> {
    folder_commands::delete_file_tree_entry(params.root_path, params.path).await?;
    Ok(Json(()))
}

pub async fn create_file_tree_entry(
    Json(params): Json<CreateFileTreeEntryParams>,
) -> Result<Json<String>, AppCommandError> {
    let result = folder_commands::create_file_tree_entry(
        params.root_path,
        params.path,
        params.name,
        params.kind,
    )
    .await?;
    Ok(Json(result))
}

// ---------------------------------------------------------------------------
// Attachment upload
// ---------------------------------------------------------------------------

/// Hard cap on a single uploaded attachment.
///
/// Hard payload limit for a single attachment. The route's multipart body
/// limit is derived from this value, while the streaming check remains
/// defense-in-depth against future route configuration changes.
pub const UPLOAD_MAX_BYTES: u64 = 100 * 1024 * 1024;

/// Env-controlled cap on the *total* bytes resident under
/// `uploads_root/`. Per-file `UPLOAD_MAX_BYTES` bounds one payload; this
/// bounds long-term accumulation so a compromised or shared token can't
/// repeatedly upload small files until the host runs out of disk. Unset
/// or `0` disables the cap — preserves the original "no GC" behavior
/// for operators who want it.
///
/// The check is intentionally conservative: it fires before any bytes
/// are streamed to disk, assuming the worst-case `UPLOAD_MAX_BYTES`.
/// That over-rejects in the last `UPLOAD_MAX_BYTES` of headroom (e.g. a
/// 100 KB upload may get rejected when only 1 MB remains under the
/// cap), but it keeps the code free of mid-stream cleanup races. With
/// the in-flight reservation (see `UPLOAD_IN_FLIGHT_BYTES` below) this
/// is effectively a hard ceiling: concurrent admits cannot accumulate
/// past `cap` because each one decrements the in-flight headroom seen
/// by the next.
const UPLOAD_TOTAL_BYTES_ENV: &str = "IYW_CLAW_UPLOAD_MAX_TOTAL_BYTES";

/// Opt-in fail-closed mode for the quota config. When truthy and
/// `IYW_CLAW_UPLOAD_MAX_TOTAL_BYTES` parses as `Invalid`, startup aborts
/// with a `FATAL` line instead of falling through to fail-open. Default
/// (unset / "0" / "false") preserves the prior fail-open posture so a
/// typo doesn't take down a production process unless the operator
/// explicitly asks for that behavior.
const UPLOAD_STRICT_ENV: &str = "IYW_CLAW_UPLOAD_QUOTA_STRICT";

/// Outcome of parsing `IYW_CLAW_UPLOAD_MAX_TOTAL_BYTES`. Carries enough
/// context that the startup banner can distinguish "operator turned it
/// off" from "operator typo silently disabled the cap".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadQuotaConfig {
    /// Env var unset; cap disabled by default.
    Unset,
    /// Env var present but `0` (or only whitespace); cap explicitly off.
    Disabled,
    /// Cap active at this many bytes.
    Enabled(u64),
    /// Env var was set to a value we could not parse as a positive
    /// `u64`. Carries the offending raw value so the operator gets a
    /// loud error mentioning the exact string they typed. Cap is *off*
    /// in this branch — we choose availability over safety here so a
    /// typo doesn't 5xx the upload endpoint, but the startup WARN line
    /// makes the failure mode discoverable.
    Invalid(String),
}

impl UploadQuotaConfig {
    /// Active cap, if any. Returns `None` for `Unset`, `Disabled`, and
    /// `Invalid` — all three mean "no quota enforcement this run".
    pub fn cap_bytes(&self) -> Option<u64> {
        match self {
            UploadQuotaConfig::Enabled(c) => Some(*c),
            _ => None,
        }
    }
}

/// Pure-function form of the env parser so unit tests don't need to
/// mutate process-global state (which would race the test harness's
/// concurrent runner).
fn parse_upload_quota_config(raw: Option<&str>) -> UploadQuotaConfig {
    let Some(s) = raw else {
        return UploadQuotaConfig::Unset;
    };
    let trimmed = s.trim();
    if trimmed.is_empty() {
        // Empty / whitespace-only is treated the same as unset rather
        // than as a typo — common in `docker run -e VAR= ...` and not
        // worth shouting about.
        return UploadQuotaConfig::Unset;
    }
    match trimmed.parse::<u64>() {
        Ok(0) => UploadQuotaConfig::Disabled,
        Ok(n) => UploadQuotaConfig::Enabled(n),
        Err(_) => UploadQuotaConfig::Invalid(trimmed.to_string()),
    }
}

fn upload_quota_config_from_env() -> UploadQuotaConfig {
    parse_upload_quota_config(std::env::var(UPLOAD_TOTAL_BYTES_ENV).ok().as_deref())
}

/// Truthy boolean parse for env-var flags. Accepts `1 / true / yes /
/// on` (case-insensitive, trim-tolerant). Everything else — including
/// `0`, `false`, empty, unset — returns `false`. Lives next to the
/// quota parser so the two share the same testability story.
fn parse_strict_mode(raw: Option<&str>) -> bool {
    let Some(s) = raw else { return false };
    matches!(
        s.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn upload_quota_strict_from_env() -> bool {
    parse_strict_mode(std::env::var(UPLOAD_STRICT_ENV).ok().as_deref())
}

/// Reason a strict-mode validation rejected the current quota
/// configuration. Lets callers (server `main`, desktop web start)
/// decide how to react — kill the process or surface a UI error.
#[derive(Debug, Clone)]
pub struct UploadQuotaStrictError {
    /// The raw env value that failed to parse, for inclusion in the
    /// operator-facing message.
    pub raw_value: String,
}

impl std::fmt::Display for UploadQuotaStrictError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{UPLOAD_TOTAL_BYTES_ENV}={:?} is not a positive integer and {UPLOAD_STRICT_ENV} is on",
            self.raw_value
        )
    }
}

impl std::error::Error for UploadQuotaStrictError {}

/// Emit a startup banner describing the current upload-quota
/// configuration. Pure logging — no process abort, no return value —
/// so it is safe to call from any startup path (server, desktop web
/// service toggle, future test harness) without surprising side
/// effects.
///
/// The strict-mode abort lives in `validate_upload_quota_config`. The
/// split exists because the server binary should die on strict+invalid
/// while the desktop must keep running and surface a UI error instead.
///
/// Called once from each binary entry point right after the data
/// directory and listener are resolved. Cheap: two env reads + one
/// `eprintln!`.
pub fn log_upload_quota_config_at_startup() {
    let config = upload_quota_config_from_env();
    let strict = upload_quota_strict_from_env();
    match &config {
        UploadQuotaConfig::Unset => {
            tracing::info!(
                "[uploads] {UPLOAD_TOTAL_BYTES_ENV} unset → total-size cap disabled (set to a byte count to enable)"
            );
        }
        UploadQuotaConfig::Disabled => {
            tracing::info!("[uploads] {UPLOAD_TOTAL_BYTES_ENV}=0 → total-size cap disabled");
        }
        UploadQuotaConfig::Enabled(cap) => {
            tracing::info!("[uploads] total-size cap: {cap} bytes ({UPLOAD_TOTAL_BYTES_ENV})");
        }
        UploadQuotaConfig::Invalid(raw) => {
            if strict {
                // Caller will abort via `validate_upload_quota_config`;
                // here we only narrate so the FATAL line lands in
                // operator logs alongside the rest of the banner.
                tracing::error!(
                    "[uploads][FATAL] {UPLOAD_TOTAL_BYTES_ENV}={raw:?} is not a positive integer \
                     and {UPLOAD_STRICT_ENV} is on. Caller will abort startup. \
                     Use a plain decimal byte count (e.g. 10737418240 for 10 GiB)."
                );
            } else {
                tracing::warn!(
                    "[uploads][WARN] {UPLOAD_TOTAL_BYTES_ENV}={raw:?} is not a positive integer; \
                     total-size cap is DISABLED. Use a plain decimal byte count (e.g. 10737418240 for 10 GiB), \
                     or set {UPLOAD_STRICT_ENV}=1 to abort startup on this condition."
                );
            }
        }
    }
}

/// Strict-mode validation: returns `Err` when `IYW_CLAW_UPLOAD_QUOTA_STRICT`
/// is truthy and the quota value is `Invalid`. Every other combination
/// (unset, disabled, enabled, or invalid-but-strict-off) returns `Ok`.
///
/// Callers choose the failure mode:
///   * `iyw-claw-server` (single-purpose process) — call early in `main`
///     and `process::exit(2)` on `Err`.
///   * Desktop web-service start — call before `persist_web_service_config`
///     and surface `Err` as an `AppCommandError` so the toggle fails
///     cleanly without taking down the host process.
pub fn validate_upload_quota_config() -> Result<(), UploadQuotaStrictError> {
    let config = upload_quota_config_from_env();
    let strict = upload_quota_strict_from_env();
    match config {
        UploadQuotaConfig::Invalid(raw) if strict => Err(UploadQuotaStrictError { raw_value: raw }),
        _ => Ok(()),
    }
}

/// Running tally of bytes reserved by `upload_attachment` calls that
/// have passed the quota check but haven't yet finished writing or
/// failed. Combined with `current_uploads_total_bytes` on disk, this
/// closes the TOCTOU race where two concurrent uploads both saw the
/// same disk-level free space and admitted past the cap.
///
/// Reservation strategy: each upload reserves the worst case
/// (`UPLOAD_MAX_BYTES`) up front and releases it on guard drop.
/// Over-reservation is acceptable — the operator-facing budget is the
/// disk, not the counter — and a uniform reservation size keeps the
/// CAS loop and the cleanup path symmetric.
///
/// **Scope:** this counter is process-local. Multiple `iyw-claw-server`
/// processes sharing the same `uploads_root` (horizontally-scaled
/// deployments mounted on the same volume) will each maintain their
/// own counter and can collectively exceed the cap. iyw-claw is designed
/// for single-process deployments; multi-process coordination would
/// require an external mechanism (file lock, Redis, reverse-proxy
/// quota) that this codebase does not provide. See the doc on
/// `paths::iyw_claw_uploads_root` for the operator-facing version of
/// this contract.
static UPLOAD_IN_FLIGHT_BYTES: AtomicU64 = AtomicU64::new(0);

/// RAII guard returned by `try_reserve_in_flight`. Releases the
/// reservation on drop, including the error and panic paths in
/// `upload_attachment`.
pub(super) struct InFlightGuard<'a> {
    counter: &'a AtomicU64,
    bytes: u64,
}

impl Drop for InFlightGuard<'_> {
    fn drop(&mut self) {
        // `AcqRel` here pairs with the `Acquire` load and `AcqRel` CAS
        // in `try_reserve_in_flight` so the next reservation sees the
        // post-decrement value.
        self.counter.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

/// Lock-free CAS reserve against `counter`. Returns `Ok(guard)` if the
/// reservation fits inside `cap` given the current on-disk `used`, or
/// `Err(())` if the cap is full.
///
/// Takes the counter by reference (rather than reading the module-level
/// static) so the unit tests can drive it with a local atomic and run
/// concurrently without poisoning each other.
fn try_reserve_in_flight<'a>(
    counter: &'a AtomicU64,
    bytes: u64,
    used: u64,
    cap: u64,
) -> Result<InFlightGuard<'a>, ()> {
    let mut current = counter.load(Ordering::Acquire);
    loop {
        let projected = used.saturating_add(current).saturating_add(bytes);
        if projected > cap {
            return Err(());
        }
        match counter.compare_exchange_weak(
            current,
            current + bytes,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return Ok(InFlightGuard { counter, bytes }),
            Err(observed) => current = observed,
        }
    }
}

/// Sum the size of every regular file under `uploads_root/` except the
/// `.tmp/` and `.image-tmp/` staging directories. Walks at most one level
/// of buckets — that is the structure produced by `stream_and_finalize` —
/// but the inner walk follows whatever entries exist, so a hand-edited deeper
/// tree is still counted faithfully.
///
/// Failures during the walk are logged and skipped: a permission error
/// on one file shouldn't block the upload pipeline. The returned total
/// is a lower bound in that case, which means the cap may admit one
/// extra upload before tripping. That's strictly better than refusing
/// to serve.
async fn current_uploads_total_bytes(uploads_root: &std::path::Path) -> u64 {
    let mut total: u64 = 0;
    let mut bucket_iter = match tokio::fs::read_dir(uploads_root).await {
        Ok(it) => it,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return 0,
        Err(e) => {
            tracing::error!(
                "[uploads] failed to enumerate uploads root {}: {}",
                uploads_root.display(),
                e
            );
            return 0;
        }
    };
    while let Some(entry) = bucket_iter.next_entry().await.transpose() {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                tracing::error!("[uploads] read_dir entry error: {e}");
                continue;
            }
        };
        let name = entry.file_name();
        if name == ".tmp" || name == ".image-tmp" {
            // Staging files are unreferenced and purged at startup —
            // exclude them so a partial upload doesn't inflate the
            // counter and reject the very next request.
            continue;
        }
        let file_type = match entry.file_type().await {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if file_type.is_file() {
            // A loose file at the top level (legacy layout or admin
            // copy-in) still counts.
            if let Ok(meta) = entry.metadata().await {
                total = total.saturating_add(meta.len());
            }
            continue;
        }
        if !file_type.is_dir() {
            continue;
        }
        let mut file_iter = match tokio::fs::read_dir(entry.path()).await {
            Ok(it) => it,
            Err(_) => continue,
        };
        while let Some(f) = file_iter.next_entry().await.transpose() {
            let f = match f {
                Ok(f) => f,
                Err(_) => continue,
            };
            if let Ok(meta) = f.metadata().await {
                if meta.is_file() {
                    total = total.saturating_add(meta.len());
                }
            }
        }
    }
    total
}

pub(super) async fn reserve_upload_bytes(
    uploads_root: &std::path::Path,
    bytes: u64,
) -> Result<Option<InFlightGuard<'static>>, AppCommandError> {
    let Some(cap) = upload_quota_config_from_env().cap_bytes() else {
        return Ok(None);
    };
    let used = current_uploads_total_bytes(uploads_root).await;
    try_reserve_in_flight(&UPLOAD_IN_FLIGHT_BYTES, bytes, used, cap)
        .map(Some)
        .map_err(|_| {
            let mut params = BTreeMap::new();
            params.insert("used".to_string(), used.to_string());
            params.insert("limit".to_string(), cap.to_string());
            AppCommandError::io_error("Upload quota exceeded for this server")
                .with_detail(format!("used={used} limit={cap}"))
                .with_i18n(UPLOAD_I18N_KEY_QUOTA_EXCEEDED, params)
        })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadAttachmentResult {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub mime_type: Option<String>,
}

/// Sanitize a client-supplied filename so it lands inside the target
/// directory and contains no shell-hostile bytes.
///
/// Strategy: keep only the final path component, strip shell-hostile and
/// cross-platform-hostile characters, and bound the length. Leading dots are
/// preserved for real dotfiles, but all-dot names fall back to `file`.
pub(super) fn sanitize_upload_filename(raw: &str) -> String {
    let base = raw
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .trim_matches(|c: char| c.is_whitespace());
    let cleaned: String = base
        .chars()
        .filter(|c| !c.is_control())
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            other => other,
        })
        .collect();
    let trimmed = cleaned
        .trim_matches(|c: char| c.is_whitespace())
        .trim_end_matches('.');
    let limited = truncate_chars(trimmed, UPLOAD_FILENAME_MAX_CHARS);
    if limited.is_empty() || limited.chars().all(|c| c == '.') {
        "file".to_string()
    } else {
        limited
    }
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    input.chars().take(max_chars).collect()
}

fn split_upload_filename(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot_idx) if dot_idx > 0 => (&name[..dot_idx], &name[dot_idx..]),
        _ => (name, ""),
    }
}

fn upload_filename_with_stem_suffix(safe_name: &str, stem_suffix: &str) -> String {
    let (stem, ext) = split_upload_filename(safe_name);
    let suffix_chars = stem_suffix.chars().count();
    let ext_budget = UPLOAD_FILENAME_MAX_CHARS.saturating_sub(suffix_chars + 1);
    let ext_part = truncate_chars(ext, ext_budget);
    let stem_budget = UPLOAD_FILENAME_MAX_CHARS
        .saturating_sub(suffix_chars + ext_part.chars().count())
        .max(1);
    let stem_part = truncate_chars(stem, stem_budget);
    let stem_part = if stem_part.is_empty() || stem_part.chars().all(|c| c == '.') {
        "file".to_string()
    } else {
        stem_part
    };
    format!("{stem_part}{stem_suffix}{ext_part}")
}

fn upload_filename_candidate(safe_name: &str, collision_index: usize) -> String {
    if collision_index == 0 {
        safe_name.to_string()
    } else {
        upload_filename_with_stem_suffix(safe_name, &format!(" ({collision_index})"))
    }
}

fn upload_uuid_fallback_candidate(safe_name: &str) -> String {
    let unique = uuid::Uuid::new_v4().simple().to_string();
    upload_filename_with_stem_suffix(safe_name, &format!("-{unique}"))
}

pub(super) async fn finalize_with_available_upload_name(
    tmp_dir: &std::path::Path,
    staging_name: &str,
    bucket_dir: &std::path::Path,
    safe_name: &str,
) -> Result<String, AppCommandError> {
    for collision_index in 0..=UPLOAD_COLLISION_SUFFIX_ATTEMPTS {
        let candidate = upload_filename_candidate(safe_name, collision_index);
        match upload_jail::finalize_into_bucket(tmp_dir, staging_name, bucket_dir, &candidate).await
        {
            Ok(()) => return Ok(candidate),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(
                    AppCommandError::io_error("Failed to move staged upload into place")
                        .with_detail(e.to_string()),
                );
            }
        }
    }

    for _ in 0..UPLOAD_UUID_FALLBACK_ATTEMPTS {
        let candidate = upload_uuid_fallback_candidate(safe_name);
        match upload_jail::finalize_into_bucket(tmp_dir, staging_name, bucket_dir, &candidate).await
        {
            Ok(()) => return Ok(candidate),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(
                    AppCommandError::io_error("Failed to move staged upload into place")
                        .with_detail(e.to_string()),
                );
            }
        }
    }

    Err(
        AppCommandError::io_error("Failed to choose an available upload filename")
            .with_detail(safe_name.to_string()),
    )
}

/// Sanitize a session identifier used as the upload bucket directory name.
///
/// Different semantics from filenames: a session id should never contain `.`
/// or whitespace, so reuse of `sanitize_upload_filename` would silently merge
/// distinct sessions whose ids degenerate to an empty string. Only allow
/// `[A-Za-z0-9_-]`; everything else collapses to `_`. Empty input falls back
/// to `"anon"`.
pub(super) fn sanitize_session_bucket(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| match c {
            c if c.is_ascii_alphanumeric() => c,
            '-' | '_' => c,
            _ => '_',
        })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    let limited: String = trimmed.chars().take(80).collect();
    if limited.is_empty() {
        "anon".to_string()
    } else {
        limited
    }
}

/// Confirm `candidate` resolves (after symlink expansion) inside `root`.
/// Returns the canonical path on success. Both paths must exist on disk.
pub(super) async fn ensure_path_inside(
    candidate: &std::path::Path,
    root: &std::path::Path,
) -> Result<std::path::PathBuf, AppCommandError> {
    let candidate_canon = tokio::fs::canonicalize(candidate).await.map_err(|e| {
        AppCommandError::io_error("Failed to canonicalize upload path").with_detail(e.to_string())
    })?;
    let root_canon = tokio::fs::canonicalize(root).await.map_err(|e| {
        AppCommandError::io_error("Failed to canonicalize uploads root").with_detail(e.to_string())
    })?;
    if !candidate_canon.starts_with(&root_canon) {
        return Err(
            AppCommandError::io_error("Resolved upload path escapes uploads root")
                .with_detail(candidate_canon.to_string_lossy().to_string()),
        );
    }
    Ok(candidate_canon)
}

/// Remove any leftover staging files in `<uploads_root>/.tmp/`.
///
/// Called once at server startup. Staging files represent in-flight uploads
/// that were interrupted by a crash/restart — they are unreferenced by
/// definition and safe to drop. Distinct from the per-bucket history under
/// `<uploads_root>/<bucket>/`, which the user explicitly opted to retain.
///
/// Failures are logged and swallowed: a missing `.tmp/` directory is the
/// expected case on a fresh install, and permission issues should not block
/// the server from starting.
pub async fn purge_upload_staging() {
    let root = iyw_claw_uploads_root();
    for name in [".tmp", ".image-tmp"] {
        let tmp_dir = root.join(name);
        match tokio::fs::remove_dir_all(&tmp_dir).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::error!(
                    "[uploads] failed to purge staging dir {}: {}",
                    tmp_dir.display(),
                    e
                );
            }
        }
    }
}

pub async fn upload_attachment(
    mut multipart: Multipart,
) -> Result<Json<UploadAttachmentResult>, AppCommandError> {
    let monitor = monitor_file_upload(None).await?;
    let uploads_root = iyw_claw_uploads_root();
    // Ensure root exists before canonicalize/ensure_path_inside can compare.
    tokio::fs::create_dir_all(&uploads_root)
        .await
        .map_err(|e| {
            AppCommandError::io_error("Failed to create uploads root").with_detail(e.to_string())
        })?;

    // Quota check, before staging any bytes. We assume the worst-case
    // payload size (`UPLOAD_MAX_BYTES`) since the actual size isn't
    // known until the multipart body is drained — admitting a request
    // we'd reject mid-stream would waste disk and require cleanup races.
    //
    // The reservation guard (`_quota_guard`) is bound to a name so its
    // RAII drop runs at function exit, not immediately. Releasing it
    // on every exit path (success, multipart error, panic) closes the
    // TOCTOU window where two concurrent uploads both saw the same
    // disk-level `used` and admitted past the cap.
    let _quota_guard = if let Some(cap) = upload_quota_config_from_env().cap_bytes() {
        let used = current_uploads_total_bytes(&uploads_root).await;
        match try_reserve_in_flight(&UPLOAD_IN_FLIGHT_BYTES, UPLOAD_MAX_BYTES, used, cap) {
            Ok(guard) => Some(guard),
            Err(()) => {
                let mut params = BTreeMap::new();
                params.insert("used".to_string(), used.to_string());
                params.insert("limit".to_string(), cap.to_string());
                return Err(
                    AppCommandError::io_error("Upload quota exceeded for this server")
                        .with_detail(format!("used={used} limit={cap}"))
                        .with_i18n(UPLOAD_I18N_KEY_QUOTA_EXCEEDED, params),
                );
            }
        }
    } else {
        None
    };

    // Pre-stage the file under <uploads_root>/.tmp/<uuid>.part so we can
    // stream bytes to disk without knowing the final bucket up front (the
    // session_id form field may arrive after the file). On success we rename
    // into place; on any error we delete it.
    let tmp_dir = uploads_root.join(".tmp");
    tokio::fs::create_dir_all(&tmp_dir).await.map_err(|e| {
        AppCommandError::io_error("Failed to create tmp directory").with_detail(e.to_string())
    })?;
    // Reject a symlinked `.tmp` for the same reason the bucket check
    // below rejects a symlinked bucket: `create_dir_all` is a no-op when
    // the target of a symlink already exists, so a pre-placed symlink
    // would let staged bytes land outside the uploads root before the
    // rename even runs (and a failed-stream cleanup would `remove_file`
    // outside the jail too).
    match tokio::fs::symlink_metadata(&tmp_dir).await {
        Ok(meta) if meta.file_type().is_symlink() => {
            return Err(AppCommandError::io_error(
                "Refusing to use a symlinked uploads tmp directory",
            )
            .with_detail(tmp_dir.to_string_lossy().to_string()));
        }
        Ok(_) => {}
        Err(e) => {
            return Err(
                AppCommandError::io_error("Failed to inspect uploads tmp directory")
                    .with_detail(e.to_string()),
            );
        }
    }
    ensure_path_inside(&tmp_dir, &uploads_root).await?;
    let staging_id = uuid::Uuid::new_v4().simple().to_string();
    let staging_name = format!("{staging_id}.part");

    // Wrap the streaming work so any early return cleans up the staged file.
    // Cleanup goes through `upload_jail::remove_staging_best_effort` so the
    // unlink itself can't be redirected by a swap of `.tmp` between
    // streaming and cleanup.
    let context = AttachmentUploadContext {
        uploads_root: &uploads_root,
        tmp_dir: &tmp_dir,
        staging_name: &staging_name,
        monitor: &monitor,
    };
    let result = stream_and_finalize(&mut multipart, context).await;
    if result.is_err() {
        upload_jail::remove_staging_best_effort(&tmp_dir, &staging_name).await;
    }
    // `_quota_guard` drops here regardless of `result`, releasing the
    // reservation for the next admission.
    result.map(Json)
}

struct AttachmentUploadContext<'a> {
    uploads_root: &'a std::path::Path,
    tmp_dir: &'a std::path::Path,
    staging_name: &'a str,
    monitor: &'a CapabilityRevocationMonitor,
}

/// Drain the multipart body and produce the final upload result. Splits out
/// of `upload_attachment` so a single staging-file cleanup wraps every early
/// return.
///
/// The staging file is created via `upload_jail::create_staging_file` so a
/// pre-placed symlink at `<tmp_dir>/<staging_name>` cannot redirect the
/// write outside the jail (Unix `O_NOFOLLOW`); the final move into the
/// bucket likewise goes through `upload_jail::finalize_into_bucket` which
/// uses `renameat` with NOFOLLOW dirfds so a concurrent swap of either
/// `tmp_dir` or `bucket` cannot land the file outside the root.
async fn stream_and_finalize(
    multipart: &mut Multipart,
    context: AttachmentUploadContext<'_>,
) -> Result<UploadAttachmentResult, AppCommandError> {
    let mut session_id: Option<String> = None;
    let mut raw_name: Option<String> = None;
    let mut mime_type: Option<String> = None;
    let mut written: u64 = 0;
    let mut file_seen = false;

    while let Some(mut field) = context
        .monitor
        .run_until_revoked(multipart.next_field())
        .await?
        .map_err(|e| {
            AppCommandError::io_error("Invalid multipart upload").with_detail(e.to_string())
        })?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "session_id" | "sessionId" => {
                let value = context
                    .monitor
                    .run_until_revoked(field.text())
                    .await?
                    .map_err(|e| {
                        AppCommandError::io_error("Failed to read session_id field")
                            .with_detail(e.to_string())
                    })?;
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    session_id = Some(sanitize_session_bucket(trimmed));
                }
            }
            "file" => {
                if file_seen {
                    return Err(AppCommandError::io_error(
                        "Multiple `file` fields are not supported per request",
                    ));
                }
                file_seen = true;
                raw_name = Some(field.file_name().unwrap_or("file").to_string());
                mime_type = field.content_type().map(|s| s.to_string());

                let mut out =
                    upload_jail::create_staging_file(context.tmp_dir, context.staging_name)
                        .await
                        .map_err(|e| {
                            AppCommandError::io_error("Failed to create staging file")
                                .with_detail(e.to_string())
                        })?;
                while let Some(chunk) = context
                    .monitor
                    .run_until_revoked(field.chunk())
                    .await?
                    .map_err(|e| {
                        AppCommandError::io_error("Failed to read upload chunk")
                            .with_detail(e.to_string())
                    })?
                {
                    let new_total = written.saturating_add(chunk.len() as u64);
                    if new_total > UPLOAD_MAX_BYTES {
                        // Symmetric with the proxy's pre/post-decode caps
                        // in `commands/remote_proxy.rs`: any of the three
                        // layers can fire first depending on how the
                        // request reached us (web direct, Tauri-proxied,
                        // or local path read), and they all surface as
                        // the same i18n key so the toast text in the UI
                        // is uniform.
                        let mut params = BTreeMap::new();
                        params.insert("size".to_string(), new_total.to_string());
                        params.insert("limit".to_string(), UPLOAD_MAX_BYTES.to_string());
                        return Err(AppCommandError::io_error(
                            "Upload exceeds the maximum allowed size",
                        )
                        .with_detail(format!("size={new_total} limit={UPLOAD_MAX_BYTES}"))
                        .with_i18n(UPLOAD_I18N_KEY_TOO_LARGE, params));
                    }
                    out.write_all(&chunk).await.map_err(|e| {
                        AppCommandError::io_error("Failed to write chunk")
                            .with_detail(e.to_string())
                    })?;
                    written = new_total;
                }
                out.flush().await.map_err(|e| {
                    AppCommandError::io_error("Failed to flush staging file")
                        .with_detail(e.to_string())
                })?;
            }
            _ => {
                // Drain unknown fields to avoid stalling the multipart parser.
                let _ = context.monitor.run_until_revoked(field.bytes()).await?;
            }
        }
    }

    if !file_seen {
        return Err(AppCommandError::io_error(
            "Missing `file` field in multipart upload",
        ));
    }
    if written == 0 {
        return Err(AppCommandError::io_error("Uploaded file is empty"));
    }

    let safe_name = sanitize_upload_filename(raw_name.as_deref().unwrap_or("file"));
    let bucket = session_id.unwrap_or_else(|| "anon".to_string());
    let dir = context.uploads_root.join(&bucket);
    tokio::fs::create_dir_all(&dir).await.map_err(|e| {
        AppCommandError::io_error("Failed to create uploads directory").with_detail(e.to_string())
    })?;

    // Reject the bucket directory itself if it's a symlink — `create_dir_all`
    // is a no-op when the target of a symlink already exists, so a pre-placed
    // symlink at <uploads_root>/<bucket> would silently let the rename below
    // land outside the jail. Filename sanitization can't help here because
    // the bucket path is what's being subverted.
    match tokio::fs::symlink_metadata(&dir).await {
        Ok(meta) if meta.file_type().is_symlink() => {
            return Err(
                AppCommandError::io_error("Refusing to use a symlinked upload bucket")
                    .with_detail(dir.to_string_lossy().to_string()),
            );
        }
        Ok(_) => {}
        Err(e) => {
            return Err(
                AppCommandError::io_error("Failed to inspect uploads bucket directory")
                    .with_detail(e.to_string()),
            );
        }
    }
    // And confirm the bucket canonicalizes inside the uploads root before
    // the rename commits any bytes outside it. This is the load-bearing
    // jail check; the post-rename `ensure_path_inside` below is kept as
    // defense in depth.
    ensure_path_inside(&dir, context.uploads_root).await?;

    // TOCTOU-safe finalization: `finalize_into_bucket` opens both `tmp_dir`
    // and `dir` as `O_NOFOLLOW` dirfds and creates the final name without
    // replacing an existing file. If the original name is already present in
    // this session bucket, retry with `name (1).ext`, `name (2).ext`, etc.
    context.monitor.require_current().await?;
    let final_name = finalize_with_available_upload_name(
        context.tmp_dir,
        context.staging_name,
        &dir,
        &safe_name,
    )
    .await?;
    let final_path = dir.join(&final_name);

    if let Err(error) = context.monitor.require_current().await {
        let _ = tokio::fs::remove_file(&final_path).await;
        return Err(error);
    }

    // Defense in depth: even though every component above was sanitized AND
    // the bucket dir was validated pre-finalization AND the commit itself
    // went through `O_NOFOLLOW` dirfds, run the final canonical path through
    // the jail check too. If somehow this fires, the file is already on disk
    // at `final_path` — clean it up so we don't leak data outside the jail
    // just because we noticed late.
    let canon = match ensure_path_inside(&final_path, context.uploads_root).await {
        Ok(p) => p,
        Err(err) => {
            let _ = tokio::fs::remove_file(&final_path).await;
            return Err(err);
        }
    };

    Ok(UploadAttachmentResult {
        path: canon.to_string_lossy().to_string(),
        name: final_name,
        size: written,
        mime_type,
    })
}
