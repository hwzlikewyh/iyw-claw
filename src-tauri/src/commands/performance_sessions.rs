use std::collections::HashMap;

use crate::acp::resource_governor::RuntimeSessionSnapshot;
use crate::models::agent::AgentType;

use super::{
    complete_private_memory, AppAgentSessionInfo, AppProcessInfo, ProcessClassification,
    ProcessRecord,
};

fn agent_key(agent_type: AgentType) -> String {
    agent_type.as_wire().into_owned()
}

fn has_ancestor(mut pid: u32, ancestor: u32, by_pid: &HashMap<u32, &ProcessRecord>) -> bool {
    while let Some(parent) = by_pid.get(&pid).and_then(|record| record.parent_pid) {
        if parent == ancestor {
            return true;
        }
        pid = parent;
    }
    false
}

pub(super) fn apply_runtime_classifications(
    records: &[ProcessRecord],
    sessions: &[RuntimeSessionSnapshot],
    classifications: &mut HashMap<u32, ProcessClassification>,
) {
    let by_pid: HashMap<_, _> = records.iter().map(|record| (record.pid, record)).collect();
    for session in sessions {
        let Some(root_pid) = session.launcher_pid else {
            continue;
        };
        for record in records {
            if record.pid != root_pid && !has_ancestor(record.pid, root_pid, &by_pid) {
                continue;
            }
            classifications.insert(
                record.pid,
                ProcessClassification {
                    agent_type: Some(agent_key(session.agent_type)),
                    group_id: format!("connection-{}", session.connection_id),
                    group_display_name: session.agent_type.to_string(),
                    process_role: if record.pid == root_pid {
                        "launcher"
                    } else {
                        "child"
                    }
                    .to_string(),
                },
            );
        }
    }
}

pub(super) fn collect_agent_sessions(
    processes: &[AppProcessInfo],
    sessions: Vec<RuntimeSessionSnapshot>,
) -> Vec<AppAgentSessionInfo> {
    sessions
        .into_iter()
        .map(|session| {
            let group_id = format!("connection-{}", session.connection_id);
            let group = processes
                .iter()
                .filter(|process| process.group_id.as_deref() == Some(&group_id))
                .collect::<Vec<_>>();
            AppAgentSessionInfo {
                connection_id: session.connection_id,
                conversation_id: session.conversation_id,
                conversation_title: None,
                agent_type: session.agent_type,
                status: session.status,
                launcher_pid: session.launcher_pid,
                last_activity_at: session.last_activity_at,
                private_memory_bytes: complete_private_memory(group.iter().copied()),
                memory_bytes: group.iter().map(|process| process.memory_bytes).sum(),
                process_count: group.len(),
                recoverable: session.recoverable,
                protection_reason: session.protection_reason.map(str::to_string),
                can_end: session.protection_reason.is_none(),
            }
        })
        .collect()
}
