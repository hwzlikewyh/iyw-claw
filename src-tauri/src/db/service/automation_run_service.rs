use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, JoinType, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, QueryTrait, RelationTrait,
};

use crate::db::entities::{automation, automation_run};
use crate::db::error::DbError;
use crate::models::{
    AutomationRunInfo, AutomationRunPage, AutomationRunQuery, AutomationRunStatus,
};

pub const RUN_PAGE_SIZE: u64 = 50;

pub struct RunLaunch {
    pub trigger: String,
    pub scheduled_for: Option<DateTime<Utc>>,
    pub retry_of_run_id: Option<i32>,
}

pub async fn get(conn: &DatabaseConnection, id: i32) -> Result<automation_run::Model, DbError> {
    automation_run::Entity::find_by_id(id)
        .one(conn)
        .await?
        .ok_or_else(|| DbError::NotFound(format!("automation run {id}")))
}

pub async fn history(
    conn: &DatabaseConnection,
    query: AutomationRunQuery,
) -> Result<AutomationRunPage, DbError> {
    let mut select = automation_run::Entity::find()
        .join(
            JoinType::InnerJoin,
            automation_run::Relation::Automation.def(),
        )
        .filter(automation::Column::DeletedAt.is_null());
    if query.retryable {
        let children = automation_run::Entity::find()
            .select_only()
            .column(automation_run::Column::RetryOfRunId)
            .filter(automation_run::Column::RetryOfRunId.is_not_null())
            .filter(
                automation_run::Column::Status
                    .is_in([AutomationRunStatus::Running, AutomationRunStatus::Succeeded]),
            );
        select = select
            .filter(automation_run::Column::Status.eq(AutomationRunStatus::Failed))
            .filter(automation_run::Column::Id.not_in_subquery(children.into_query()));
    }
    if let Some(id) = query.automation_id {
        select = select.filter(automation_run::Column::AutomationId.eq(id));
    }
    if let Some(id) = query.folder_id {
        select = select.filter(automation::Column::RootFolderId.eq(id));
    }
    if let Some(value) = query.status.filter(|value| !value.is_empty()) {
        select = select.filter(automation_run::Column::Status.eq(value));
    }
    if let Some(value) = query.trigger.filter(|value| !value.is_empty()) {
        select = select.filter(automation_run::Column::Trigger.eq(value));
    }
    if let Some(value) = query.search.filter(|value| !value.trim().is_empty()) {
        select = select.filter(automation::Column::Name.contains(value.trim()));
    }
    let total = select.clone().count(conn).await?;
    let rows = select
        .order_by_desc(automation_run::Column::CreatedAt)
        .order_by_desc(automation_run::Column::Id)
        .offset(query.offset)
        .limit(RUN_PAGE_SIZE)
        .all(conn)
        .await?;
    Ok(AutomationRunPage {
        runs: with_retry_links(conn, rows).await?,
        total,
    })
}

pub async fn with_retry_links(
    conn: &DatabaseConnection,
    rows: Vec<automation_run::Model>,
) -> Result<Vec<AutomationRunInfo>, DbError> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<i32> = rows.iter().map(|row| row.id).collect();
    let children = automation_run::Entity::find()
        .filter(automation_run::Column::RetryOfRunId.is_in(ids))
        .filter(
            automation_run::Column::Status
                .is_in([AutomationRunStatus::Running, AutomationRunStatus::Succeeded]),
        )
        .order_by_asc(automation_run::Column::Id)
        .all(conn)
        .await?;
    let links: HashMap<i32, i32> = children
        .into_iter()
        .filter_map(|row| Some((row.retry_of_run_id?, row.id)))
        .collect();
    Ok(rows
        .into_iter()
        .map(|row| {
            let mut info = super::automation_service::run_to_info(row);
            info.retry_run_id = links.get(&info.id).copied();
            info
        })
        .collect())
}

pub async fn validate_retry(
    conn: &DatabaseConnection,
    id: i32,
) -> Result<automation_run::Model, DbError> {
    let original = get(conn, id).await?;
    if original.status != AutomationRunStatus::Failed {
        return Err(DbError::Validation(
            "only failed runs can be retried".into(),
        ));
    }
    let linked = with_retry_links(conn, vec![original.clone()]).await?;
    if linked[0].retry_run_id.is_some() {
        return Err(DbError::Validation(
            "this run already has a successful or active retry".into(),
        ));
    }
    Ok(original)
}
