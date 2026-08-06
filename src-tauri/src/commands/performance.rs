use serde::Serialize;
use std::collections::HashSet;
use sysinfo::{Pid, Process, System};

#[derive(Debug, Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppPerformanceStats {
    pub cpu_usage: f32,
    pub memory_used_bytes: u64,
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
    pub display_name: String,
    pub agent_type: Option<String>,
    pub is_main_process: bool,
    pub cpu_usage: f32,
    pub memory_bytes: u64,
    pub status: String,
}

#[derive(Clone, Copy)]
struct PerformanceScope {
    root_pid: Pid,
    cpu_count: f32,
}

/// 识别已知的智能体进程，返回 (agent_type_key, 中文名)
fn identify_agent(name: &str, cmd: &str) -> Option<(&'static str, &'static str)> {
    let haystack = format!("{} {}", name, cmd).to_lowercase();
    let normalized_name = name.to_lowercase();
    let executable_name = normalized_name.trim_end_matches(".exe");
    // 按优先级匹配，避免误判
    if haystack.contains("claude-code")
        || (haystack.contains("claude") && haystack.contains("code"))
    {
        return Some(("claude_code", "远山"));
    }
    if haystack.contains("codex") {
        return Some(("codex", "星河"));
    }
    if haystack.contains("opencode") || haystack.contains("open-code") {
        return Some(("open_code", "云舟"));
    }
    if haystack.contains("gemini") {
        return Some(("gemini", "流光"));
    }
    if haystack.contains("openclaw") || haystack.contains("open-claw") {
        return Some(("open_claw", "开放之爪"));
    }
    if haystack.contains("cline") {
        return Some(("cline", "逐风"));
    }
    if haystack.contains("hermes") {
        return Some(("hermes", "赫尔墨斯"));
    }
    if haystack.contains("code-buddy")
        || haystack.contains("codebuddy")
        || haystack.contains("code_buddy")
    {
        return Some(("code_buddy", "青岚"));
    }
    if haystack.contains("kimi") {
        return Some(("kimi_code", "月白"));
    }
    if haystack.contains("pi-acp") || executable_name == "pi" {
        return Some(("pi", "墨川"));
    }
    if haystack.contains("grok") {
        return Some(("grok", "知微"));
    }
    None
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
            if process
                .parent()
                .is_some_and(|parent| scoped_pids.contains(&parent))
                && scoped_pids.insert(*pid)
            {
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

fn process_info(pid: Pid, process: &Process, scope: PerformanceScope) -> AppProcessInfo {
    let name = process.name();
    let cmd = process.cmd().join(" ");
    let is_main_process = pid == scope.root_pid;
    let (agent_type, display_name) = if is_main_process {
        (None, "iyw-claw".to_string())
    } else if let Some((agent_type, display_name)) = identify_agent(name, &cmd) {
        (Some(agent_type.to_string()), display_name.to_string())
    } else {
        (None, name.to_string())
    };
    AppProcessInfo {
        pid: pid.as_u32(),
        display_name,
        agent_type,
        is_main_process,
        cpu_usage: process.cpu_usage() / scope.cpu_count,
        memory_bytes: process.memory(),
        status: process_status(process),
    }
}

fn collect_stats() -> AppPerformanceStats {
    let mut sys = System::new_all();
    // 首次刷新建立基线，休眠后再次刷新以获得有意义的 CPU 读数
    sys.refresh_all();
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_all();

    let os_name = System::os_version().unwrap_or_else(|| "Unknown".to_string());
    let arch = std::env::consts::ARCH.to_string();
    let cpu_count = sys.cpus().len().max(1) as f32;
    let root_pid = Pid::from_u32(std::process::id());
    let scope = PerformanceScope {
        root_pid,
        cpu_count,
    };
    let scoped_pids = collect_descendant_pids(&sys, root_pid);
    let mut processes: Vec<AppProcessInfo> = sys
        .processes()
        .iter()
        .filter_map(|(pid, process)| {
            scoped_pids
                .contains(pid)
                .then(|| process_info(*pid, process, scope))
        })
        .collect();

    let cpu_usage = processes.iter().map(|process| process.cpu_usage).sum();
    let memory_used = processes.iter().map(|process| process.memory_bytes).sum();

    // 排序：自身进程优先，其余按内存降序
    processes.sort_by(|a, b| match (a.is_main_process, b.is_main_process) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => b.memory_bytes.cmp(&a.memory_bytes),
    });

    AppPerformanceStats {
        cpu_usage,
        memory_used_bytes: memory_used,
        os_info: OsInfo { os_name, arch },
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
