// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Working-directory resolution shared by the two Windows ProcessContainer
//! launch paths (`AppContainerScriptRunner` -> `CreateProcessW` and
//! `BaseContainerRunner` -> `Experimental_CreateProcessInSandbox`).
//!
//! Both launch APIs treat a `NULL` current directory as "inherit the parent's
//! cwd". Under a deny-by-default sandbox token that directory is usually not
//! openable, and the kernel then silently restarts the child at the drive root
//! instead of failing the launch — a confusing, unlogged relocation.
//!
//! [`launch_working_directory`] therefore always yields a concrete directory,
//! so neither runner ever passes `NULL`. Keeping the mapping here (rather than
//! inline at each launch site) means it is covered by ordinary unit tests that
//! need no prepared host, and that the two runners cannot drift apart.

use wxc_common::models::{ExecutionRequest, WorkingDirectorySource};

/// Drive root used when neither `process.cwd` nor the filesystem policy yields
/// a usable directory. Matches what `wxc-host-prep prepare-system-drive` grants
/// sandbox tokens traverse access to.
const DEFAULT_DRIVE_ROOT: &str = "C:\\";

/// Where the launch working directory came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchCwdSource {
    /// The caller's explicit `process.cwd`.
    Explicit,
    /// The first filesystem-policy grant that is an existing directory.
    Policy,
    /// The drive root, because nothing else was usable.
    DriveRoot,
}

impl LaunchCwdSource {
    /// Short phrase naming the origin, for log lines and launch errors.
    pub fn describe(self) -> &'static str {
        match self {
            Self::Explicit => "process.cwd",
            Self::Policy => "filesystem policy (process.cwd omitted)",
            Self::DriveRoot => "drive-root fallback (no usable process.cwd or policy directory)",
        }
    }
}

/// The current directory to hand to the launch API, and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchWorkingDirectory {
    /// The directory to launch in. **Never empty**, so callers always pass a
    /// non-`NULL` pointer.
    pub path: String,
    /// Origin of `path`, for diagnostics.
    pub source: LaunchCwdSource,
}

impl LaunchWorkingDirectory {
    /// One-line description for logs and error context, e.g.
    /// `C:\workspace (from filesystem policy (process.cwd omitted))`.
    pub fn describe(&self) -> String {
        format!("{} (from {})", self.path, self.source.describe())
    }
}

/// Resolve the current directory for a ProcessContainer launch.
///
/// Precedence: explicit `process.cwd`, else the first filesystem-policy grant
/// that is an existing directory, else the system drive root. The result is
/// never empty — passing `NULL` to `CreateProcessW` /
/// `Experimental_CreateProcessInSandbox` would silently relocate the child.
pub fn launch_working_directory(request: &ExecutionRequest) -> LaunchWorkingDirectory {
    launch_working_directory_with(
        request,
        |path| std::path::Path::new(path).is_dir(),
        &system_drive_root(),
    )
}

/// [`launch_working_directory`] with the filesystem probe and drive root
/// injected, so unit tests cover the mapping without touching the host.
pub fn launch_working_directory_with(
    request: &ExecutionRequest,
    is_dir: impl Fn(&str) -> bool,
    drive_root: &str,
) -> LaunchWorkingDirectory {
    match request.resolved_working_directory_with(is_dir) {
        Some(resolved) => LaunchWorkingDirectory {
            path: resolved.path.to_string(),
            source: match resolved.source {
                WorkingDirectorySource::Explicit => LaunchCwdSource::Explicit,
                WorkingDirectorySource::Policy => LaunchCwdSource::Policy,
            },
        },
        None => LaunchWorkingDirectory {
            path: drive_root.to_string(),
            source: LaunchCwdSource::DriveRoot,
        },
    }
}

/// The system drive root (`%SystemDrive%\`), falling back to `C:\` when the
/// variable is unset or malformed.
fn system_drive_root() -> String {
    match std::env::var("SystemDrive") {
        Ok(drive) if drive.trim().len() >= 2 && drive.trim().ends_with(':') => {
            format!("{}\\", drive.trim())
        }
        _ => DEFAULT_DRIVE_ROOT.to_string(),
    }
}
