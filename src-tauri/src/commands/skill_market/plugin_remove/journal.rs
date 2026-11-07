use crate::app_error::AppCommandError;
use crate::db::service::app_metadata_service;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Journal {
    pub plan: super::cleanup::Plan,
    pub clear: bool,
    pub phase: String,
    pub operation: String,
    #[serde(default)]
    pub backups: Vec<crate::db::service::plugin_installation_service::PluginInstallationInput>,
    #[serde(default)]
    pub pointers: Vec<PointerBackup>,
    #[serde(default)]
    pub skills: Option<serde_json::Value>,
    #[serde(default)]
    pub catalog_ref: Option<String>,
    #[serde(default)]
    pub root_updated_at: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PointerBackup {
    pub slug: String,
    pub previous: Vec<u8>,
    pub next: Vec<u8>,
}

pub(super) async fn write(
    conn: &DatabaseConnection,
    journal: &Journal,
) -> Result<(), AppCommandError> {
    let raw = serde_json::to_string(journal).map_err(|_| super::invalid("移除记录无法编码"))?;
    if raw.len() > MAX_JOURNAL_BYTES {
        return Err(super::invalid("插件移除记录超过安全容量，操作未提交"));
    }
    app_metadata_service::upsert_value(conn, &key(&journal.plan.id, &journal.plan.scope), &raw)
        .await
        .map_err(AppCommandError::db)
}

pub(super) async fn read(
    conn: &DatabaseConnection,
    id: &str,
    scope: &str,
) -> Result<Option<Journal>, AppCommandError> {
    let raw = app_metadata_service::get_value(conn, &key(id, scope))
        .await
        .map_err(AppCommandError::db)?;
    raw.map(|value| {
        if value.len() > MAX_JOURNAL_BYTES {
            return Err(super::invalid("插件移除记录超过读取限制"));
        }
        let journal: Journal = serde_json::from_str(&value)
            .map_err(|_| super::invalid("插件移除记录损坏，请检查本地安装记录"))?;
        if journal.plan.id != id
            || journal.plan.scope != scope
            || uuid::Uuid::parse_str(&journal.operation).is_err()
        {
            return Err(super::invalid("插件移除记录身份无效"));
        }
        super::super::plugin_history::local::root(&journal.plan.slug)?;
        Ok(journal)
    })
    .transpose()
}

const MAX_JOURNAL_BYTES: usize = 8 * 1024 * 1024;
fn key(id: &str, scope: &str) -> String {
    format!("plugin-removal:{id}:{scope}")
}

pub(super) async fn finish(
    conn: &DatabaseConnection,
    journal: &mut Journal,
) -> Result<bool, AppCommandError> {
    if journal.phase != "committed" {
        return Err(super::invalid("插件尚未完成移除，不能执行数据清理"));
    }
    if crate::commands::mcp_cli_config::account_scope(conn).await? != journal.plan.scope {
        return Ok(false);
    }
    finish_affected(conn, journal).await?;
    if journal.clear {
        let plan = journal.plan.clone();
        if let Err(error) = tokio::task::spawn_blocking(move || super::cleanup::purge(&plan))
            .await
            .map_err(|_| super::invalid("插件清理任务中断"))?
        {
            tracing::warn!(plugin=%journal.plan.slug,code=?error.code,"[plugin-remove] 安装已移除，账号数据清理待继续");
            return Ok(false);
        }
    }
    let plan = journal.plan.clone();
    if tokio::task::spawn_blocking(move || super::cleanup::package(&plan))
        .await
        .map_err(|_| super::invalid("插件包清理任务中断"))?
        .is_err()
    {
        return Ok(false);
    }
    journal.phase = "complete".into();
    super::catalog::clear(journal.catalog_ref.as_deref())?;
    journal.catalog_ref = None;
    journal.backups.clear();
    journal.pointers.clear();
    journal.skills = None;
    write(conn, journal).await?;
    Ok(true)
}

async fn finish_affected(
    conn: &DatabaseConnection,
    journal: &Journal,
) -> Result<(), AppCommandError> {
    use crate::db::entities::plugin_installation::{Column, Entity};
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    for backup in &journal.backups {
        if backup.market_skill_id.to_string() == journal.plan.id {
            continue;
        }
        Entity::update_many()
            .col_expr(
                Column::ReconcileState,
                sea_orm::sea_query::Expr::value("ready"),
            )
            .filter(Column::MarketSkillId.eq(backup.market_skill_id))
            .filter(Column::Version.eq(backup.version.clone()))
            .filter(Column::ReconcileState.eq("removal_pending"))
            .exec(conn)
            .await
            .map_err(|error| AppCommandError::db(error.into()))?;
    }
    crate::plugin_runtime::registry::reconcile_global(conn).await?;
    Ok(())
}
