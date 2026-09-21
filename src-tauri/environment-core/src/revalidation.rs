use anyhow::{bail, Result};

use crate::client::FusionClient;
use crate::inventory;
use crate::model::{EnvironmentPlan, InventoryEntry, PreparedState, ResolveRequest};
use crate::paths::Layout;
use crate::validation::{validate_plan, PlanExpectation};

pub fn verify(layout: &Layout, state: &PreparedState, installation_id: String) -> Result<()> {
    let current = inventory::load_current(layout).ok().flatten();
    let request = ResolveRequest {
        schema_version: 1,
        installation_id,
        client_version: state.pc_version.clone(),
        pc_version: state.pc_version.clone(),
        channel: "stable",
        runtime: "desktop",
        target: state.target.clone(),
        arch: state.arch.clone(),
        // 此处只核对在线策略；提交仍会逐文件验证，不重复计算整套库存摘要。
        inventory: current
            .iter()
            .flat_map(|snapshot| &snapshot.components)
            .map(|component| InventoryEntry {
                component_key: component.component_id.clone(),
                component_kind: component.component_kind.clone(),
                current_version: component.version.clone(),
                sha256: component.artifact_sha256.clone(),
                active: true,
                pinned: false,
                healthy: false,
                lkg: true,
            })
            .collect(),
    };
    crate::download::emit("environment", "revalidating", (0, 0));
    let plan = FusionClient::new()?.resolve(&request)?;
    validate_plan(
        &plan,
        &PlanExpectation {
            app_version: &state.pc_version,
            target: &state.target,
            arch: &state.arch,
        },
    )?;
    matches_prepared(&plan, state)
}

fn matches_prepared(plan: &EnvironmentPlan, state: &PreparedState) -> Result<()> {
    if plan.actions.len() != state.components.len() {
        bail!("安装期间环境下载策略发生变化，请重新准备后重试")
    }
    for action in &plan.actions {
        crate::validation::validate_action(action)?;
        let matches = state.components.iter().any(|entry| {
            let component = &entry.component;
            component.component_id == action.component_id
                && component.component_kind == action.component_kind
                && component.optional == action.optional
                && component.version == action.version
                && component.version_id == action.artifact.version_id
                && component.artifact_id == action.artifact.artifact_id
                && component.artifact_sha256 == action.artifact.sha256
                && component.package_kind == action.artifact.package_kind
        });
        if !matches {
            bail!(
                "组件 {} 的发布或安装策略已变化，请重新准备后重试",
                action.component_id
            )
        }
    }
    Ok(())
}
