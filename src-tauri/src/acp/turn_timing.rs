use chrono::{DateTime, Utc};
use sea_orm::sea_query::OnConflict;
use sea_orm::{ActiveValue::NotSet, DatabaseConnection, EntityTrait, Set};
use serde::{Deserialize, Serialize};

use crate::db::entities::app_metadata;
use crate::db::error::DbError;
use crate::db::service::app_metadata_service;
use crate::models::{DbConversationDetail, MessageTurn, TurnRole};

use super::manager::ConnectionManager;
use super::session_activity::SessionActivitySnapshot;

const KEY_PREFIX: &str = "conversation.turn-timing.v1:";

#[derive(Serialize, Deserialize)]
struct TurnTiming {
    started_at: DateTime<Utc>,
    completed_at: DateTime<Utc>,
}

impl TurnTiming {
    fn from_activity(activity: &SessionActivitySnapshot) -> Option<Self> {
        let started_at = activity.started_at?;
        (activity.sampled_at >= started_at).then_some(Self {
            started_at,
            completed_at: activity.sampled_at,
        })
    }
}

pub(super) async fn record(
    conn: &DatabaseConnection,
    conversation_id: i32,
    activity: Option<&SessionActivitySnapshot>,
) -> Result<(), DbError> {
    let Some(timing) = activity.and_then(TurnTiming::from_activity) else {
        tracing::warn!(
            conversation_id,
            "[turn-timing] completion has no valid timing"
        );
        return Ok(());
    };
    let key = format!(
        "{KEY_PREFIX}{conversation_id}:{}",
        timing.started_at.timestamp_micros()
    );
    let value =
        serde_json::to_string(&timing).map_err(|error| DbError::Validation(error.to_string()))?;
    let now = Utc::now();
    // 复用现有元数据表，以轮次起点去重；重放只能读到首次结算结果。
    app_metadata::Entity::insert(app_metadata::ActiveModel {
        id: NotSet,
        key: Set(key),
        value: Set(value),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: NotSet,
    })
    .on_conflict(
        OnConflict::column(app_metadata::Column::Key)
            .do_nothing()
            .to_owned(),
    )
    .do_nothing()
    .exec(conn)
    .await?;
    tracing::info!(conversation_id, started_at = %timing.started_at,
        completed_at = %timing.completed_at,
        duration_ms = (timing.completed_at - timing.started_at).num_milliseconds(),
        "[turn-timing] completed timing persisted");
    Ok(())
}

pub(crate) async fn enrich(
    conn: &DatabaseConnection,
    manager: &ConnectionManager,
    detail: &mut DbConversationDetail,
) -> Result<(), DbError> {
    let prefix = format!("{KEY_PREFIX}{}:", detail.summary.id);
    let partial = detail.history_start > 0;
    for value in app_metadata_service::list_values_by_key_prefix(conn, &prefix).await? {
        let timing = serde_json::from_str::<TurnTiming>(&value)
            .map_err(|error| DbError::Validation(format!("invalid turn timing: {error}")))?;
        apply(&mut detail.turns, &timing, partial);
    }
    // 完成事件先广播、生命周期随后落盘；覆盖这段窗口内的历史读取。
    if let Some(connection_id) = manager
        .find_connection_by_conversation_id(detail.summary.id)
        .await
    {
        if let Some((state, _)) = manager.get_state_and_emitter(&connection_id).await {
            let activity = state.read().await.activity.completed_snapshot();
            if let Some(timing) = activity.as_ref().and_then(TurnTiming::from_activity) {
                apply(&mut detail.turns, &timing, partial);
            }
        }
    }
    Ok(())
}

fn apply(turns: &mut [MessageTurn], timing: &TurnTiming, partial: bool) {
    let mut started_at = timing.started_at;
    let mut assistant_seen = false;
    for index in 0..turns.len() {
        let turn = &turns[index];
        if turn.timestamp < timing.started_at || turn.timestamp > timing.completed_at {
            continue;
        }
        if matches!(turn.role, TurnRole::User) {
            if assistant_seen {
                started_at = turn.timestamp;
            }
            continue;
        }
        if !matches!(turn.role, TurnRole::Assistant) {
            continue;
        }
        assistant_seen = true;
        let next = turns[index + 1..].iter().find(|next| {
            next.timestamp <= timing.completed_at
                && matches!(next.role, TurnRole::User | TurnRole::Assistant)
        });
        if next.is_some_and(|next| matches!(next.role, TurnRole::Assistant)) {
            turns[index].duration_ms = None;
            continue;
        }
        let end = next.map_or(timing.completed_at, |next| next.timestamp);
        if partial && turns[index].completed_at == Some(end) && turns[index].duration_ms.is_some() {
            continue;
        }
        let duration = u64::try_from((end - started_at).num_milliseconds()).ok();
        turns[index].duration_ms = duration;
        turns[index].completed_at = Some(end);
    }
}
