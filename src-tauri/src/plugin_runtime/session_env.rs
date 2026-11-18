use std::{collections::BTreeMap, path::PathBuf};

use sea_orm::DatabaseConnection;
use sha2::{Digest, Sha256};

const DATA: &str = "IYW_PLUGIN_DATA_ROOT";
const ACCOUNT: &str = "IYW_PLUGIN_ACCOUNT_HASH";
const SESSION: &str = "IYW_PLUGIN_CONVERSATION_ID";

pub(crate) fn data_root() -> Option<PathBuf> {
    crate::acp::agent_storage::AgentStoragePaths::active()
        .map(|storage| storage.root().join("plugin-data"))
}

pub(crate) fn account_hash(scope: &str) -> String {
    format!("{:x}", Sha256::digest(scope.as_bytes()))
}

pub(crate) fn is_session_key(key: &str) -> bool {
    matches!(key, DATA | ACCOUNT | SESSION)
}

pub(crate) async fn project(
    db: Option<&DatabaseConnection>,
    conversation: Option<i32>,
    env: &mut BTreeMap<String, String>,
) {
    env.retain(|key, _| !is_session_key(key));
    let (Some(db), Some(id), Some(root)) = (db, conversation.filter(|id| *id != 0), data_root()) else {
        return;
    };
    match crate::commands::mcp_cli_config::account_scope(db).await {
        Ok(scope) => {
            env.insert(DATA.into(), root.to_string_lossy().into_owned());
            env.insert(ACCOUNT.into(), account_hash(&scope));
            env.insert(SESSION.into(), id.to_string());
        }
        Err(error) => tracing::warn!(conversation_id = id, %error, "Could not bind plugin session"),
    }
}
