use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{Instant, SystemTime};

use crate::models::agent::AgentType;

use super::agent_retention_policy::{AgentLogRule, AgentLogTarget};

pub(super) struct LogFile {
    pub path: PathBuf,
    pub size: u64,
    pub modified: SystemTime,
    pub primary: bool,
}

pub(super) struct LogGroup {
    pub agent_type: AgentType,
    pub group: &'static str,
    pub allowed_dir: PathBuf,
    pub files: Vec<LogFile>,
    pub reset_only: bool,
}

#[derive(Default)]
pub(super) struct AgentLogScanResult {
    pub groups: Vec<LogGroup>,
    pub scanned_agents: usize,
    pub scanned_files: usize,
    pub total_bytes: u64,
    pub failed_files: usize,
    pub first_error: Option<String>,
    pub timed_out: bool,
}

struct TargetScan {
    groups: Vec<LogGroup>,
    timed_out: bool,
}

pub(super) fn collect_groups(
    targets: Vec<AgentLogTarget>,
    deadline: Instant,
) -> AgentLogScanResult {
    let mut agents = BTreeSet::new();
    let mut result = AgentLogScanResult::default();
    for target in targets {
        if Instant::now() >= deadline {
            result.timed_out = true;
            break;
        }
        match scan_target(&target, deadline) {
            Ok(found) => {
                append_groups(&mut result, &mut agents, target.agent_type, found.groups);
                if found.timed_out {
                    result.timed_out = true;
                    break;
                }
            }
            Err(error) => record_scan_failure(&mut result, &target, error),
        }
    }
    result.scanned_agents = agents.len();
    result
}

pub(super) fn remove_file(allowed_dir: &Path, file: &LogFile) -> io::Result<bool> {
    let metadata = match std::fs::symlink_metadata(&file.path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if !metadata.is_file() || is_unsafe_link(&metadata) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "log target changed type",
        ));
    }
    let path = std::fs::canonicalize(&file.path)?;
    if path.parent() != Some(allowed_dir) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "log target escaped root",
        ));
    }
    std::fs::remove_file(path).map(|()| true)
}

fn append_groups(
    result: &mut AgentLogScanResult,
    agents: &mut BTreeSet<AgentType>,
    agent_type: AgentType,
    groups: Vec<LogGroup>,
) {
    if !groups.is_empty() {
        agents.insert(agent_type);
    }
    for group in groups {
        result.scanned_files += group.files.len();
        result.total_bytes = group.files.iter().fold(result.total_bytes, |sum, file| {
            sum.saturating_add(file.size)
        });
        result.groups.push(group);
    }
}

fn record_scan_failure(result: &mut AgentLogScanResult, target: &AgentLogTarget, error: io::Error) {
    result.failed_files += 1;
    if result.first_error.is_none() {
        result.first_error = Some(format!("{} {}: {error}", target.agent_type, target.group));
    }
}

fn scan_target(target: &AgentLogTarget, deadline: Instant) -> io::Result<TargetScan> {
    let Some(directory) = safe_directory(&target.profile_root, &target.path)? else {
        return Ok(TargetScan {
            groups: Vec::new(),
            timed_out: false,
        });
    };
    match target.rule {
        AgentLogRule::CodexDatabase => scan_codex(target, directory, deadline),
        AgentLogRule::Extensions(_) => scan_directory(target, directory, deadline),
    }
}

fn scan_codex(
    target: &AgentLogTarget,
    directory: PathBuf,
    deadline: Instant,
) -> io::Result<TargetScan> {
    let mut groups: BTreeMap<String, Vec<LogFile>> = BTreeMap::new();
    let mut timed_out = false;
    for entry in std::fs::read_dir(&directory)? {
        if Instant::now() >= deadline {
            timed_out = true;
            break;
        }
        let Some((name, file)) = accepted_file(entry?, target, &directory)? else {
            continue;
        };
        let Some(group) = target.codex_group(&name) else {
            continue;
        };
        groups.entry(group.clone()).or_default().push(LogFile {
            primary: name == group,
            ..file
        });
    }
    Ok(TargetScan {
        groups: groups
            .into_values()
            .map(|files| log_group(target, directory.clone(), files, true))
            .collect(),
        timed_out,
    })
}

fn scan_directory(
    target: &AgentLogTarget,
    directory: PathBuf,
    deadline: Instant,
) -> io::Result<TargetScan> {
    let mut groups = Vec::new();
    let mut timed_out = false;
    for entry in std::fs::read_dir(&directory)? {
        if Instant::now() >= deadline {
            timed_out = true;
            break;
        }
        let Some((_name, file)) = accepted_file(entry?, target, &directory)? else {
            continue;
        };
        groups.push(log_group(target, directory.clone(), vec![file], false));
    }
    Ok(TargetScan { groups, timed_out })
}

fn log_group(
    target: &AgentLogTarget,
    allowed_dir: PathBuf,
    files: Vec<LogFile>,
    reset_only: bool,
) -> LogGroup {
    LogGroup {
        agent_type: target.agent_type,
        group: target.group,
        allowed_dir,
        files,
        reset_only,
    }
}

fn accepted_file(
    entry: std::fs::DirEntry,
    target: &AgentLogTarget,
    directory: &Path,
) -> io::Result<Option<(String, LogFile)>> {
    let name = entry.file_name().to_string_lossy().into_owned();
    if !target.accepts_file(&name) {
        return Ok(None);
    }
    let metadata = std::fs::symlink_metadata(entry.path())?;
    if !metadata.is_file() || is_unsafe_link(&metadata) {
        return Ok(None);
    }
    let path = std::fs::canonicalize(entry.path())?;
    if path.parent() != Some(directory) {
        return Ok(None);
    }
    Ok(Some((
        name,
        LogFile {
            path,
            size: metadata.len(),
            modified: metadata.modified()?,
            primary: false,
        },
    )))
}

fn safe_directory(profile_root: &Path, target: &Path) -> io::Result<Option<PathBuf>> {
    let Some(root_meta) = directory_metadata(profile_root)? else {
        return Ok(None);
    };
    if is_unsafe_link(&root_meta) {
        return Ok(None);
    }
    let Ok(relative) = target.strip_prefix(profile_root) else {
        return Ok(None);
    };
    let mut current = profile_root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(segment) = component else {
            return Ok(None);
        };
        current.push(segment);
        let Some(metadata) = directory_metadata(&current)? else {
            return Ok(None);
        };
        if is_unsafe_link(&metadata) {
            return Ok(None);
        }
    }
    let root = std::fs::canonicalize(profile_root)?;
    let directory = std::fs::canonicalize(target)?;
    Ok(directory.starts_with(root).then_some(directory))
}

fn is_unsafe_link(metadata: &std::fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        return metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    }
    #[cfg(not(windows))]
    false
}

fn directory_metadata(path: &Path) -> io::Result<Option<std::fs::Metadata>> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(Some(metadata)),
        Ok(_) => Ok(None),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}
