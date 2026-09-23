//! Resolve sandbox helpers and materialize legacy executables with inherited sandbox ACLs.
//! An explicit registered-runtime request never falls through to copying or PATH lookup;
//! the service and startup handshake independently verify the installed image.

mod copy;
use copy::CopyOutcome;
use copy::copy_from_source_if_needed;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use crate::app_package::registered_core_requested;
use crate::logging::log_note;
use crate::sandbox_bin_dir;
use crate::setup::SetupRuntime;

const DEV_BUILD_VERSION_SENTINEL: &str = "0.0.0";
const COMMAND_RUNNER_EXE: &str = "xinghe-command-runner.exe";
pub(crate) const BIN_DIRNAME: &str = "bin";
pub(crate) const RESOURCES_DIRNAME: &str = "xinghe-resources";

pub(crate) fn helper_bin_dir(codex_home: &Path) -> PathBuf {
    sandbox_bin_dir(codex_home)
}

fn legacy_lookup() -> PathBuf {
    if let Ok(exe) = std::env::current_exe()
        && let Some(candidate) = bundled_executable_path_for_exe(&exe, COMMAND_RUNNER_EXE)
    {
        return candidate;
    }
    PathBuf::from(COMMAND_RUNNER_EXE)
}

pub(crate) fn resolve_command_runner(codex_home: &Path, log_dir: Option<&Path>) -> Result<PathBuf> {
    if registered_core_requested() {
        let exe = std::env::current_exe().context("resolve registered Core helper source")?;
        let direct_path = exe.with_file_name(COMMAND_RUNNER_EXE);
        log_note(
            &format!(
                "helper launch resolution: using app-contained command-runner path {}",
                direct_path.display()
            ),
            log_dir,
        );
        // Missing packaged helpers must fail rather than search PATH or create a copy.
        return Ok(direct_path);
    }
    Ok(match copy_runner_if_needed(codex_home, log_dir) {
        Ok(path) => {
            log_note(
                &format!(
                    "helper launch resolution: using copied command-runner path {}",
                    path.display()
                ),
                log_dir,
            );
            path
        }
        Err(err) => {
            let fallback = legacy_lookup();
            log_note(
                &format!(
                    "helper copy failed for command-runner: {err:#}; falling back to legacy path {}",
                    fallback.display()
                ),
                log_dir,
            );
            fallback
        }
    })
}

pub fn resolve_exe_for_launch(source: &Path, codex_home: &Path) -> PathBuf {
    let runtime = crate::setup::current_setup_runtime();
    resolve_exe_for_runtime(source, codex_home, runtime)
}

fn resolve_exe_for_runtime(source: &Path, codex_home: &Path, runtime: SetupRuntime) -> PathBuf {
    let sandbox_log_dir = crate::sandbox_dir(codex_home);
    if runtime == SetupRuntime::Registered {
        log_note(
            &format!(
                "helper executable resolution: route=direct source={} selected={}",
                source.display(),
                source.display()
            ),
            Some(&sandbox_log_dir),
        );
        return source.to_path_buf();
    }
    let Some(file_name) = source.file_name() else {
        return source.to_path_buf();
    };
    let destination = helper_bin_dir(codex_home).join(file_name);
    match copy_from_source_if_needed(source, &destination) {
        Ok(_) => {
            log_note(
                &format!(
                    "helper executable resolution: route=materialized source={} selected={}",
                    source.display(),
                    destination.display()
                ),
                Some(&sandbox_log_dir),
            );
            destination
        }
        Err(err) => {
            log_note(
                &format!(
                    "helper copy failed for executable: {err:#}; falling back to legacy path {}",
                    source.display()
                ),
                Some(&sandbox_log_dir),
            );
            source.to_path_buf()
        }
    }
}

fn copy_runner_if_needed(codex_home: &Path, log_dir: Option<&Path>) -> Result<PathBuf> {
    let source = sibling_source_path()?;
    let suffix = helper_version_suffix(&source)?;
    let destination = helper_bin_dir(codex_home).join(materialized_file_name(&suffix));
    log_note(
        &format!(
            "helper copy: validating command-runner source={} destination={}",
            source.display(),
            destination.display()
        ),
        log_dir,
    );
    let outcome = copy_from_source_if_needed(&source, &destination)?;
    let action = match outcome {
        CopyOutcome::Reused => "reused",
        CopyOutcome::ReCopied => "recopied",
    };
    log_note(
        &format!(
            "helper copy: {} command-runner source={} destination={}",
            action,
            source.display(),
            destination.display()
        ),
        log_dir,
    );
    Ok(destination)
}

fn sibling_source_path() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("resolve current executable for helper lookup")?;
    bundled_executable_path_for_exe(&exe, COMMAND_RUNNER_EXE).ok_or_else(|| {
        anyhow!(
            "helper not found next to current executable or under {RESOURCES_DIRNAME}: {}",
            exe.display()
        )
    })
}

pub(crate) fn bundled_executable_path_for_exe(exe: &Path, file_name: &str) -> Option<PathBuf> {
    let find = |exe: &Path| {
        let dir = exe.parent()?;
        let direct_candidate = dir.join(file_name);
        if direct_candidate.is_file() {
            return Some(direct_candidate);
        }

        if dir.file_name() == Some(OsStr::new(BIN_DIRNAME))
            && let Some(package_dir) = dir.parent()
        {
            let package_resource_candidate = package_dir.join(RESOURCES_DIRNAME).join(file_name);
            if package_resource_candidate.is_file() {
                return Some(package_resource_candidate);
            }
        }

        let resource_candidate = dir.join(RESOURCES_DIRNAME).join(file_name);
        resource_candidate.is_file().then_some(resource_candidate)
    };

    // Installer bin directories can be junctions, so retry beside the real executable once.
    find(exe).or_else(|| find(&dunce::canonicalize(exe).ok()?))
}

fn materialized_file_name(suffix: &str) -> String {
    format!("codex-command-runner-{suffix}.exe")
}

fn helper_version_suffix(source: &Path) -> Result<String> {
    let version = env!("CARGO_PKG_VERSION");
    if version == DEV_BUILD_VERSION_SENTINEL {
        dev_build_suffix(source)
    } else {
        Ok(version.to_string())
    }
}

fn dev_build_suffix(source: &Path) -> Result<String> {
    let metadata = fs::metadata(source)
        .with_context(|| format!("read helper source metadata {}", source.display()))?;
    let modified = metadata
        .modified()
        .with_context(|| format!("read helper source mtime {}", source.display()))?;
    let duration = modified
        .duration_since(UNIX_EPOCH)
        .with_context(|| format!("convert helper source mtime {}", source.display()))?;
    Ok(format!("{}-{:x}", metadata.len(), duration.as_secs(),))
}
