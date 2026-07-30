use serde::Serialize;
use sysinfo::System;

#[derive(Debug, Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct SystemPerformanceStats {
    pub cpu_usage: f32,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub os_info: OsInfo,
    pub processes: Vec<AgentProcessInfo>,
}

#[derive(Debug, Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct OsInfo {
    pub os_name: String,
    pub arch: String,
    pub cpu_count: usize,
    pub uptime_secs: u64,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AgentProcessInfo {
    pub pid: u32,
    pub display_name: String,
    pub agent_type: Option<String>,
    pub cpu_usage: f32,
    pub memory_bytes: u64,
    pub status: String,
}

/// 识别已知的智能体进程，返回 (agent_type_key, 中文名)
fn identify_agent(name: &str, cmd: &str) -> Option<(&'static str, &'static str)> {
    let haystack = format!("{} {}", name, cmd).to_lowercase();
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
    if haystack.contains("grok") {
        return Some(("grok", "知微"));
    }
    None
}

fn collect_stats() -> SystemPerformanceStats {
    let mut sys = System::new_all();
    // 首次刷新建立基线，休眠后再次刷新以获得有意义的 CPU 读数
    sys.refresh_all();
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_all();

    let cpu_usage = sys.global_cpu_info().cpu_usage();
    let memory_used = sys.used_memory();
    let memory_total = sys.total_memory();

    let os_name = System::os_version().unwrap_or_else(|| "Unknown".to_string());
    let arch = std::env::consts::ARCH.to_string();
    let cpu_count = sys.cpus().len();
    let uptime_secs = System::uptime();

    let self_pid = std::process::id();

    let mut processes: Vec<AgentProcessInfo> = Vec::new();

    for (pid, process) in sys.processes() {
        let pid_u32 = usize::from(*pid) as u32;
        let name = process.name().to_string();
        let cmd = process
            .cmd()
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        let status = {
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
        };

        if pid_u32 == self_pid {
            processes.push(AgentProcessInfo {
                pid: pid_u32,
                display_name: "iyw-claw".to_string(),
                agent_type: None,
                cpu_usage: process.cpu_usage(),
                memory_bytes: process.memory(),
                status,
            });
        } else if let Some((agent_type, display_name)) = identify_agent(&name, &cmd) {
            processes.push(AgentProcessInfo {
                pid: pid_u32,
                display_name: display_name.to_string(),
                agent_type: Some(agent_type.to_string()),
                cpu_usage: process.cpu_usage(),
                memory_bytes: process.memory(),
                status,
            });
        }
    }

    // 排序：自身进程优先，其余按内存降序
    processes.sort_by(
        |a, b| match (a.agent_type.is_none(), b.agent_type.is_none()) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => b.memory_bytes.cmp(&a.memory_bytes),
        },
    );

    SystemPerformanceStats {
        cpu_usage,
        memory_used_bytes: memory_used,
        memory_total_bytes: memory_total,
        os_info: OsInfo {
            os_name,
            arch,
            cpu_count,
            uptime_secs,
        },
        processes,
    }
}

pub async fn get_performance_stats_core() -> SystemPerformanceStats {
    tokio::task::spawn_blocking(collect_stats)
        .await
        .unwrap_or_default()
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_performance_stats() -> SystemPerformanceStats {
    get_performance_stats_core().await
}
