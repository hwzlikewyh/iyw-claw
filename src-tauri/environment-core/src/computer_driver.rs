use anyhow::{Context, Result};
use chrono::Utc;
use fs2::FileExt;

use crate::{
    inventory,
    paths::{from_slash, Layout},
};

/// 先发布新清单再清理组件；写清单失败时不会破坏现有驱动。
pub fn remove() -> Result<()> {
    let layout = Layout::resolve()?;
    layout.ensure()?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(layout.runtime.join(".environment-writer.lock"))?;
    lock.lock_exclusive()?;
    let Some(mut snapshot) = inventory::load_current(&layout)? else {
        return Ok(());
    };
    let removed = snapshot
        .components
        .iter()
        .filter(|c| c.component_id == "cua-driver")
        .map(|c| from_slash(&layout.root, &c.relative_path))
        .collect::<Result<Vec<_>>>()?;
    if removed.is_empty() {
        return Ok(());
    }
    snapshot
        .components
        .retain(|c| c.component_id != "cua-driver");
    snapshot.generation = uuid::Uuid::new_v4().simple().to_string();
    snapshot.created_at = Utc::now().to_rfc3339();
    let generation = layout
        .inventory
        .join("environments")
        .join(format!("{}.json", snapshot.generation));
    inventory::write_json(&generation, &snapshot)?;
    inventory::write_json(&layout.current_snapshot(), &snapshot)?;
    for path in removed {
        // 清单只决定组件身份，删除范围仍受固定受管目录约束。
        if path.starts_with(layout.runtime.join("cua-driver")) && path.is_dir() {
            std::fs::remove_dir_all(&path).context("清理受管 cua-driver 失败")?;
        }
    }
    crate::download::emit("cua-driver", "committed", 0, 0);
    Ok(())
}
