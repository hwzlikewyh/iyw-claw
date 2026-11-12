use crate::app_error::AppCommandError;
use crate::db::entities::plugin_installation;
use crate::db::service::plugin_installation_service;
use sea_orm::DatabaseConnection;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Default)]
struct Slot {
    installation: Option<plugin_installation::Model>,
    markers: Vec<(PathBuf, Vec<u8>)>,
}

pub(super) struct Snapshot {
    scope: String,
    slots: BTreeMap<i64, Slot>,
}

const MAX_SHARED_ENTRIES: usize = 4096;

impl Snapshot {
    pub(super) async fn capture(
        conn: &DatabaseConnection,
        ids: &[i64],
    ) -> Result<Self, AppCommandError> {
        let scope = crate::commands::mcp_cli_config::account_scope(conn).await?;
        let mut slots = ids
            .iter()
            .map(|id| (*id, Slot::default()))
            .collect::<BTreeMap<_, _>>();
        for installation in plugin_installation_service::list_installations_by_ids(conn, ids)
            .await
            .map_err(AppCommandError::db)?
        {
            if let Some(slot) = slots.get_mut(&installation.market_skill_id) {
                slot.installation = Some(installation);
            }
        }
        collect_markers(&mut slots)?;
        Ok(Self { scope, slots })
    }

    pub(super) async fn unchanged(
        &self,
        conn: &DatabaseConnection,
        id: i64,
    ) -> Result<bool, AppCommandError> {
        if crate::commands::mcp_cli_config::account_scope(conn).await? != self.scope {
            return Ok(false);
        }
        let Some(slot) = self.slots.get(&id) else {
            return Ok(false);
        };
        if slot.installation.is_none() && slot.markers.is_empty() {
            return Ok(false);
        }
        let current = plugin_installation_service::list_installations_by_ids(conn, &[id])
            .await
            .map_err(AppCommandError::db)?
            .pop();
        if current != slot.installation {
            return Ok(false);
        }
        for (path, expected) in &slot.markers {
            let current = super::plugin_history::local::read_marker(path)?
                .map(|marker| serde_json::to_vec(&marker))
                .transpose()
                .map_err(|_| invalid())?;
            if current.as_deref() != Some(expected.as_slice()) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

fn collect_markers(slots: &mut BTreeMap<i64, Slot>) -> Result<(), AppCommandError> {
    let entries = match std::fs::read_dir(crate::commands::acp::shared_skills_dir()) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(invalid()),
    };
    for (index, entry) in entries.enumerate() {
        if index >= MAX_SHARED_ENTRIES {
            return Err(invalid());
        }
        let path = entry.map_err(|_| invalid())?.path();
        if let Some(marker) = super::plugin_history::local::read_marker(&path)? {
            if let Some(slot) = slots.get_mut(&marker.skill_id) {
                slot.markers
                    .push((path, serde_json::to_vec(&marker).map_err(|_| invalid())?));
            }
        }
    }
    Ok(())
}

fn invalid() -> AppCommandError {
    AppCommandError::configuration_invalid("无法完整记录已安装能力的清理快照，操作已保留本地安装")
}
