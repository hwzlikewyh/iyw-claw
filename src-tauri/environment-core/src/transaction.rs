use anyhow::{bail, Context, Result};

use crate::inventory;
use crate::model::{EnvironmentSnapshot, PreparedState};
use crate::paths::{safe_segment, Layout};

pub fn snapshot(state: &PreparedState) -> EnvironmentSnapshot {
    EnvironmentSnapshot {
        schema_version: 1,
        generation: state.transaction_id.clone(),
        pc_version: state.pc_version.clone(),
        catalog_revision: state.catalog_revision,
        binding_revision: state.binding_revision,
        target: state.target.clone(),
        arch: state.arch.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
        selected_components: Some(
            state
                .components
                .iter()
                .map(|item| item.component.component_id.clone())
                .collect(),
        ),
        components: state
            .components
            .iter()
            .map(|item| item.component.clone())
            .collect(),
    }
}

pub fn published_snapshot(
    layout: &Layout,
    state: &PreparedState,
) -> Result<Option<EnvironmentSnapshot>> {
    if state.base_digest == inventory::snapshot_digest(layout)? {
        return Ok(None);
    }
    let current = inventory::load_current(layout)?.context("已提交环境清单缺失")?;
    let components = state
        .components
        .iter()
        .map(|item| &item.component)
        .collect::<Vec<_>>();
    if current.generation != state.transaction_id
        || current.pc_version != state.pc_version
        || current.catalog_revision != state.catalog_revision
        || current.binding_revision != state.binding_revision
        || serde_json::to_value(&current.components)? != serde_json::to_value(components)?
    {
        bail!("环境已被另一个安装程序更改，请重新准备后重试")
    }
    Ok(Some(current))
}

pub fn finish(layout: &Layout, generation: &str) -> Result<()> {
    inventory::write_json(
        &receipt_path(layout, generation)?,
        &serde_json::json!({"generation":generation,"state":"committed"}),
    )
}

pub fn verify_finished(layout: &Layout, snapshot: &EnvironmentSnapshot) -> Result<()> {
    if snapshot.selected_components.is_none() {
        return Ok(());
    }
    let bytes = std::fs::read(receipt_path(layout, &snapshot.generation)?)
        .context("环境提交尚未完成，请重新运行安装或环境修复")?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    if value["generation"] != snapshot.generation || value["state"] != "committed" {
        bail!("环境事务记录异常，请运行环境修复")
    }
    Ok(())
}

fn receipt_path(layout: &Layout, generation: &str) -> Result<std::path::PathBuf> {
    safe_segment(generation, "generation")?;
    Ok(layout
        .inventory
        .join("transactions")
        .join(format!("{generation}.json")))
}
