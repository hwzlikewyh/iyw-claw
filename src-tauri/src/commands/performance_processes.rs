use std::collections::HashMap;

#[derive(Debug, Clone)]
pub(super) struct ProcessRecord {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub name: String,
    pub command_line: String,
}

#[derive(Debug, Clone)]
pub(super) struct ProcessClassification {
    pub agent_type: Option<String>,
    pub group_id: String,
    pub group_display_name: String,
    pub process_role: String,
}

#[derive(Clone, Copy)]
struct AgentRule {
    key: &'static str,
    label: &'static str,
    executable_names: &'static [&'static str],
    command_markers: &'static [&'static str],
}

struct ClassificationContext<'a> {
    root_pid: u32,
    by_pid: &'a HashMap<u32, &'a ProcessRecord>,
    web_roots: &'a HashMap<u32, ()>,
    agents: &'a HashMap<u32, AgentRule>,
}

const AGENT_RULES: &[AgentRule] = &[
    AgentRule {
        key: "claude_code",
        label: "远山",
        executable_names: &["claude", "claude-code"],
        command_markers: &["claude-code", "@anthropic-ai/claude-code", "claude.cmd"],
    },
    AgentRule {
        key: "codex",
        label: "星河",
        executable_names: &["codex"],
        command_markers: &[
            "codex.cmd",
            "@openai/codex",
            "\\codex\\bin\\",
            "/codex/bin/",
        ],
    },
    AgentRule {
        key: "open_code",
        label: "云舟",
        executable_names: &["opencode", "open-code"],
        command_markers: &["opencode.cmd", "open-code.cmd", "/opencode/"],
    },
    AgentRule {
        key: "gemini",
        label: "流光",
        executable_names: &["gemini"],
        command_markers: &["gemini.cmd", "@google/gemini-cli"],
    },
    AgentRule {
        key: "open_claw",
        label: "开放之爪",
        executable_names: &["openclaw", "open-claw"],
        command_markers: &["openclaw.cmd", "open-claw.cmd"],
    },
    AgentRule {
        key: "cline",
        label: "逐风",
        executable_names: &["cline"],
        command_markers: &["cline.cmd", "/cline/"],
    },
    AgentRule {
        key: "hermes",
        label: "赫尔墨斯",
        executable_names: &["hermes"],
        command_markers: &["hermes.cmd", "/hermes/"],
    },
    AgentRule {
        key: "code_buddy",
        label: "青岚",
        executable_names: &["code-buddy", "codebuddy", "code_buddy"],
        command_markers: &["code-buddy.cmd", "codebuddy.cmd"],
    },
    AgentRule {
        key: "kimi_code",
        label: "月白",
        executable_names: &["kimi", "kimi-code"],
        command_markers: &["kimi.cmd", "kimi-code.cmd"],
    },
    AgentRule {
        key: "pi",
        label: "墨川",
        executable_names: &["pi", "pi-acp"],
        command_markers: &["pi-acp", "pi.cmd"],
    },
    AgentRule {
        key: "grok",
        label: "知微",
        executable_names: &["grok"],
        command_markers: &["grok.cmd", "/grok/"],
    },
];

fn executable_name(name: &str) -> String {
    name.to_lowercase().trim_end_matches(".exe").to_string()
}

fn identify_agent(record: &ProcessRecord) -> Option<AgentRule> {
    let executable = executable_name(&record.name);
    let command = record.command_line.to_lowercase().replace('\\', "/");
    AGENT_RULES.iter().copied().find(|rule| {
        rule.executable_names.contains(&executable.as_str())
            || rule
                .command_markers
                .iter()
                .any(|marker| command.contains(marker))
    })
}

fn is_launch_wrapper(name: &str) -> bool {
    matches!(
        executable_name(name).as_str(),
        "cmd" | "powershell" | "pwsh" | "node" | "bun" | "npx" | "uv" | "python"
    )
}

fn promote_agent_root(pid: u32, root_pid: u32, by_pid: &HashMap<u32, &ProcessRecord>) -> u32 {
    let mut current_pid = pid;
    while let Some(parent_pid) = by_pid.get(&current_pid).and_then(|item| item.parent_pid) {
        if parent_pid == root_pid {
            break;
        }
        let Some(parent) = by_pid.get(&parent_pid) else {
            break;
        };
        if !is_launch_wrapper(&parent.name) {
            break;
        }
        current_pid = parent_pid;
    }
    current_pid
}

fn agent_roots(
    records: &[ProcessRecord],
    root_pid: u32,
    by_pid: &HashMap<u32, &ProcessRecord>,
) -> HashMap<u32, AgentRule> {
    let mut roots = HashMap::new();
    for record in records {
        let Some(agent) = identify_agent(record) else {
            continue;
        };
        let group_root = promote_agent_root(record.pid, root_pid, by_pid);
        roots.entry(group_root).or_insert(agent);
    }
    roots
}

fn ancestor_in<'a, T>(
    pid: u32,
    values: &'a HashMap<u32, T>,
    by_pid: &HashMap<u32, &ProcessRecord>,
) -> Option<(u32, &'a T)> {
    let mut current = Some(pid);
    while let Some(current_pid) = current {
        if let Some(value) = values.get(&current_pid) {
            return Some((current_pid, value));
        }
        current = by_pid.get(&current_pid).and_then(|item| item.parent_pid);
    }
    None
}

fn webview_roots(
    records: &[ProcessRecord],
    by_pid: &HashMap<u32, &ProcessRecord>,
) -> HashMap<u32, ()> {
    records
        .iter()
        .filter(|record| executable_name(&record.name) == "msedgewebview2")
        .filter(|record| {
            record
                .parent_pid
                .and_then(|parent_pid| by_pid.get(&parent_pid))
                .map_or(true, |parent| {
                    executable_name(&parent.name) != "msedgewebview2"
                })
        })
        .map(|record| (record.pid, ()))
        .collect()
}

fn webview_role(record: &ProcessRecord) -> String {
    let command = record.command_line.to_lowercase();
    for role in ["renderer", "gpu-process", "utility", "crashpad-handler"] {
        if command.contains(&format!("--type={role}")) || command.contains(role) {
            return role.to_string();
        }
    }
    "browser".to_string()
}

fn fixed_classification(group_id: &str, group_name: &str, role: &str) -> ProcessClassification {
    ProcessClassification {
        agent_type: None,
        group_id: group_id.to_string(),
        group_display_name: group_name.to_string(),
        process_role: role.to_string(),
    }
}

fn classify_record(
    record: &ProcessRecord,
    context: &ClassificationContext<'_>,
) -> ProcessClassification {
    if record.pid == context.root_pid {
        return fixed_classification("main", "iyw-claw", "main");
    }
    if let Some((web_root, _)) = ancestor_in(record.pid, context.web_roots, context.by_pid) {
        return fixed_classification(
            &format!("webview2-{web_root}"),
            "WebView2",
            &webview_role(record),
        );
    }
    if let Some((agent_root, agent)) = ancestor_in(record.pid, context.agents, context.by_pid) {
        return ProcessClassification {
            agent_type: Some(agent.key.to_string()),
            group_id: format!("agent-{}-{agent_root}", agent.key),
            group_display_name: format!("{}会话", agent.label),
            process_role: if record.pid == agent_root {
                "launcher"
            } else {
                "child"
            }
            .to_string(),
        };
    }
    fixed_classification("other", "其他附属进程", "child")
}

pub(super) fn classify_processes(
    records: &[ProcessRecord],
    root_pid: u32,
) -> HashMap<u32, ProcessClassification> {
    let by_pid: HashMap<_, _> = records.iter().map(|record| (record.pid, record)).collect();
    let web_roots = webview_roots(records, &by_pid);
    let agents = agent_roots(records, root_pid, &by_pid);
    let context = ClassificationContext {
        root_pid,
        by_pid: &by_pid,
        web_roots: &web_roots,
        agents: &agents,
    };
    records
        .iter()
        .map(|record| (record.pid, classify_record(record, &context)))
        .collect()
}
