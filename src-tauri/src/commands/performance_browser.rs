use std::collections::HashMap;
use std::path::Path;

use sysinfo::{Pid, System};

use crate::browser::ManagedBrowserProcessSnapshot;

use super::{AppProcessInfo, ProcessClassification, ProcessRecord};

pub(super) fn validated_root_pid(
    system: &System,
    snapshot: &ManagedBrowserProcessSnapshot,
) -> Option<Pid> {
    let pid = Pid::from_u32(snapshot.pid);
    let process = system.process(pid)?;
    (process.start_time() == snapshot.started_at
        && process
            .exe()
            .is_some_and(|actual| same_path(actual, &snapshot.executable)))
    .then_some(pid)
}

pub(super) fn apply_classifications(
    records: &[ProcessRecord],
    root_pid: u32,
    classifications: &mut HashMap<u32, ProcessClassification>,
) {
    let by_pid: HashMap<_, _> = records.iter().map(|record| (record.pid, record)).collect();
    for record in records {
        if !belongs_to_root(record.pid, root_pid, &by_pid) {
            continue;
        }
        classifications.insert(
            record.pid,
            ProcessClassification {
                agent_type: None,
                group_id: format!("managed-browser-{root_pid}"),
                group_display_name: "内置浏览器".to_string(),
                process_role: browser_role(record, root_pid),
            },
        );
    }
}

pub(super) fn log_included_sample(root_pid: Pid, processes: &[AppProcessInfo]) {
    let group_id = format!("managed-browser-{}", root_pid.as_u32());
    let process_count = processes
        .iter()
        .filter(|process| process.group_id.as_deref() == Some(&group_id))
        .count();
    tracing::debug!(
        daemon_pid = root_pid.as_u32(),
        process_count,
        "performance sample included managed browser"
    );
}

fn belongs_to_root(mut pid: u32, root_pid: u32, by_pid: &HashMap<u32, &ProcessRecord>) -> bool {
    loop {
        if pid == root_pid {
            return true;
        }
        let Some(parent_pid) = by_pid.get(&pid).and_then(|record| record.parent_pid) else {
            return false;
        };
        pid = parent_pid;
    }
}

fn browser_role(record: &ProcessRecord, root_pid: u32) -> String {
    if record.pid == root_pid {
        return "controller".to_string();
    }
    let command = record.command_line.to_lowercase();
    for role in ["renderer", "gpu-process", "utility", "crashpad-handler"] {
        if command.contains(&format!("--type={role}")) || command.contains(role) {
            return role.to_string();
        }
    }
    "browser".to_string()
}

fn same_path(left: &Path, right: &Path) -> bool {
    let left = std::fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf());
    let right = std::fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());
    if cfg!(target_os = "windows") {
        return left
            .to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy());
    }
    left == right
}
