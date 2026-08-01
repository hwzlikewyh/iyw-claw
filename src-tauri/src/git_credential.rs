use std::path::{Path, PathBuf};

use sea_orm::DatabaseConnection;

use crate::db::service::app_metadata_service;
use crate::models::system::{GitHubAccount, GitHubAccountsSettings};

const GITHUB_ACCOUNTS_KEY: &str = "github_accounts";

/// Wrap a value as a POSIX-sh single-quoted literal so it survives shell
/// evaluation regardless of which characters it contains. Internal `'`
/// is closed, escaped, and reopened (`'\''`). Used both inside the
/// generated helper script and inside `GIT_CONFIG_VALUE_0` (which git
/// hands to `sh -c`).
pub(crate) fn sh_single_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for ch in s.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

/// Wrap a value as a Windows-batch double-quoted literal. Inside batch
/// files, `%VAR%` is expanded even within double quotes, so we must
/// escape `%` to `%%` (which the batch parser rewrites back to a literal
/// `%`). Internal `"` is doubled to `""` so the value can't terminate the
/// quoted string. Compiled on Windows (where it's used by the helper
/// script generator) and under `cfg(test)` (so tests can exercise the
/// escape rules from any host).
#[cfg(any(windows, test))]
pub(crate) fn bat_double_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\"\""),
            '%' => out.push_str("%%"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Make a path absolute without requiring it to exist on disk. Falls back
/// to the original path if the current directory is unreadable.
pub fn absolutize(p: &Path) -> PathBuf {
    if p.is_absolute() {
        return p.to_path_buf();
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(p))
        .unwrap_or_else(|_| p.to_path_buf())
}

/// Create a credential helper that calls the app binary directly with
/// `--credential-helper` flag. The app binary opens the DB, looks up
/// the matching account, and outputs credentials to stdout.
///
/// The script embeds the absolute `--data-dir` path so that server-mode
/// deployments (which don't share the desktop's Tauri identifier-derived
/// data dir) point the helper at the correct database. `run_credential_helper`
/// falls back to a hardcoded resolution if the flag is missing for back-compat
/// with stale scripts on disk.
///
/// Paths are absolutized and shell-quoted before being substituted into the
/// script body so a quirky data dir (`IYW_CLAW_DATA_DIR=$HOME/x`, paths with
/// spaces, single quotes, or `"`) cannot break — or escape — the script.
pub fn create_credential_helper_script(
    app_data_dir: &Path,
    app_binary_path: &Path,
) -> std::io::Result<PathBuf> {
    let app_data_dir = absolutize(app_data_dir);
    let app_binary_path = absolutize(app_binary_path);
    let binary_str = app_binary_path.to_string_lossy();
    let data_dir_str = app_data_dir.to_string_lossy();

    #[cfg(unix)]
    {
        let script_path = app_data_dir.join("git-credential-iyw-claw.sh");
        let content = format!(
            r#"#!/bin/sh
# iyw-claw credential helper — calls the app binary to look up credentials.
# Only responds to "get" action; ignores "store" and "erase".
[ "$1" != "get" ] && exit 0
exec {binary} --credential-helper --data-dir {data_dir} < /dev/stdin
"#,
            binary = sh_single_quote(&binary_str),
            data_dir = sh_single_quote(&data_dir_str),
        );
        std::fs::write(&script_path, content)?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))?;
        Ok(script_path)
    }

    #[cfg(windows)]
    {
        let script_path = app_data_dir.join("git-credential-iyw-claw.bat");
        let content = format!(
            r#"@echo off
if not "%~1"=="get" exit /b 0
{binary} --credential-helper --data-dir {data_dir}
"#,
            binary = bat_double_quote(&binary_str),
            data_dir = bat_double_quote(&data_dir_str),
        );
        std::fs::write(&script_path, content)?;
        Ok(script_path)
    }
}

/// Run the credential helper mode (called from main when `--credential-helper` is detected).
///
/// Reads git's credential protocol from stdin (host=xxx, protocol=xxx),
/// opens the DB, finds the matching account, and outputs username/password
/// to stdout. Exits immediately — does NOT start the Tauri GUI.
pub fn run_credential_helper() {
    // Parse the optional `--data-dir <path>` arg. Helper scripts written by
    // recent iyw-claw versions embed this so server deployments and custom
    // IYW_CLAW_DATA_DIR setups land on the right database.
    let explicit_data_dir = parse_data_dir_arg(std::env::args());

    // Pin IYW_CLAW_DATA_DIR for downstream lookups (notably the file-based
    // `keyring_store::tokens_file_path` in server mode). The DB path comes
    // from `--data-dir`, but the token file path comes from the env var,
    // and they must match — otherwise the helper finds the account row but
    // returns no token. set_var is safe here because run_credential_helper
    // is invoked from main() before any tokio runtime is built.
    if let Some(dir) = &explicit_data_dir {
        std::env::set_var("IYW_CLAW_DATA_DIR", dir);
    }

    // git's credential protocol expects this helper to be silent on a
    // miss so the caller (and any subsequent helper in the chain) can
    // proceed cleanly. `Ok(None)` outcomes — empty stdin, missing DB,
    // unmatched host, no token — return without writing to stderr; only
    // genuine errors (DB connection failure, runtime build failure) are
    // surfaced. This avoids polluting the agent's terminal output and
    // leaking the local data-dir path on every GitLab/enterprise URL.
    let host = read_host_from_stdin();
    if host.is_empty() {
        return;
    }

    // Prefer the path passed by the helper script; only fall back to the
    // hardcoded Tauri-style location for stale scripts written before the
    // `--data-dir` flag existed.
    let app_data_dir = match explicit_data_dir.or_else(resolve_app_data_dir) {
        Some(d) => d,
        None => return,
    };

    // Use a minimal tokio runtime just for the DB query
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!("[iyw-claw credential-helper] tokio runtime build failed: {e}");
            return;
        }
    };

    match rt.block_on(lookup_credential(&app_data_dir, &host)) {
        Ok(Some((username, token))) => {
            println!("username={username}");
            println!("password={token}");
        }
        Ok(None) => {
            // Silent — git falls through to the next helper. See block
            // comment above for why we don't log here.
        }
        Err(e) => tracing::error!("[iyw-claw credential-helper] lookup failed: {e}"),
    }
}

/// Read host from git's credential protocol on stdin. Returns empty string
/// on EOF or if no `host=` line was provided.
fn read_host_from_stdin() -> String {
    use std::io::BufRead;
    let mut host = String::new();
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("host=") {
            host = value.to_string();
        }
    }
    host
}

/// Look up a (username, token) pair for the given host using the iyw-claw
/// database in `app_data_dir` plus whichever token store is active for this
/// build (OS keyring on desktop, `tokens.json` on server).
///
/// Returns:
///   - `Ok(Some((u, p)))` when a matching account + token are found.
///   - `Ok(None)` for normal misses — DB doesn't exist yet, no GitHub
///     accounts configured, no account matches `host`, token absent
///     from the store. All miss paths are silent so the helper doesn't
///     pollute agent/terminal output or leak the local data-dir path
///     when the user hits an unconfigured GitLab/enterprise host.
///   - `Err(_)` for I/O / DB / driver errors that the caller should
///     surface verbatim.
///
/// Extracted from `run_credential_helper` so tests can exercise the
/// lookup path without spawning a subprocess.
pub(crate) async fn lookup_credential(
    app_data_dir: &Path,
    host: &str,
) -> Result<Option<(String, String)>, String> {
    let db_path = app_data_dir.join(crate::db::database_file_name());
    if !db_path.exists() {
        return Ok(None);
    }

    let db_url = format!(
        "sqlite:{}?mode=ro",
        urlencoding::encode(&db_path.to_string_lossy())
    );
    let opts = sea_orm::ConnectOptions::new(db_url);
    let conn = sea_orm::Database::connect(opts)
        .await
        .map_err(|e| format!("open db: {e}"))?;

    let settings = match load_github_accounts(&conn).await {
        Some(s) => s,
        None => return Ok(None),
    };

    let remote_url = format!("https://{host}");
    let account = match find_matching_account(&settings.accounts, &remote_url) {
        Some(a) => a,
        None => return Ok(None),
    };

    match crate::keyring_store::get_token(&account.id) {
        Some(token) => Ok(Some((account.username.clone(), token))),
        None => Ok(None),
    }
}

/// Extract `--data-dir <path>` (or `--data-dir=<path>`) from the given args,
/// or `None` if absent. Generic over any String iterator so tests can
/// supply a vec without touching the real process args.
fn parse_data_dir_arg<I: IntoIterator<Item = String>>(args: I) -> Option<std::path::PathBuf> {
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        if arg == "--data-dir" {
            return iter.next().map(std::path::PathBuf::from);
        }
        if let Some(value) = arg.strip_prefix("--data-dir=") {
            return Some(std::path::PathBuf::from(value));
        }
    }
    None
}

/// Resolve the app data directory (same path Tauri uses).
fn resolve_app_data_dir() -> Option<std::path::PathBuf> {
    // On macOS: ~/Library/Application Support/app.iywclaw
    // On Linux: ~/.local/share/app.iywclaw
    // On Windows: %APPDATA%/app.iywclaw
    #[cfg(target_os = "macos")]
    {
        dirs::data_dir().map(|d| d.join("app.iywclaw"))
    }
    #[cfg(target_os = "linux")]
    {
        dirs::data_dir().map(|d| d.join("app.iywclaw"))
    }
    #[cfg(target_os = "windows")]
    {
        dirs::data_dir().map(|d| d.join("app.iywclaw"))
    }
}

/// Ensure the GIT_ASKPASS helper script exists in the app data directory.
/// Returns the path to the script.
pub fn ensure_askpass_script(app_data_dir: &Path) -> std::io::Result<PathBuf> {
    #[cfg(unix)]
    {
        let script_path = app_data_dir.join("git-askpass.sh");
        if !script_path.exists() {
            let content = r#"#!/bin/sh
case "$1" in
*[Uu]sername*) echo "$IYW_CLAW_GIT_USERNAME" ;;
*[Pp]assword*) echo "$IYW_CLAW_GIT_PASSWORD" ;;
esac
"#;
            std::fs::write(&script_path, content)?;
            // Make executable
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))?;
        }
        Ok(script_path)
    }

    #[cfg(windows)]
    {
        let script_path = app_data_dir.join("git-askpass.bat");
        if !script_path.exists() {
            let content = r#"@echo off
echo %1 | findstr /i "username" >nul
if %errorlevel% equ 0 (
    echo %IYW_CLAW_GIT_USERNAME%
    exit /b
)
echo %1 | findstr /i "password" >nul
if %errorlevel% equ 0 (
    echo %IYW_CLAW_GIT_PASSWORD%
    exit /b
)
"#;
            std::fs::write(&script_path, content)?;
        }
        Ok(script_path)
    }
}

/// Inject GitHub credentials into a git command via GIT_ASKPASS.
pub fn inject_credentials(
    cmd: &mut tokio::process::Command,
    username: &str,
    token: &str,
    askpass_path: &Path,
) {
    // Clear all system credential helpers (e.g. macOS Keychain `osxkeychain`)
    // so git falls through to GIT_ASKPASS. Without this, a system credential
    // helper may return stale/wrong credentials and GIT_ASKPASS is never called.
    // Per git docs, setting credential.helper="" resets the helper list.
    cmd.env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "credential.helper")
        .env("GIT_CONFIG_VALUE_0", "")
        .env("GIT_ASKPASS", askpass_path)
        .env("IYW_CLAW_GIT_USERNAME", username)
        .env("IYW_CLAW_GIT_PASSWORD", token)
        .env("GIT_TERMINAL_PROMPT", "0");
}

/// Get the remote URL for the "origin" remote of a repository.
pub async fn get_remote_url(repo_path: &str) -> Option<String> {
    get_remote_url_by_name(repo_path, "origin").await
}

/// Get the remote URL for a specific named remote.
pub async fn get_remote_url_by_name(repo_path: &str, remote_name: &str) -> Option<String> {
    let output = crate::process::tokio_command("git")
        .args(["remote", "get-url", remote_name])
        .current_dir(repo_path)
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if url.is_empty() {
        None
    } else {
        Some(url)
    }
}

/// Extract the hostname from a git remote URL.
///
/// Handles both HTTPS and SSH URLs:
/// - `https://github.com/user/repo.git` → `github.com`
/// - `git@github.com:user/repo.git` → `github.com`
fn extract_host(remote_url: &str) -> Option<String> {
    let url = remote_url.trim();

    // HTTPS: https://github.com/...
    if let Some(after_scheme) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    {
        // Strip optional user@ prefix (e.g. https://user@github.com/...)
        let after_at = after_scheme
            .find('@')
            .map(|i| &after_scheme[i + 1..])
            .unwrap_or(after_scheme);
        return after_at.split('/').next().map(|h| h.to_lowercase());
    }

    // SSH: git@github.com:user/repo.git
    if let Some(at_pos) = url.find('@') {
        let after_at = &url[at_pos + 1..];
        return after_at.split(':').next().map(|h| h.to_lowercase());
    }

    None
}

/// Mask any `userinfo` segment (`username[:password]@`) from a URL used in
/// log output, so credentials embedded in remote URLs are never written to
/// logs (P0-1 / IYW-SEC-001). The host and path are preserved for debugging.
fn redact_url_userinfo(url: &str) -> String {
    let Some(scheme_end) = url.find("://") else {
        return url.to_string();
    };
    let after_scheme = &url[scheme_end + 3..];
    let Some(at_pos) = after_scheme.find('@') else {
        return url.to_string();
    };
    // Only treat the segment as userinfo when `@` appears before any path
    // separator; otherwise it is part of the path and must stay intact.
    if after_scheme[..at_pos].contains('/') {
        return url.to_string();
    }
    format!("{}://***@{}", &url[..scheme_end], &after_scheme[at_pos + 1..])
}

/// Find the best matching account for a given remote URL.
///
/// Only returns an account whose server_url hostname matches the remote URL host.
/// When multiple accounts match the same hostname, prefers the one marked `is_default`.
/// Does NOT fall back to unrelated accounts — if no hostname matches, returns None
/// so the caller can fall back to git config defaults.
pub fn find_matching_account<'a>(
    accounts: &'a [GitHubAccount],
    remote_url: &str,
) -> Option<&'a GitHubAccount> {
    if accounts.is_empty() {
        return None;
    }

    let remote_host = extract_host(remote_url)?;

    let matching: Vec<&GitHubAccount> = accounts
        .iter()
        .filter(|a| {
            let account_host = extract_host(&a.server_url)
                .unwrap_or_else(|| a.server_url.trim().trim_end_matches('/').to_lowercase());
            account_host == remote_host
        })
        .collect();

    // Prefer the default account among matches, otherwise take the first
    matching
        .iter()
        .find(|a| a.is_default)
        .or(matching.first())
        .copied()
}

/// Load GitHub accounts from the database.
pub async fn load_github_accounts(conn: &DatabaseConnection) -> Option<GitHubAccountsSettings> {
    let raw = app_metadata_service::get_value(conn, GITHUB_ACCOUNTS_KEY)
        .await
        .ok()??;

    serde_json::from_str::<GitHubAccountsSettings>(&raw).ok()
}

/// Resolve the commit author (name + email) from the matching account for a repo.
///
/// Returns `Some((name, email))` if a matching account is found.
/// Uses GitHub's noreply email format: `username@users.noreply.github.com`.
pub async fn resolve_commit_author(
    repo_path: &str,
    conn: &DatabaseConnection,
) -> Option<(String, String)> {
    let settings = load_github_accounts(conn).await?;
    if settings.accounts.is_empty() {
        return None;
    }

    let remote_url = get_remote_url(repo_path).await?;
    let account = find_matching_account(&settings.accounts, &remote_url)?;

    let host = extract_host(&remote_url).unwrap_or_default();
    let email = if host == "github.com" {
        format!("{}@users.noreply.github.com", account.username)
    } else {
        // For non-GitHub hosts, use username@host as a reasonable fallback
        format!("{}@{}", account.username, host)
    };

    Some((account.username.clone(), email))
}

/// Resolve credentials for a git repository and inject them into the command.
///
/// This is the main entry point: given a repo path and a git command,
/// it finds the matching GitHub account and injects credentials.
/// When `remote_name` is provided, uses that remote's URL for credential matching;
/// otherwise defaults to "origin".
/// Returns `true` if credentials were injected.
pub async fn try_inject_for_repo(
    cmd: &mut tokio::process::Command,
    repo_path: &str,
    conn: &DatabaseConnection,
    app_data_dir: &Path,
) -> bool {
    try_inject_for_repo_remote(cmd, repo_path, None, conn, app_data_dir).await
}

/// Same as `try_inject_for_repo` but allows specifying the remote name.
pub async fn try_inject_for_repo_remote(
    cmd: &mut tokio::process::Command,
    repo_path: &str,
    remote_name: Option<&str>,
    conn: &DatabaseConnection,
    app_data_dir: &Path,
) -> bool {
    let settings = match load_github_accounts(conn).await {
        Some(s) if !s.accounts.is_empty() => s,
        _ => {
            tracing::info!("[GIT_CRED] no accounts configured");
            return false;
        }
    };

    let target_remote = remote_name.unwrap_or("origin");
    let remote_url = match get_remote_url_by_name(repo_path, target_remote).await {
        Some(url) => url,
        None => {
            tracing::info!(
                "[GIT_CRED] no remote URL found for {} (remote: {})",
                repo_path,
                target_remote
            );
            return false;
        }
    };

    // Only inject for HTTPS URLs (SSH uses keys, not tokens)
    if !remote_url.starts_with("https://") && !remote_url.starts_with("http://") {
        tracing::warn!(
            "[GIT_CRED] skipping non-HTTPS URL: {}",
            redact_url_userinfo(&remote_url)
        );
        return false;
    }

    let account = match find_matching_account(&settings.accounts, &remote_url) {
        Some(a) => a,
        None => {
            tracing::info!(
                "[GIT_CRED] no matching account for remote {}. Available hosts: {}",
                redact_url_userinfo(&remote_url),
                settings
                    .accounts
                    .iter()
                    .map(|a| redact_url_userinfo(&a.server_url))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            return false;
        }
    };

    let askpass = match ensure_askpass_script(app_data_dir) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("[GIT_CRED] failed to create askpass script: {}", e);
            return false;
        }
    };

    let token = match crate::keyring_store::get_token(&account.id) {
        Some(t) => t,
        None => {
            tracing::info!("[GIT_CRED] no token in keyring for account {}", account.id);
            return false;
        }
    };

    tracing::info!(
        "[GIT_CRED] injecting credentials for {} (user: {})",
        redact_url_userinfo(&remote_url),
        account.username
    );
    inject_credentials(cmd, &account.username, &token, &askpass);
    true
}

/// Same as `try_inject_for_repo` but for clone operations where
/// we don't have a repo path yet — just a URL.
pub async fn try_inject_for_url(
    cmd: &mut tokio::process::Command,
    clone_url: &str,
    conn: &DatabaseConnection,
    app_data_dir: &Path,
) -> bool {
    if !clone_url.starts_with("https://") && !clone_url.starts_with("http://") {
        return false;
    }

    let settings = match load_github_accounts(conn).await {
        Some(s) if !s.accounts.is_empty() => s,
        _ => return false,
    };

    let account = match find_matching_account(&settings.accounts, clone_url) {
        Some(a) => a,
        None => return false,
    };

    let token = match crate::keyring_store::get_token(&account.id) {
        Some(t) => t,
        None => {
            tracing::info!("[GIT_CRED] no token in keyring for account {}", account.id);
            return false;
        }
    };

    let askpass = match ensure_askpass_script(app_data_dir) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("[GIT_CRED] failed to create askpass script: {}", e);
            return false;
        }
    };

    inject_credentials(cmd, &account.username, &token, &askpass);
    true
}
