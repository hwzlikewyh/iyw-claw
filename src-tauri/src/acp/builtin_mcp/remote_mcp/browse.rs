use std::time::Instant;

use rmcp::ErrorData;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{failure, RemoteAccount};

const MAX_CURSORS: usize = 256;

pub(super) struct BrowseCursor {
    remote: String,
    group: Option<String>,
    accessed: Instant,
}

impl RemoteAccount {
    pub(super) fn browse_arguments(&self, arguments: &mut Value) -> Result<(), ErrorData> {
        let group = arguments
            .get("group_id")
            .and_then(Value::as_str)
            .map(str::to_owned);
        if let Some(id) = &group {
            let route = self.route(id)?;
            if !route.group {
                return Err(failure(
                    "remote_group_required",
                    "group_id must identify a returned group capability_id",
                    false,
                ));
            }
            arguments["group_id"] = route.id.into();
        }
        if let Some(cursor) = arguments.get("cursor").and_then(Value::as_str) {
            let mut cursors = self
                .cursors
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let entry = cursors.get_mut(cursor).filter(|entry| entry.group == group)
                .ok_or_else(|| failure("remote_catalog_expired", "Browse cursor expired or belongs to a different group/account; restart browsing", false))?;
            entry.accessed = Instant::now();
            arguments["cursor"] = entry.remote.clone().into();
        }
        Ok(())
    }

    pub(super) fn project_cursor(&self, payload: &Value, group: Option<String>) -> Value {
        let Some(remote) = payload
            .get("next_cursor")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        else {
            return Value::Null;
        };
        let mut hash = Sha256::new();
        hash.update(self.fingerprint);
        hash.update(serde_json::to_vec(&(&group, remote)).unwrap_or_default());
        let id = format!("iyw.cursor.{:x}", hash.finalize());
        let mut cursors = self
            .cursors
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if cursors.len() >= MAX_CURSORS && !cursors.contains_key(&id) {
            let oldest = cursors
                .iter()
                .min_by_key(|(_, item)| item.accessed)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                cursors.remove(&oldest);
            }
        }
        cursors.insert(
            id.clone(),
            BrowseCursor {
                remote: remote.to_owned(),
                group,
                accessed: Instant::now(),
            },
        );
        Value::String(id)
    }
}
