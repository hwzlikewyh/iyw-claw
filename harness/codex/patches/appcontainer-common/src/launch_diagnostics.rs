// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Post-failure launch diagnostics.
//!
//! When a process-creation call (`CreateProcessW` or
//! `Experimental_CreateProcessInSandbox`) fails, or when the child exits
//! with a non-zero code immediately, the caller can invoke
//! [`diagnose_create_process_failure`] or [`diagnose_process_exit`] to check
//! for well-known environment conditions and produce an actionable message.
//!
//! This module is intentionally decoupled from the runner implementations
//! so both `AppContainerScriptRunner` and `BaseContainerRunner` share the
//! same detection logic.

use std::path::Path;

/// A structured diagnostic describing *why* a sandboxed process launch failed
/// and what the user can do about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchDiagnostic {
    /// Machine-readable discriminator (e.g. `"packaged_app"`,
    /// `"missing_filesystem_access"`).
    pub kind: &'static str,
    /// Human-readable explanation of the failure including remediation guidance.
    pub message: String,
}

// -- Public API --------------------------------------------------------------

/// Diagnose a failed `CreateProcess` / `Experimental_CreateProcessInSandbox`
/// call. Inspects the Win32 error code and the command line to identify known
/// failure conditions.
///
/// Always returns a `LaunchDiagnostic` -- if no specific heuristic matches,
/// a generic message is produced from the raw error code.
pub fn diagnose_create_process_failure(
    win32_error: u32,
    command_line: &str,
    readonly_paths: &[String],
) -> LaunchDiagnostic {
    if win32_error == ERROR_ACCESS_DISABLED_BY_POLICY.0 {
        return LaunchDiagnostic {
            kind: "launch_blocked_by_policy",
            message:
                "Windows blocked the sandboxed process launch because of an IT-managed policy rule \
                      (ERROR_ACCESS_DISABLED_BY_POLICY, 1260). Contact your system administrator \
                      to allow the target executable to run in an MXC sandbox."
                    .to_string(),
        };
    }

    // Check for feature-not-enabled (velocity keys).
    if win32_error == ERROR_CALL_NOT_IMPLEMENTED.0 || win32_error == E_NOTIMPL.0 as u32 {
        return diagnose_api_not_implemented();
    }

    // Resolve the exe from the command line for further heuristics.
    let bare_exe = Path::new(extract_exe_from_command_line(command_line));
    let resolved_exe = resolve_exe_on_path(bare_exe);

    if let Some(diag) = check_exe_heuristics(&resolved_exe, readonly_paths, None) {
        return diag;
    }

    // Generic fallback.
    LaunchDiagnostic {
        kind: "create_process_failed",
        message: format!(
            "CreateProcessInSandbox failed with error code {win32_error} (0x{win32_error:08X})."
        ),
    }
}

/// Returns `true` when the Win32 error is `ERROR_NOT_SUPPORTED` (0x32) and
/// the caller passed a non-null environment block. Downlevel OS builds that
/// predate environment-parameter support in `Experimental_CreateProcessInSandbox`
/// surface this error; the caller should retry without the environment block.
pub fn is_environment_not_supported(win32_error: u32, has_environment: bool) -> bool {
    win32_error == ERROR_NOT_SUPPORTED.0 && has_environment
}

/// Produce a [`LaunchDiagnostic`] for the environment-not-supported case.
pub fn diagnose_environment_not_supported() -> LaunchDiagnostic {
    LaunchDiagnostic {
        kind: "environment_not_supported_downlevel",
        message: "WARNING: The `environment` parameter is not supported on this OS build. \
                  Retrying without explicit environment variables."
            .to_string(),
    }
}

/// Diagnose a process that launched successfully but exited with a non-zero
/// code. Returns `None` when no recognized condition matches.
pub fn diagnose_process_exit(
    command_line: &str,
    readonly_paths: &[String],
    _readwrite_paths: &[String],
    exit_code: u32,
) -> Option<LaunchDiagnostic> {
    let bare_exe = Path::new(extract_exe_from_command_line(command_line));
    let resolved_exe = resolve_exe_on_path(bare_exe);
    if let Some(diag) = check_exe_heuristics(&resolved_exe, readonly_paths, Some(exit_code)) {
        return Some(diag);
    }
    None
}

// -- Constants ---------------------------------------------------------------

/// Velocity key IDs required by the BaseContainer feature.
const REQUIRED_VELOCITY_KEYS: &[(u32, &str)] = &[
    (61389575, "BaseContainer core"),
    (61155944, "BaseContainer sandbox spec"),
];

// `ERROR_CALL_NOT_IMPLEMENTED`, `E_NOTIMPL`, and `STATUS_DLL_INIT_FAILED`
// are re-exported from the `windows` crate. Comparisons against them
// flow through `u32`, which matches the existing public surface of
// this module (`diagnose_create_process_failure` takes `u32`).
use windows::Win32::Foundation::{
    ERROR_ACCESS_DISABLED_BY_POLICY, ERROR_CALL_NOT_IMPLEMENTED, ERROR_NOT_SUPPORTED, E_NOTIMPL,
    STATUS_DLL_INIT_FAILED,
};

// -- Internal heuristics -----------------------------------------------------

/// Checks exe-path-based heuristics (packaged app, DLL init failure, missing
/// root access). Returns `None` if nothing matches.
fn check_exe_heuristics(
    exe_path: &Path,
    readonly_paths: &[String],
    exit_code: Option<u32>,
) -> Option<LaunchDiagnostic> {
    if is_packaged_app(exe_path) {
        return Some(LaunchDiagnostic {
            kind: "packaged_app",
            message: format!(
                "The target executable '{}' appears to be a packaged (MSIX) app. \
                 Packaged apps cannot be launched inside a sandboxed container. \
                 Uninstall the packaged version and install an unpackaged build.",
                exe_path.display()
            ),
        });
    }

    if exit_code == Some(STATUS_DLL_INIT_FAILED.0 as u32) && is_powershell(exe_path) {
        return Some(LaunchDiagnostic {
            kind: "dll_init_failed_ui_required",
            message: "PowerShell exited with STATUS_DLL_INIT_FAILED (0xC0000142). \
                      This typically means the sandbox is blocking Win32k system calls \
                      (UI subsystem access), which PowerShell requires to initialize. \
                      Enable UI access in your sandbox policy (set `ui.allowWindows: true`)."
                .to_string(),
        });
    }

    if missing_root_readonly(exe_path, readonly_paths) {
        let root = drive_root(exe_path);
        return Some(LaunchDiagnostic {
            kind: "missing_filesystem_access",
            message: format!(
                "pwsh.exe versions before 7.7 require read-only access to the \
                 root drive ({root}) to start. The current sandbox policy does \
                 not grant this access. Add \"{root}\" to `readonlyPaths` in your \
                 sandbox policy, or upgrade to pwsh 7.7+."
            ),
        });
    }

    None
}

/// Produces a diagnostic when the API returns E_NOTIMPL/ERROR_CALL_NOT_IMPLEMENTED,
/// indicating the feature is gated behind velocity keys.
fn diagnose_api_not_implemented() -> LaunchDiagnostic {
    let key_status = check_velocity_keys();

    let message = if key_status.is_empty() {
        "Experimental_CreateProcessInSandbox returned E_NOTIMPL. \
         The BaseContainer feature is not enabled on this OS build. \
         It may be possible to enable it through the Windows experimental \
         features settings, or run on a host that supports the BaseContainer \
         backend (MXC falls back to AppContainer automatically on builds \
         without it)."
            .to_string()
    } else {
        let disabled: Vec<_> = key_status.iter().filter(|(_, enabled)| !enabled).collect();
        if disabled.is_empty() {
            "Experimental_CreateProcessInSandbox returned E_NOTIMPL. \
             The BaseContainer feature is not enabled on this OS build; it may \
             require additional enablement. MXC falls back to AppContainer \
             automatically on builds without BaseContainer support."
                .to_string()
        } else {
            let disabled_list: Vec<String> =
                disabled.iter().map(|(id, _)| id.to_string()).collect();
            format!(
                "Experimental_CreateProcessInSandbox returned E_NOTIMPL. \
                 The BaseContainer feature is not enabled on this OS build \
                 (disabled feature flags: {}). It may be possible to enable it \
                 through the Windows experimental features settings, or run on a \
                 host that supports the BaseContainer backend (MXC falls back to \
                 AppContainer automatically on builds without it).",
                disabled_list.join(", ")
            )
        }
    };

    LaunchDiagnostic {
        kind: "feature_not_enabled",
        message,
    }
}

/// Query the Windows Feature Store registry to check whether each required
/// velocity key is enabled. Returns a list of `(key_id, is_enabled)` pairs.
/// Returns an empty vec if the registry cannot be read.
fn check_velocity_keys() -> Vec<(u32, bool)> {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::HKEY_LOCAL_MACHINE;
        use winreg::RegKey;

        let mut results = Vec::new();
        for &(key_id, _label) in REQUIRED_VELOCITY_KEYS {
            let enabled = [4u32, 8].iter().any(|priority| {
                let path = format!(
                    r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\FeatureManagement\Overrides\{}\{}",
                    priority, key_id
                );
                if let Ok(reg_key) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(&path) {
                    if let Ok(state) = reg_key.get_value::<u32, _>("EnabledState") {
                        return state == 2;
                    }
                }
                false
            });
            results.push((key_id, enabled));
        }
        results
    }
    #[cfg(not(target_os = "windows"))]
    {
        Vec::new()
    }
}

/// Attempt to resolve a potentially bare executable name (e.g. `pwsh.exe`)
/// to its full path by searching the system PATH. Returns the original path
/// if resolution fails or the input is already absolute.
pub fn resolve_exe_on_path(exe: &Path) -> std::path::PathBuf {
    if exe.is_absolute() {
        return exe.to_path_buf();
    }
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(exe);
            if candidate.exists() {
                return candidate;
            }
        }
    }
    exe.to_path_buf()
}

/// Extract the executable path from a command line string.
///
/// Handles both quoted paths (`"C:\Program Files\...\pwsh.exe" -args`) and
/// unquoted paths (`pwsh.exe -args`). Strips surrounding quotes if present.
pub fn extract_exe_from_command_line(command_line: &str) -> &str {
    let trimmed = command_line.trim();
    if let Some(after_quote) = trimmed.strip_prefix('"') {
        match after_quote.find('"') {
            Some(end) => &after_quote[..end],
            None => trimmed.split_whitespace().next().unwrap_or(""),
        }
    } else {
        trimmed.split_whitespace().next().unwrap_or("")
    }
}

// -- Internal detection helpers ----------------------------------------------

fn is_packaged_app(exe_path: &Path) -> bool {
    let normalized = exe_path.to_string_lossy().to_lowercase();
    normalized.contains("\\windowsapps\\") || normalized.contains("/windowsapps/")
}

fn is_powershell(exe_path: &Path) -> bool {
    let filename = exe_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    filename == "pwsh.exe" || filename == "powershell.exe"
}

fn missing_root_readonly(exe_path: &Path, readonly_paths: &[String]) -> bool {
    let filename = exe_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    if filename != "pwsh.exe" {
        return false;
    }
    let root = drive_root(exe_path);
    !readonly_paths
        .iter()
        .any(|p| p.eq_ignore_ascii_case(&root) || p == "\\")
}

fn drive_root(exe_path: &Path) -> String {
    let s = exe_path.to_string_lossy();
    if s.len() >= 3 && s.as_bytes()[1] == b':' {
        format!("{}\\", &s[..2])
    } else {
        "C:\\".to_string()
    }
}

// -- Tests -------------------------------------------------------------------
