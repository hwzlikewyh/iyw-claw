use sea_orm::sea_query::Expr;
use sea_orm::ExprTrait;
use sea_orm::{
    ColumnTrait, DatabaseConnection, DbErr, EntityTrait, FromQueryResult, QueryFilter, QueryOrder,
    QueryResult, QuerySelect, Select,
};
use serde::{Deserialize, Serialize};

use super::{conv_to_summary, fill_child_counts};
use crate::db::entities::conversation::{self, ConversationKind, ConversationStatus};
use crate::db::error::DbError;
use crate::db::service::conversation_query;
use crate::models::{AgentType, DbConversationSummary};

const DEFAULT_PAGE_SIZE: u64 = 100;
const MAX_PAGE_SIZE: u64 = 200;

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationPageRequest {
    pub folder_ids: Option<Vec<i32>>,
    pub agent_type: Option<AgentType>,
    pub search: Option<String>,
    pub sort_by: Option<String>,
    pub status: Option<String>,
    #[serde(default)]
    pub include_children: bool,
    pub cursor: Option<ConversationCursor>,
    pub page_size: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ConversationCursor {
    pub at: String,
    pub id: i32,
}

#[derive(Serialize)]
pub struct ConversationPage {
    pub items: Vec<DbConversationSummary>,
    pub next_cursor: Option<ConversationCursor>,
    pub incomplete: bool,
}

struct PageRow {
    model: conversation::Model,
    sort_key: String,
}

impl FromQueryResult for PageRow {
    fn from_query_result(row: &QueryResult, prefix: &str) -> Result<Self, DbErr> {
        Ok(Self {
            model: conversation::Model::from_query_result(row, prefix)?,
            // 游标沿用 SQLite 原始文本排序，兼容旧库时间格式及精度。
            sort_key: row.try_get(prefix, "updated_at")?,
        })
    }
}

impl ConversationPageRequest {
    pub fn limit(&self) -> u64 {
        self.page_size
            .unwrap_or(DEFAULT_PAGE_SIZE)
            .clamp(1, MAX_PAGE_SIZE)
    }
}

pub(super) fn query(request: &ConversationPageRequest) -> Select<conversation::Entity> {
    let mut query = conversation::Entity::find()
        .filter(conversation::Column::DeletedAt.is_null())
        .filter(conversation::Column::Kind.ne(ConversationKind::Loop))
        .filter(conversation_query::non_automation());
    if !request.include_children {
        query = query.filter(conversation::Column::ParentId.is_null());
    }
    query = match request.folder_ids.as_deref() {
        Some(ids) if !ids.is_empty() => {
            query.filter(conversation::Column::FolderId.is_in(ids.iter().copied()))
        }
        _ => query.filter(conversation_query::live_folder()),
    };
    if let Some(agent) = request.agent_type {
        query = query.filter(conversation::Column::AgentType.eq(agent.as_wire().into_owned()));
    }
    if let Some(search) = request.search.as_deref().filter(|value| !value.is_empty()) {
        query = query.filter(conversation::Column::Title.contains(search));
    }
    if let Some(status) = request.status.as_deref().and_then(|value| {
        serde_json::from_value::<ConversationStatus>(serde_json::Value::String(value.to_owned()))
            .ok()
    }) {
        query = query.filter(conversation::Column::Status.eq(status));
    }
    query
}

pub async fn list_page(
    conn: &DatabaseConnection,
    request: &ConversationPageRequest,
) -> Result<ConversationPage, DbError> {
    let limit = request.limit();
    let mut rows = ordered_query(request)
        .limit(limit + 1)
        .into_model::<PageRow>()
        .all(conn)
        .await?;
    let has_more = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    let next_cursor = rows
        .last()
        .filter(|_| has_more)
        .map(|row| ConversationCursor {
            at: row.sort_key.clone(),
            id: row.model.id,
        });
    let mut items: Vec<_> = rows
        .into_iter()
        .map(|row| conv_to_summary(row.model))
        .collect();
    fill_child_counts(conn, &mut items).await?;
    Ok(ConversationPage {
        items,
        next_cursor,
        incomplete: false,
    })
}

fn ordered_query(request: &ConversationPageRequest) -> Select<conversation::Entity> {
    let oldest = request.sort_by.as_deref() == Some("oldest");
    let mut query = query(request);
    if let Some(cursor) = &request.cursor {
        let columns = Expr::tuple([
            Expr::col((conversation::Entity, conversation::Column::UpdatedAt)).into(),
            Expr::col((conversation::Entity, conversation::Column::Id)).into(),
        ]);
        let values = Expr::tuple([
            Expr::value(cursor.at.clone()).into(),
            Expr::value(cursor.id).into(),
        ]);
        query = query.filter(if oldest {
            columns.gt(values)
        } else {
            columns.lt(values)
        });
    }
    let order = if oldest {
        sea_orm::Order::Asc
    } else {
        sea_orm::Order::Desc
    };
    query
        .order_by(conversation::Column::UpdatedAt, order.clone())
        .order_by(conversation::Column::Id, order)
}
