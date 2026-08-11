use serde::Serialize;
use std::collections::{HashMap, HashSet};
use sysinfo::{Pid, Process, System};

#[path = "performance_processes.rs"]
mod performance_processes;
#[cfg(target_os = "windows")]
#[path = "performance_windows.rs"]
mod performance_windows;

use performance_processes::{classify_processes, ProcessClassification, ProcessRecord};

const PERFORMANCE_SAMPLE_INTERVAL_MS: u64 = 200;

#[derive(Debug, Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppPerformanceStats {
    pub cpu_usage: f32,
    pub memory_used_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_memory_used_bytes: Option<u64>,
    pub os_info: OsInfo,
    pub processes: Vec<AppProcessInfo>,
}

#[derive(Debug, Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct OsInfo {
    pub os_name: String,
    pub arch: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppProcessInfo {
    pub pid: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_pid: Option<u32>,
    pub display_name: String,
    pub agent_type: Option<String>,
    pub is_main_process: bool,
    pub cpu_usage: f32,
    pub memory_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_memory_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_role: Option<String>,
    pub status: String,
}

struct PerformanceScope<'a> {
    root_pid: Pid,
    cpu_count: f32,
    classifications: &'a HashMap<u32, ProcessClassification>,
}

fn collect_descendant_pids(sys: &System, root_pid: Pid) -> HashSet<Pid> {
    let mut scoped_pids = HashSet::from([root_pid]);
    let mut changed = true;
    while changed {
        changed = false;
        for (pid, process) in sys.processes() {
            if scoped_pids.contains(pid) {
                continue;
            }
            let is_descendant = process
                .parent()
                .is_some_and(|parent| scoped_pids.contains(&parent));
            if is_descendant && scoped_pids.insert(*pid) {
                changed = true;
            }
        }
    }
    scoped_pids
}

fn process_status(process: &Process) -> String {
    let raw = format!("{:?}", process.status());
    if raw.contains("Run") {
        "运行中".to_string()
    } else if raw.contains("Sleep") || raw.contains("Idle") {
        "空闲".to_string()
    } else if raw.contains("Stop") {
        "停止".to_string()
    } else if raw.contains("Zombie") {
        "僵死".to_string()
    } else {
        raw
    }
}

#[cfg(target_os = "windows")]
fn private_memory_bytes(pid: u32) -> Option<u64> {
    performance_windows::private_commit_bytes(pid)
}

#[cfg(not(target_os = "windows"))]
fn private_memory_bytes(_pid: u32) -> Option<u64> {
    None
}

fn collect_records(sys: &System, scoped_pids: &HashSet<Pid>) -> Vec<ProcessRecord> {
    sys.processes()
        .iter()
        .filter(|(pid, _)| scoped_pids.contains(pid))
        .map(|(pid, process)| ProcessRecord {
            pid: pid.as_u32(),
            parent_pid: process.parent().map(|parent| parent.as_u32()),
            name: process.name().to_string(),
            command_line: process.cmd().join(" "),
        })
        .collect()
}

fn process_info(pid: Pid, process: &Process, scope: &PerformanceScope<'_>) -> AppProcessInfo {
    let raw_pid = pid.as_u32();
    let classification = scope.classifications.get(&raw_pid);
    AppProcessInfo {
        pid: raw_pid,
        parent_pid: process.parent().map(|parent| parent.as_u32()),
        display_name: process.name().to_string(),
        agent_type: classification.and_then(|item| item.agent_type.clone()),
        is_main_process: pid == scope.root_pid,
        cpu_usage: process.cpu_usage() / scope.cpu_count,
        memory_bytes: process.memory(),
        private_memory_bytes: private_memory_bytes(raw_pid),
        group_id: classification.map(|item| item.group_id.clone()),
        group_display_name: classification.map(|item| item.group_display_name.clone()),
        process_role: classification.map(|item| item.process_role.clone()),
        status: process_status(process),
    }
}

fn sort_processes(processes: &mut [AppProcessInfo]) {
    processes.sort_by(
        |left, right| match (left.is_main_process, right.is_main_process) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => left
                .group_id
                .cmp(&right.group_id)
                .then_with(|| right.memory_bytes.cmp(&left.memory_bytes)),
        },
    );
}

fn collect_stats() -> AppPerformanceStats {
    let mut sys = System::new_all();
    sys.refresh_all();
    std::thread::sleep(std::time::Duration::from_millis(
        PERFORMANCE_SAMPLE_INTERVAL_MS,
    ));
    sys.refresh_all();

    let root_pid = Pid::from_u32(std::process::id());
    let scoped_pids = collect_descendant_pids(&sys, root_pid);
    let records = collect_records(&sys, &scoped_pids);
    let classifications = classify_processes(&records, root_pid.as_u32());
    let scope = PerformanceScope {
        root_pid,
        cpu_count: sys.cpus().len().max(1) as f32,
        classifications: &classifications,
    };
    let mut processes: Vec<_> = sys
        .processes()
        .iter()
        .filter(|(pid, _)| scoped_pids.contains(pid))
        .map(|(pid, process)| process_info(*pid, process, &scope))
        .collect();
    sort_processes(&mut processes);

    let private_values: Vec<u64> = processes
        .iter()
        .filter_map(|process| process.private_memory_bytes)
        .collect();
    AppPerformanceStats {
        cpu_usage: processes.iter().map(|process| process.cpu_usage).sum(),
        memory_used_bytes: processes.iter().map(|process| process.memory_bytes).sum(),
        private_memory_used_bytes: (!private_values.is_empty())
            .then(|| private_values.iter().sum()),
        os_info: OsInfo {
            os_name: System::os_version().unwrap_or_else(|| "Unknown".to_string()),
            arch: std::env::consts::ARCH.to_string(),
        },
        processes,
    }
}

pub async fn get_performance_stats_core() -> AppPerformanceStats {
    tokio::task::spawn_blocking(collect_stats)
        .await
        .unwrap_or_default()
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_performance_stats() -> AppPerformanceStats {
    get_performance_stats_core().await
}
