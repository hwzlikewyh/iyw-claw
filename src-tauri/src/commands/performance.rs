use std::collections::{HashMap, HashSet};
use sysinfo::{Pid, Process, System};

use crate::acp::manager::ConnectionManager;
use crate::acp::resource_governor::RuntimeSessionSnapshot;
#[cfg(feature = "tauri-runtime")]
use crate::app_error::AppCommandError;
use crate::db::service::conversation_service;
#[cfg(feature = "tauri-runtime")]
use crate::db::AppDatabase;
#[cfg(feature = "tauri-runtime")]
use tauri::State;

#[path = "performance_models.rs"]
mod performance_models;
#[path = "performance_processes.rs"]
mod performance_processes;
#[path = "performance_sessions.rs"]
mod performance_sessions;
#[cfg(target_os = "windows")]
#[path = "performance_windows.rs"]
pub(crate) mod performance_windows;

pub use performance_models::{
    AppAgentSessionInfo, AppPerformanceStats, AppProcessInfo, AppSystemMemoryInfo, OsInfo,
};
use performance_processes::{classify_processes, ProcessClassification, ProcessRecord};
use performance_sessions::{apply_runtime_classifications, collect_agent_sessions};

const PERFORMANCE_SAMPLE_INTERVAL_MS: u64 = 200;

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

pub(super) fn complete_private_memory<'a>(
    processes: impl Iterator<Item = &'a AppProcessInfo>,
) -> Option<u64> {
    let mut total = 0;
    let mut sampled = false;
    for process in processes {
        sampled = true;
        total += process.private_memory_bytes?;
    }
    sampled.then_some(total)
}

fn system_memory_info(sys: &System) -> AppSystemMemoryInfo {
    let memory = crate::acp::resource_governor::system_memory_snapshot(
        sys.total_memory(),
        sys.available_memory(),
    );
    AppSystemMemoryInfo {
        total_bytes: memory.total_bytes,
        available_bytes: memory.available_bytes,
        pressure: memory.pressure.as_str().to_string(),
        shrinking_reserve_bytes: memory.shrinking_reserve_bytes,
        emergency_reserve_bytes: memory.emergency_reserve_bytes,
        idle_agent_budget_bytes: crate::acp::resource_governor::idle_private_budget(
            memory.total_bytes,
        ),
    }
}

fn collect_processes(sys: &System, sessions: &[RuntimeSessionSnapshot]) -> Vec<AppProcessInfo> {
    let root_pid = Pid::from_u32(std::process::id());
    let scoped_pids = collect_descendant_pids(sys, root_pid);
    let records = collect_records(sys, &scoped_pids);
    let mut classifications = classify_processes(&records, root_pid.as_u32());
    apply_runtime_classifications(&records, sessions, &mut classifications);
    let scope = PerformanceScope {
        root_pid,
        cpu_count: sys.cpus().len().max(1) as f32,
        classifications: &classifications,
    };
    let mut processes = sys
        .processes()
        .iter()
        .filter(|(pid, _)| scoped_pids.contains(pid))
        .map(|(pid, process)| process_info(*pid, process, &scope))
        .collect::<Vec<_>>();
    sort_processes(&mut processes);
    processes
}

fn collect_stats(sessions: Vec<RuntimeSessionSnapshot>) -> AppPerformanceStats {
    let mut sys = System::new_all();
    sys.refresh_all();
    std::thread::sleep(std::time::Duration::from_millis(
        PERFORMANCE_SAMPLE_INTERVAL_MS,
    ));
    sys.refresh_all();

    let system_memory = Some(system_memory_info(&sys));
    let processes = collect_processes(&sys, &sessions);
    let private_memory_used_bytes = complete_private_memory(processes.iter());
    let agent_sessions = collect_agent_sessions(&processes, sessions);
    AppPerformanceStats {
        cpu_usage: processes.iter().map(|process| process.cpu_usage).sum(),
        memory_used_bytes: processes.iter().map(|process| process.memory_bytes).sum(),
        private_memory_used_bytes,
        os_info: OsInfo {
            os_name: System::os_version().unwrap_or_else(|| "Unknown".to_string()),
            arch: std::env::consts::ARCH.to_string(),
        },
        processes,
        agent_sessions,
        system_memory,
    }
}

pub async fn get_performance_stats_core(
    manager: &ConnectionManager,
    db: &sea_orm::DatabaseConnection,
) -> AppPerformanceStats {
    let sessions = manager.runtime_session_snapshots().await;
    let mut stats = tokio::task::spawn_blocking(move || collect_stats(sessions))
        .await
        .unwrap_or_default();
    for session in &mut stats.agent_sessions {
        let Some(conversation_id) = session.conversation_id else {
            continue;
        };
        if let Ok(conversation) = conversation_service::get_by_id(db, conversation_id).await {
            session.conversation_title = conversation.title;
        }
    }
    stats
}

pub async fn end_agent_runtime_session_core(
    manager: &ConnectionManager,
    connection_id: &str,
) -> Result<bool, crate::acp::error::AcpError> {
    manager
        .disconnect_if_reclaimable(
            connection_id,
            crate::acp::resource_governor::completion_grace(),
            false,
            "performance_page",
        )
        .await
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
#[cfg(feature = "tauri-runtime")]
pub async fn get_performance_stats(
    manager: State<'_, ConnectionManager>,
    db: State<'_, AppDatabase>,
) -> Result<AppPerformanceStats, AppCommandError> {
    Ok(get_performance_stats_core(&manager, &db.conn).await)
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
#[cfg(feature = "tauri-runtime")]
pub async fn end_agent_runtime_session(
    connection_id: String,
    manager: State<'_, ConnectionManager>,
) -> Result<bool, crate::acp::error::AcpError> {
    end_agent_runtime_session_core(&manager, &connection_id).await
}
