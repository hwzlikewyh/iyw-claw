use std::collections::BTreeMap;
use std::time::Instant;

use sea_orm::DatabaseConnection;

use super::{
    reconcile_shared_skills_for_agent_state_locked, repair_market_target_references_locked,
    shared_skill_mutation_guard, AcpError, AgentType,
};
use crate::commands::skill_inventory::{skill_inventory_list_core, SkillInventorySnapshot};

type ReconcileInput = (AgentType, bool, BTreeMap<String, bool>);

pub(super) async fn reconcile_agents(
    conn: &DatabaseConnection,
    agent_states: Vec<(AgentType, bool)>,
) -> Result<(), AcpError> {
    let started = Instant::now();
    // 清单已包含全部 Agent 的有效状态；本轮共用一次扫描，下一轮仍重新检查。
    let snapshot = skill_inventory_list_core(conn, None).await?;
    let inventory_ms = started.elapsed().as_millis();
    let agent_count = agent_states.len();
    let skill_count = snapshot.skills.len();
    let inputs = agent_states
        .into_iter()
        .map(|(agent, enabled)| (agent, enabled, states_for_agent(&snapshot, agent)))
        .collect();
    drop(snapshot);
    let result = tokio::task::spawn_blocking(move || reconcile_locked(inputs))
        .await
        .map_err(|error| {
            AcpError::protocol(format!("Shared skill reconciliation task failed: {error}"))
        })?;
    if let Err(error) = &result {
        tracing::warn!(agent_count, skill_count, inventory_ms,
            elapsed_ms = started.elapsed().as_millis(), %error,
            "[skills] shared publication reconcile failed");
    } else {
        tracing::debug!(
            agent_count,
            skill_count,
            inventory_ms,
            elapsed_ms = started.elapsed().as_millis(),
            "[skills] shared publication reconciled from one inventory"
        );
    }
    result
}

pub(super) fn states_for_agent(
    snapshot: &SkillInventorySnapshot,
    agent_type: AgentType,
) -> BTreeMap<String, bool> {
    snapshot
        .skills
        .iter()
        .filter_map(|skill| {
            skill
                .agent_states
                .iter()
                .find(|state| state.agent_type == agent_type)
                .map(|state| (skill.skill_id.clone(), state.effective_enabled))
        })
        .collect()
}

fn reconcile_locked(inputs: Vec<ReconcileInput>) -> Result<(), AcpError> {
    // 锁随阻塞任务持有至写入结束，调用方取消也不会提前释放。
    let _guard = shared_skill_mutation_guard();
    let mut errors = Vec::new();
    if let Err(error) = repair_market_target_references_locked() {
        errors.push(format!("market reference repair: {error}"));
    }
    for (agent_type, enabled, policies) in inputs {
        if let Err(error) =
            reconcile_shared_skills_for_agent_state_locked(agent_type, enabled, &policies)
        {
            errors.push(format!("{agent_type}: {error}"));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(AcpError::protocol(format!(
            "shared skill reconcile failed for {} agent(s): {}",
            errors.len(),
            errors.join("; ")
        )))
    }
}
