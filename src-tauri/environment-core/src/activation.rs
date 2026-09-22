use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::inventory;
use crate::model::{EnvironmentSnapshot, PreparedComponent, PreparedState};
use crate::paths::{from_slash, safe_segment, Layout};

struct ActivatedComponent {
    destination: PathBuf,
    backup: Option<PathBuf>,
}

pub fn commit(
    layout: &Layout,
    state: &PreparedState,
    snapshot: &EnvironmentSnapshot,
) -> Result<()> {
    let mut activated = Vec::new();
    let result = activate_and_publish(layout, state, snapshot, &mut activated);
    if let Err(error) = result {
        if let Err(rollback_error) = rollback(activated) {
            return Err(error).context(format!(
                "environment rollback also failed: {rollback_error:#}"
            ));
        }
        return Err(error);
    }
    cleanup(layout, state, activated);
    write_receipts(layout, state);
    Ok(())
}

fn activate_and_publish(
    layout: &Layout,
    state: &PreparedState,
    snapshot: &EnvironmentSnapshot,
    activated: &mut Vec<ActivatedComponent>,
) -> Result<()> {
    for prepared in &state.components {
        if prepared.staged {
            activated.push(activate_component(layout, state, prepared)?);
        }
        crate::download::emit(&prepared.component.component_id, "verifying", 0, 0);
        inventory::verify_component(layout, &prepared.component).context(
            "environment integrity check failed; run environment repair before retrying",
        )?;
        crate::download::emit(&prepared.component.component_id, "verified", 0, 0);
    }
    let generation = layout
        .inventory
        .join("environments")
        .join(format!("{}.json", snapshot.generation));
    inventory::write_json(&generation, snapshot)?;
    inventory::write_json(&layout.current_snapshot(), snapshot)
}

fn activate_component(
    layout: &Layout,
    state: &PreparedState,
    prepared: &PreparedComponent,
) -> Result<ActivatedComponent> {
    let component = safe_segment(&prepared.component.component_id, "component")?;
    let source = layout
        .transaction_dir(&state.transaction_id)?
        .join("components")
        .join(component);
    let destination = from_slash(&layout.root, &prepared.component.relative_path)?;
    let backup = move_existing_to_backup(layout, state, component, &destination)?;
    fs::create_dir_all(
        destination
            .parent()
            .context("component path has no parent")?,
    )?;
    if let Err(error) = fs::rename(&source, &destination) {
        restore_backup(&destination, backup.as_ref())?;
        return Err(error).context("activate managed component");
    }
    Ok(ActivatedComponent {
        destination,
        backup,
    })
}

fn move_existing_to_backup(
    layout: &Layout,
    state: &PreparedState,
    component: &str,
    destination: &PathBuf,
) -> Result<Option<PathBuf>> {
    if !destination.exists() {
        return Ok(None);
    }
    let backup = layout
        .quarantine
        .join(safe_segment(&state.transaction_id, "transaction")?)
        .join(component);
    if backup.exists() {
        fs::remove_dir_all(&backup)?;
    }
    fs::create_dir_all(backup.parent().context("backup path has no parent")?)?;
    fs::rename(destination, &backup)?;
    Ok(Some(backup))
}

fn rollback(activated: Vec<ActivatedComponent>) -> Result<()> {
    for item in activated.into_iter().rev() {
        if item.destination.exists() {
            fs::remove_dir_all(&item.destination).context("remove failed component activation")?;
        }
        restore_backup(&item.destination, item.backup.as_ref())?;
    }
    Ok(())
}

fn restore_backup(destination: &PathBuf, backup: Option<&PathBuf>) -> Result<()> {
    if let Some(backup) = backup.filter(|path| path.exists()) {
        fs::rename(backup, destination).context("restore previous managed component")?;
    }
    Ok(())
}

fn cleanup(layout: &Layout, state: &PreparedState, activated: Vec<ActivatedComponent>) {
    for item in activated {
        if let Some(backup) = item.backup {
            let _ = fs::remove_dir_all(backup);
        }
    }
    if let Ok(path) = layout.transaction_dir(&state.transaction_id) {
        let _ = fs::remove_dir_all(path);
    }
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
