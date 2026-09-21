use std::fs;

use anyhow::{bail, Context, Result};

use crate::inventory;
use crate::model::{EnvironmentSnapshot, PreparedState};
use crate::paths::{ensure_within, from_slash, Layout};

pub fn commit(
    layout: &Layout,
    state: &PreparedState,
    snapshot: &EnvironmentSnapshot,
) -> Result<()> {
    for prepared in &state.components {
        if prepared.staged {
            activate_component(layout, state, &prepared.component)?;
        }
        inventory::verify_component(layout, &prepared.component)?;
    }
    let generation = layout
        .inventory
        .join("environments")
        .join(format!("{}.json", snapshot.generation));
    inventory::write_json(&generation, snapshot)?;
    inventory::write_json(&layout.current_snapshot(), snapshot)?;
    crate::memory_model::publish_pointer(layout, snapshot)?;
    write_receipts(layout, state);
    Ok(())
}

fn activate_component(
    layout: &Layout,
    state: &PreparedState,
    component: &crate::model::InstalledComponent,
) -> Result<()> {
    let (_, _, platform) = crate::paths::platform();
    let mut expected =
        layout.component_dir(&component.component_id, &component.version, &platform)?;
    if component.component_id != crate::memory_model::COMPONENT {
        expected.push(&state.transaction_id);
    }
    let destination = from_slash(&layout.root, &component.relative_path)?;
    if expected != destination {
        bail!("prepared component destination does not match its identity")
    }
    ensure_within(&layout.root, &destination)?;
    // 各次安装目录独立；崩溃后可继续提交，旧清单与在用文件始终保持原样。
    if destination.exists() {
        if inventory::verify_component(layout, component).is_ok() {
            return Ok(());
        }
        if component.component_id != crate::memory_model::COMPONENT {
            bail!("previously staged component failed verification")
        }
        let backup = layout
            .quarantine
            .join(&state.transaction_id)
            .join(&component.component_id);
        ensure_within(&layout.root, &backup)?;
        fs::create_dir_all(backup.parent().context("model backup has no parent")?)?;
        fs::rename(&destination, &backup)
            .context("BGE 模型正在使用或不可写，请关闭主程序后重试")?;
    }
    let source = layout
        .transaction_dir(&state.transaction_id)?
        .join("components")
        .join(&component.component_id);
    ensure_within(&layout.root, &source)?;
    fs::create_dir_all(
        destination
            .parent()
            .context("component path has no parent")?,
    )?;
    fs::rename(&source, &destination).context("activate verified component")?;
    Ok(())
}

fn write_receipts(layout: &Layout, state: &PreparedState) {
    for prepared in &state.components {
        let receipt = layout
            .inventory
            .join("receipts")
            .join(&prepared.component.component_id)
            .join(format!("{}.json", prepared.component.artifact_id));
        if let Err(error) = inventory::write_json(&receipt, &prepared.component) {
            eprintln!("write environment receipt failed: {error:#}");
        }
    }
}
