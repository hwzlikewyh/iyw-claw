//! 组件切换前持久化意图；进程崩溃后在环境锁内恢复，不依赖内存回滚列表。
use crate::{
    inventory,
    model::PreparedComponent,
    paths::{from_slash, safe_segment, Layout},
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Serialize, Deserialize)]
struct Journal {
    transaction: String,
    entries: Vec<Entry>,
}
#[derive(Serialize, Deserialize)]
struct Entry {
    component: String,
    destination: String,
    had_original: bool,
    restoring: bool,
}
const JOURNAL: &str = "activation-pending-v1.json";
const MAX_JOURNAL_BYTES: u64 = 1024 * 1024;

fn read_journal(path: &Path) -> Result<Journal> {
    plain_path(path)?;
    if fs::metadata(path)?.len() > MAX_JOURNAL_BYTES {
        bail!("environment recovery journal is too large")
    }
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn plain_path(path: &Path) -> Result<()> {
    for parent in path.ancestors() {
        let metadata = match fs::symlink_metadata(parent) {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() {
            bail!("environment recovery path contains a link")
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            const REPARSE_POINT: u32 = 0x400;
            if metadata.file_attributes() & REPARSE_POINT != 0 {
                bail!("environment recovery path contains a junction")
            }
        }
    }
    Ok(())
}

fn validate_entries(layout: &Layout, journal: &Journal) -> Result<()> {
    let mut components = std::collections::HashSet::new();
    for entry in &journal.entries {
        let parts: Vec<_> = entry.destination.split('/').collect();
        if parts.len() != 4
            || parts[0] != "runtime"
            || parts[1] != entry.component
            || !components.insert(&entry.component)
        {
            bail!("invalid component recovery destination")
        }
        for part in parts {
            safe_segment(part, "recovery path")?;
        }
        plain_path(&from_slash(&layout.root, &entry.destination)?)?;
        plain_path(
            &layout
                .quarantine
                .join(&journal.transaction)
                .join(&entry.component),
        )?;
        plain_path(
            &layout
                .transaction_dir(&journal.transaction)?
                .join("components")
                .join(&entry.component),
        )?;
    }
    Ok(())
}

pub fn begin(layout: &Layout, transaction: &str) -> Result<()> {
    recover(layout, None)?;
    inventory::write_json(
        &layout.inventory.join(JOURNAL),
        &Journal {
            transaction: transaction.into(),
            entries: vec![],
        },
    )
}

pub fn record(layout: &Layout, transaction: &str, prepared: &PreparedComponent) -> Result<()> {
    let path = layout.inventory.join(JOURNAL);
    let mut journal = read_journal(&path)?;
    if journal.transaction != transaction {
        bail!("environment activation journal changed")
    }
    let destination = from_slash(&layout.root, &prepared.component.relative_path)?;
    journal.entries.push(Entry {
        component: prepared.component.component_id.clone(),
        destination: prepared.component.relative_path.clone(),
        had_original: destination.exists(),
        restoring: false,
    });
    validate_entries(layout, &journal)?;
    inventory::write_json(&path, &journal)
}

pub fn finish(layout: &Layout) -> Result<()> {
    let path = layout.inventory.join(JOURNAL);
    if path.exists() {
        fs::remove_file(path).context("clear activation recovery journal")?;
    }
    Ok(())
}

pub fn mark_committed(layout: &Layout, transaction: &str) -> Result<()> {
    let receipt = layout
        .inventory
        .join("transactions")
        .join(format!("{transaction}.json"));
    inventory::write_json(
        &receipt,
        &serde_json::json!({
            "schemaVersion": 2, "generation": transaction, "state": "committed"
        }),
    )
}

pub fn recover(layout: &Layout, expected: Option<&str>) -> Result<()> {
    let path = layout.inventory.join(JOURNAL);
    if !path.exists() {
        return Ok(());
    }
    let mut journal = read_journal(&path)?;
    safe_segment(&journal.transaction, "transaction")?;
    validate_entries(layout, &journal)?;
    if expected.is_some_and(|id| id != journal.transaction) {
        bail!("another environment transaction owns the recovery journal")
    }
    if inventory::load_current(layout)?.is_some_and(|s| s.generation == journal.transaction) {
        mark_committed(layout, &journal.transaction)?;
        return finish(layout);
    }
    while let Some(entry) = journal.entries.last() {
        let id = safe_segment(&entry.component, "component")?;
        let destination = from_slash(&layout.root, &entry.destination)?;
        if !destination.starts_with(layout.runtime.join(id)) {
            bail!("environment recovery path is outside its component")
        }
        let backup = layout.quarantine.join(&journal.transaction).join(id);
        let source = layout
            .transaction_dir(&journal.transaction)?
            .join("components")
            .join(id);
        if !entry.restoring {
            // 备份仍在或者原本没有目标目录时，才可以移走本次激活的内容。
            if (backup.exists() || !entry.had_original) && !source.exists() && destination.exists()
            {
                fs::create_dir_all(source.parent().context("invalid component stage")?)?;
                fs::rename(&destination, &source)
                    .context("retain interrupted component activation")?;
            }
            journal.entries.last_mut().unwrap().restoring = true;
            inventory::write_json(&path, &journal)?;
        }
        if backup.exists() {
            fs::create_dir_all(
                destination
                    .parent()
                    .context("invalid restored component path")?,
            )?;
            fs::rename(&backup, &destination).context("restore interrupted component backup")?;
        }
        journal.entries.pop();
        inventory::write_json(&path, &journal)?;
    }
    eprintln!(
        "environment activation recovered: transaction={}",
        journal.transaction
    );
    finish(layout)
}
