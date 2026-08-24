use async_trait::async_trait;
use sea_orm::DatabaseConnection;
use serde_json::{json, Map, Value};

use crate::acp::delegation::listener::UserProfileAccess;
use crate::app_error::AppErrorCode;
use crate::commands::iyw_account::iyw_account_get_profile_core;

/// Account profile adapter used by the Agent host capability. It projects the
/// full account response to display-safe fields at this boundary.
pub struct DbUserProfileAccess {
    conn: DatabaseConnection,
}

impl DbUserProfileAccess {
    pub fn new(conn: DatabaseConnection) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl UserProfileAccess for DbUserProfileAccess {
    async fn current_profile(&self) -> Value {
        let profile = match iyw_account_get_profile_core(&self.conn).await {
            Ok(profile) => profile,
            Err(error) => {
                tracing::warn!(
                    code = ?error.code,
                    "[iyw-account] Agent profile capability unavailable"
                );
                return json!({
                    "status": "profile_unavailable",
                    "errorCode": profile_error_code(error.code),
                });
            }
        };
        if !profile.logged_in {
            return json!({ "status": "logged_out" });
        }
        let mut safe = Map::new();
        insert_non_empty(&mut safe, "display_name", profile.name);
        insert_non_empty(&mut safe, "preferred_name", profile.nick_name);
        insert_non_empty(&mut safe, "organization_name", profile.org_name);
        json!({ "status": "ok", "profile": Value::Object(safe) })
    }
}

fn insert_non_empty(target: &mut Map<String, Value>, key: &str, value: Option<String>) {
    let Some(value) = value.map(|value| value.trim().to_string()) else {
        return;
    };
    if !value.is_empty() {
        target.insert(key.to_string(), Value::String(value));
    }
}

fn profile_error_code(code: AppErrorCode) -> &'static str {
    match code {
        AppErrorCode::AuthenticationFailed => "authentication_failed",
        AppErrorCode::NetworkError => "network_error",
        AppErrorCode::DatabaseError => "database_error",
        _ => "profile_error",
    }
}
