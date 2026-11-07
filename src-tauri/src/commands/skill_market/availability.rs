use super::availability_snapshot::Snapshot;
use super::{client, parse_value, RemovalPolicy, SkillAvailability, AVAILABILITY_TIMEOUT};
use crate::app_error::AppCommandError;
use sea_orm::DatabaseConnection;
use std::collections::HashSet;

pub(super) async fn reconcile_market_skill_availability_batch(
    conn: &DatabaseConnection,
    ids: &[i64],
) -> Result<bool, AppCommandError> {
    let _storage = crate::acp::agent_storage_work::begin_agent_storage_read().await;
    let snapshot = {
        let _operation = crate::plugin_runtime::registry::lock_mutation().await;
        Snapshot::capture(conn, ids).await?
    };
    let body = serde_json::json!({"ids":ids.iter().map(ToString::to_string).collect::<Vec<_>>()});
    let builder = client::request(conn, reqwest::Method::POST, "/skills/availability")
        .await?
        .json(&body)
        .timeout(AVAILABILITY_TIMEOUT);
    let items: Vec<SkillAvailability> = parse_value(client::send(builder).await?, Some("items"))?;
    let requested = ids.iter().copied().collect::<HashSet<_>>();
    let mut deferred = false;
    for item in items {
        if !matches!(item.status.as_str(), "removed" | "inaccessible") {
            continue;
        }
        let Some(id) = requested_id(&item, &requested) else {
            continue;
        };
        match cleanup(conn, id, &snapshot).await {
            Ok(waiting) => {
                deferred |= waiting;
                if !waiting {
                    tracing::info!(skill_id=id,status=%item.status,"[skill-market] 已清理不可访问的本地能力");
                }
            }
            Err(error) => {
                deferred = true;
                tracing::warn!(skill_id=id,status=%item.status,error=%error,"[skill-market] 不可访问能力的清理未完成，按原事务保留可恢复记录");
            }
        }
    }
    Ok(deferred)
}

fn requested_id(item: &SkillAvailability, requested: &HashSet<i64>) -> Option<i64> {
    let id = item
        .id
        .parse::<i64>()
        .ok()
        .filter(|id| requested.contains(id));
    if id.is_none() {
        tracing::warn!(id=%item.id,status=%item.status,"[skill-market] 忽略清理响应中的无效或未请求标识");
    }
    id
}

async fn cleanup(
    conn: &DatabaseConnection,
    id: i64,
    snapshot: &Snapshot,
) -> Result<bool, AppCommandError> {
    let _operation = crate::plugin_runtime::registry::lock_mutation().await;
    if !snapshot.unchanged(conn, id).await? {
        tracing::info!(
            skill_id = id,
            "[skill-market] 账号或安装已变化，旧清理响应未应用"
        );
        return Ok(true);
    }
    for (slug, version) in super::installed_market_skill_plugin_refs(conn, id).await? {
        if crate::plugin_runtime::global::plugin_version_has_active_leases(&slug, Some(&version))
            .await
        {
            return Ok(true);
        }
    }
    super::uninstall_locked(conn, id, RemovalPolicy::Unavailable).await?;
    Ok(false)
}
