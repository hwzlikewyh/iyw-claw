use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, IntoActiveModel, QueryFilter, QueryOrder, QuerySelect, QueryTrait, Set,
};

use crate::db::entities::conversation::ConversationKind;
use crate::db::entities::{automation_run, conversation, folder};
use crate::db::error::DbError;
use crate::models::{AgentType, DbConversationSummary};

pub async fn create<C: ConnectionTrait>(
    conn: &C,
    folder_id: i32,
    agent_type: AgentType,
    title: Option<String>,
    git_branch: Option<String>,
) -> Result<conversation::Model, DbError> {
    create_inner(
        conn,
        folder_id,
        agent_type,
        title,
        git_branch,
        None,
        ConversationKind::Regular,
    )
    .await
}

/// Mirror of [`create`] for folderless chat-mode conversations: identical row
/// shape but `kind = 'chat'`, so the sidebar routes the row to its flat "Chat"
/// section. Callers must pair it with the hidden chat folder created in the
/// same flow (`create_chat_conversation_core`).
pub async fn create_chat(
    conn: &DatabaseConnection,
    folder_id: i32,
    agent_type: AgentType,
    title: Option<String>,
    git_branch: Option<String>,
) -> Result<conversation::Model, DbError> {
    create_inner(
        conn,
        folder_id,
        agent_type,
        title,
        git_branch,
        None,
        ConversationKind::Chat,
    )
    .await
}

/// Mirror of [`create`] plus optional delegation linkage. Used by the
/// multi-agent broker when spawning a child sub-session — populates
/// `parent_id` / `parent_tool_use_id` / `delegation_call_id` so the lifecycle
/// subscriber and frontend can rebuild the parent ↔ child binding without
/// inspecting the live broker state. `kind` follows the invariant
/// `delegate ⟺ parent_id set`.
pub async fn create_with_delegation(
    conn: &DatabaseConnection,
    folder_id: i32,
    agent_type: AgentType,
    title: Option<String>,
    git_branch: Option<String>,
    delegation: Option<crate::acp::delegation::spawner::DelegationLink>,
) -> Result<conversation::Model, DbError> {
    let kind = if delegation.is_some() {
        ConversationKind::Delegate
    } else {
        ConversationKind::Regular
    };
    create_inner(
        conn, folder_id, agent_type, title, git_branch, delegation, kind,
    )
    .await
}

async fn create_inner<C: ConnectionTrait>(
    conn: &C,
    folder_id: i32,
    agent_type: AgentType,
    title: Option<String>,
    git_branch: Option<String>,
    delegation: Option<crate::acp::delegation::spawner::DelegationLink>,
    kind: ConversationKind,
) -> Result<conversation::Model, DbError> {
    let at_str = serde_json::to_value(agent_type)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    let now = Utc::now();
    let (parent_id, parent_tool_use_id, delegation_call_id) = match delegation {
        Some(link) => (
            Some(link.parent_conversation_id),
            Some(link.parent_tool_use_id),
            Some(link.delegation_call_id),
        ),
        None => (None, None, None),
    };
    let model = conversation::ActiveModel {
        id: NotSet,
        folder_id: Set(folder_id),
        title: Set(title),
        title_locked: Set(false),
        agent_type: Set(at_str),
        status: Set(conversation::ConversationStatus::InProgress),
        kind: Set(kind),
        model: Set(None),
        git_branch: Set(git_branch),
        external_id: Set(None),
        parent_id: Set(parent_id),
        parent_tool_use_id: Set(parent_tool_use_id),
        delegation_call_id: Set(delegation_call_id),
        message_count: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        pinned_at: Set(None),
    };
    Ok(model.insert(conn).await?)
}

pub async fn update_status(
    conn: &DatabaseConnection,
    conversation_id: i32,
    status: conversation::ConversationStatus,
) -> Result<(), DbError> {
    let conv = conversation::Entity::find_by_id(conversation_id)
        .one(conn)
        .await?
        .ok_or_else(|| DbError::Migration(format!("Conversation not found: {conversation_id}")))?;
    let mut active: conversation::ActiveModel = conv.into();
    active.status = Set(status);
    active.updated_at = Set(Utc::now());
    active.update(conn).await?;
    Ok(())
}

/// Conditional status transition (CAS): write `new_status` only if the row's
/// current `status` equals `expected`. Returns `true` when the row was
/// updated. Used by the lifecycle subscriber on disconnect/error so a
/// concurrent user-driven `completed` (or a prior `pending_review` from
/// `TurnComplete`) cannot be silently overwritten.
pub async fn update_status_if(
    conn: &DatabaseConnection,
    conversation_id: i32,
    expected: conversation::ConversationStatus,
    new_status: conversation::ConversationStatus,
) -> Result<bool, DbError> {
    use sea_orm::sea_query::Expr;
    let result = conversation::Entity::update_many()
        .col_expr(conversation::Column::Status, Expr::value(new_status))
        .col_expr(conversation::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(conversation::Column::Id.eq(conversation_id))
        .filter(conversation::Column::Status.eq(expected))
        .exec(conn)
        .await?;
    Ok(result.rows_affected > 0)
}

/// Manual rename: set the title AND lock it. Once locked, the per-turn
/// auto-title backfill ([`refresh_auto_title`]) leaves this row alone, so the
/// user's hand-picked name survives every subsequent session-file parse.
pub async fn update_title(
    conn: &DatabaseConnection,
    conversation_id: i32,
    title: String,
) -> Result<(), DbError> {
    let conv = conversation::Entity::find_by_id(conversation_id)
        .one(conn)
        .await?
        .ok_or_else(|| DbError::Migration(format!("Conversation not found: {conversation_id}")))?;
    let mut active: conversation::ActiveModel = conv.into();
    active.title = Set(Some(title));
    active.title_locked = Set(true);
    active.updated_at = Set(Utc::now());
    active.update(conn).await?;
    Ok(())
}

/// Auto-derive counterpart to [`update_title`]: write `title` ONLY when the row
/// is not user-locked and the value actually changed. Never sets `title_locked`
/// (the title stays eligible for future auto-refreshes, e.g. when an agent like
/// OpenCode regenerates its own session title) and deliberately does NOT bump
/// `updated_at` — a title backfill is metadata, not user activity, so it must
/// not float the row to the top of a recency-sorted sidebar. Returns `true`
/// when a row was written so the caller can broadcast a sidebar upsert.
///
/// Implemented as a single conditional UPDATE (`... WHERE id = ? AND
/// title_locked = false AND (title IS NULL OR title <> ?)`) so the lock/equality
/// checks and the write are atomic: a manual rename ([`update_title`], which
/// sets `title_locked = true`) that lands between a would-be read and the write
/// can never be clobbered, because the lock predicate is re-evaluated at write
/// time by the database. A non-existent or soft-deleted row simply matches
/// nothing (`false`).
pub async fn refresh_auto_title(
    conn: &DatabaseConnection,
    conversation_id: i32,
    title: String,
) -> Result<bool, DbError> {
    use sea_orm::sea_query::Expr;
    let title = title.trim();
    if title.is_empty() {
        return Ok(false);
    }
    let res = conversation::Entity::update_many()
        .col_expr(conversation::Column::Title, Expr::value(title))
        .filter(conversation::Column::Id.eq(conversation_id))
        .filter(conversation::Column::DeletedAt.is_null())
        .filter(conversation::Column::TitleLocked.eq(false))
        .filter(
            sea_orm::Condition::any()
                .add(conversation::Column::Title.is_null())
                .add(conversation::Column::Title.ne(title)),
        )
        .exec(conn)
        .await?;
    Ok(res.rows_affected > 0)
}

/// Pin or unpin a conversation. Sets `pinned_at = now()` when pinning, `NULL`
/// when unpinning. Only the `pinned_at` column is written — `updated_at` is
/// deliberately left untouched (SeaORM updates only the `Set` field), because
/// pinning is a view preference, not conversation activity, and must not float
/// the row to the top of a recency-sorted sidebar (same reasoning as
/// [`refresh_auto_title`]). The sidebar's "Pinned" section orders by `pinned_at`
/// descending, so a freshly pinned conversation jumps to the top.
pub async fn update_pin(
    conn: &DatabaseConnection,
    conversation_id: i32,
    pinned: bool,
) -> Result<(), DbError> {
    let conv = conversation::Entity::find_by_id(conversation_id)
        .one(conn)
        .await?
        .ok_or_else(|| DbError::Migration(format!("Conversation not found: {conversation_id}")))?;
    let mut active: conversation::ActiveModel = conv.into();
    active.pinned_at = Set(pinned.then(Utc::now));
    active.update(conn).await?;
    Ok(())
}

pub async fn update_external_id(
    conn: &DatabaseConnection,
    conversation_id: i32,
    external_id: String,
) -> Result<(), DbError> {
    use sea_orm::sea_query::Expr;

    conversation::Entity::update_many()
        .col_expr(conversation::Column::ExternalId, Expr::value(external_id))
        .col_expr(conversation::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(conversation::Column::Id.eq(conversation_id))
        .filter(conversation::Column::DeletedAt.is_null())
        .exec(conn)
        .await?;
    Ok(())
}

/// Persist the model name used in a conversation. Called by the lifecycle
/// subscriber when `TurnComplete` fires so each conversation row remembers
/// the model that was active during its turns. `None` clears the field.
/// Uses `update_many` for a direct column-level UPDATE (no round-trip fetch).
pub async fn update_model(
    conn: &DatabaseConnection,
    conversation_id: i32,
    model: Option<String>,
) -> Result<(), DbError> {
    use sea_orm::sea_query::Expr;

    conversation::Entity::update_many()
        .col_expr(conversation::Column::Model, Expr::value(model))
        .col_expr(conversation::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(conversation::Column::Id.eq(conversation_id))
        .filter(conversation::Column::DeletedAt.is_null())
        .exec(conn)
        .await?;
    Ok(())
}

pub async fn soft_delete(conn: &DatabaseConnection, conversation_id: i32) -> Result<(), DbError> {
    let conv = conversation::Entity::find_by_id(conversation_id)
        .filter(conversation::Column::DeletedAt.is_null())
        .one(conn)
        .await?
        .ok_or_else(|| DbError::Migration(format!("Conversation not found: {conversation_id}")))?;
    let mut active: conversation::ActiveModel = conv.into();
    active.deleted_at = Set(Some(Utc::now()));
    active.update(conn).await?;
    Ok(())
}

fn parse_agent_type(s: &str) -> AgentType {
    match serde_json::from_value(serde_json::Value::String(s.to_string())) {
        Ok(at) => at,
        Err(_) => {
            // DB has a value the enum does not recognise (manual edit or removed variant).
            // Fall back to ClaudeCode so the row stays readable, but log so resume-as-wrong-agent
            // regressions are traceable.
            tracing::warn!(
                "[conversation_service] unknown agent_type {s:?} in DB, falling back to ClaudeCode"
            );
            AgentType::ClaudeCode
        }
    }
}

fn conv_to_summary(r: conversation::Model) -> DbConversationSummary {
    let status = serde_json::to_value(&r.status)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| format!("{:?}", r.status));
    DbConversationSummary {
        id: r.id,
        folder_id: r.folder_id,
        title: r.title,
        title_locked: r.title_locked,
        agent_type: parse_agent_type(&r.agent_type),
        status,
        kind: r.kind.clone(),
        model: r.model,
        git_branch: r.git_branch,
        external_id: r.external_id,
        message_count: r.message_count as u32,
        // Pure mapper: `child_count` is backfilled by `fill_child_counts` over
        // the returned set, never queried per-row here.
        child_count: 0,
        created_at: r.created_at,
        updated_at: r.updated_at,
        pinned_at: r.pinned_at,
        parent_id: r.parent_id,
        parent_tool_use_id: r.parent_tool_use_id,
        delegation_call_id: r.delegation_call_id,
    }
}

/// Backfill each summary's `child_count` with its number of direct, non-deleted
/// delegation children using ONE `GROUP BY` aggregate over the whole set (never
/// per-row — no N+1). `child_count > 0` iff `list_children` would return rows
/// (same `parent_id == id AND deleted_at IS NULL` predicate), so the sidebar
/// chevron neither expands to nothing nor hides a real subtree. No-op on an
/// empty slice (avoids an `IN ()`).
async fn fill_child_counts(
    conn: &DatabaseConnection,
    summaries: &mut [DbConversationSummary],
) -> Result<(), DbError> {
    if summaries.is_empty() {
        return Ok(());
    }
    let ids: Vec<i32> = summaries.iter().map(|s| s.id).collect();
    let pairs: Vec<(Option<i32>, i64)> = conversation::Entity::find()
        .select_only()
        .column(conversation::Column::ParentId)
        .column_as(conversation::Column::Id.count(), "cnt")
        .filter(conversation::Column::ParentId.is_in(ids))
        .filter(conversation::Column::DeletedAt.is_null())
        .group_by(conversation::Column::ParentId)
        .into_tuple()
        .all(conn)
        .await?;
    let mut counts: std::collections::HashMap<i32, u32> =
        std::collections::HashMap::with_capacity(pairs.len());
    for (parent_id, cnt) in pairs {
        if let Some(pid) = parent_id {
            counts.insert(pid, cnt.max(0) as u32);
        }
    }
    for s in summaries.iter_mut() {
        s.child_count = counts.get(&s.id).copied().unwrap_or(0);
    }
    Ok(())
}

pub async fn get_by_id(
    conn: &DatabaseConnection,
    conversation_id: i32,
) -> Result<DbConversationSummary, DbError> {
    let conv = conversation::Entity::find_by_id(conversation_id)
        .filter(conversation::Column::DeletedAt.is_null())
        .one(conn)
        .await?
        .ok_or_else(|| DbError::Migration(format!("Conversation not found: {conversation_id}")))?;

    let mut summary = conv_to_summary(conv);
    fill_child_counts(conn, std::slice::from_mut(&mut summary)).await?;
    Ok(summary)
}

pub async fn bind_delegation_parent_tool_call(
    conn: &DatabaseConnection,
    conversation_id: i32,
    parent_conversation_id: i32,
    delegation_call_id: &str,
    parent_tool_use_id: &str,
) -> Result<bool, DbError> {
    let Some(row) = conversation::Entity::find_by_id(conversation_id)
        .filter(conversation::Column::DeletedAt.is_null())
        .one(conn)
        .await?
    else {
        return Ok(false);
    };
    if row.parent_id != Some(parent_conversation_id)
        || row.delegation_call_id.as_deref() != Some(delegation_call_id)
    {
        return Ok(false);
    }
    if row.parent_tool_use_id.as_deref() == Some(parent_tool_use_id) {
        return Ok(true);
    }
    let mut active = row.into_active_model();
    active.parent_tool_use_id = Set(Some(parent_tool_use_id.to_string()));
    active.updated_at = Set(Utc::now());
    active.update(conn).await?;
    Ok(true)
}

/// Look up a child conversation by its `delegation_call_id` (the broker's
/// `task_id`). Returns `Ok(None)` when no row matches — used by the broker's
/// `ChildStatusLookup` DB fallback to recover a delegation task's terminal
/// status after its in-memory result was evicted from the completed-cache.
/// Unlike [`get_by_id`] this never errors hard on "not found": a missing row
/// is a legitimate "unknown task" answer.
pub async fn get_by_delegation_call_id(
    conn: &DatabaseConnection,
    delegation_call_id: &str,
) -> Result<Option<DbConversationSummary>, DbError> {
    let conv = conversation::Entity::find()
        .filter(conversation::Column::DelegationCallId.eq(delegation_call_id))
        .filter(conversation::Column::DeletedAt.is_null())
        .one(conn)
        .await?;
    Ok(conv.map(conv_to_summary))
}

/// Resolve a persisted Agent session to its workspace. This is only used when
/// a resume caller omits `working_dir`; new sessions never get an implicit cwd.
pub async fn find_folder_path_by_external_id(
    conn: &DatabaseConnection,
    external_id: &str,
    agent_type: AgentType,
) -> Result<Option<String>, DbError> {
    let agent_type = serde_json::to_value(agent_type)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default();
    let row = conversation::Entity::find()
        .filter(conversation::Column::ExternalId.eq(external_id))
        .filter(conversation::Column::AgentType.eq(agent_type))
        .filter(conversation::Column::DeletedAt.is_null())
        .find_also_related(folder::Entity)
        .one(conn)
        .await?;
    Ok(row
        .and_then(|(_, folder)| folder)
        .filter(|folder| folder.deleted_at.is_none())
        .map(|folder| folder.path))
}

pub async fn list_by_folder(
    conn: &DatabaseConnection,
    folder_id: i32,
    agent_type: Option<AgentType>,
    search: Option<String>,
    sort_by: Option<String>,
    status: Option<String>,
) -> Result<Vec<DbConversationSummary>, DbError> {
    let mut query = conversation::Entity::find()
        .filter(conversation::Column::FolderId.eq(folder_id))
        .filter(conversation::Column::DeletedAt.is_null());

    // Keep automation-owned run conversations out of every ordinary history
    // query, including folder-scoped callers. The automation detail view loads
    // them explicitly by id from the run record.
    let automation_conversations = automation_run::Entity::find()
        .select_only()
        .column(automation_run::Column::ConversationId)
        .filter(automation_run::Column::ConversationId.is_not_null())
        .into_query();
    query = query.filter(
        conversation::Column::Id
            .into_expr()
            .not_in_subquery(automation_conversations),
    );

    // Filter by agent_type
    if let Some(ref at) = agent_type {
        let at_str = serde_json::to_value(at)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();
        query = query.filter(conversation::Column::AgentType.eq(at_str));
    }

    // Search by title
    if let Some(ref s) = search {
        if !s.is_empty() {
            query = query.filter(conversation::Column::Title.contains(s));
        }
    }

    // Filter by status
    if let Some(ref st) = status {
        if let Ok(status_enum) = serde_json::from_value::<conversation::ConversationStatus>(
            serde_json::Value::String(st.clone()),
        ) {
            query = query.filter(conversation::Column::Status.eq(status_enum));
        }
    }

    // Sort
    query = match sort_by.as_deref() {
        Some("oldest") => query.order_by_asc(conversation::Column::CreatedAt),
        _ => query.order_by_desc(conversation::Column::CreatedAt),
    };

    let rows = query.all(conn).await?;

    let mut summaries: Vec<DbConversationSummary> = rows.into_iter().map(conv_to_summary).collect();
    fill_child_counts(conn, &mut summaries).await?;

    Ok(summaries)
}

/// List conversations across folders. When `folder_ids` is `None`, queries all
/// When `folder_ids` is provided, results are scoped to that set. Otherwise
/// returns conversations across every non-deleted folder (open or not).
///
/// `include_children` controls visibility of delegation sub-sessions. When
/// `false` (the default for the top-level list), rows whose `parent_id` is
/// non-null are filtered out — they belong to their parent's tool-call view,
/// not the workspace conversation list. Rows with `kind = 'loop'` are always
/// excluded — they belong to the loops workbench.
pub async fn list_all(
    conn: &DatabaseConnection,
    folder_ids: Option<Vec<i32>>,
    agent_type: Option<AgentType>,
    search: Option<String>,
    sort_by: Option<String>,
    status: Option<String>,
    include_children: bool,
) -> Result<Vec<DbConversationSummary>, DbError> {
    let mut query = conversation::Entity::find().filter(conversation::Column::DeletedAt.is_null());

    // Loop-engineering runs never surface in the workspace conversation list —
    // their entry point is the loops workbench.
    query = query.filter(conversation::Column::Kind.ne(ConversationKind::Loop));

    // Automation runs are intentionally reachable only from the automation
    // detail view. Keep them out of the normal workspace history even when
    // their generated conversation uses a regular folder.
    let automation_conversations = automation_run::Entity::find()
        .select_only()
        .column(automation_run::Column::ConversationId)
        .filter(automation_run::Column::ConversationId.is_not_null())
        .into_query();
    query = query.filter(
        conversation::Column::Id
            .into_expr()
            .not_in_subquery(automation_conversations),
    );

    if !include_children {
        query = query.filter(conversation::Column::ParentId.is_null());
    }

    match folder_ids {
        Some(ids) if !ids.is_empty() => {
            query = query.filter(conversation::Column::FolderId.is_in(ids));
        }
        _ => {
            // Exclude conversations whose folder was soft-deleted.
            let active_folder_ids: Vec<i32> = folder::Entity::find()
                .filter(folder::Column::DeletedAt.is_null())
                .all(conn)
                .await?
                .into_iter()
                .map(|m| m.id)
                .collect();
            if active_folder_ids.is_empty() {
                return Ok(Vec::new());
            }
            query = query.filter(conversation::Column::FolderId.is_in(active_folder_ids));
        }
    }

    if let Some(ref at) = agent_type {
        let at_str = serde_json::to_value(at)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();
        query = query.filter(conversation::Column::AgentType.eq(at_str));
    }

    if let Some(ref s) = search {
        if !s.is_empty() {
            query = query.filter(conversation::Column::Title.contains(s));
        }
    }

    if let Some(ref st) = status {
        if let Ok(status_enum) = serde_json::from_value::<conversation::ConversationStatus>(
            serde_json::Value::String(st.clone()),
        ) {
            query = query.filter(conversation::Column::Status.eq(status_enum));
        }
    }

    query = match sort_by.as_deref() {
        Some("oldest") => query.order_by_asc(conversation::Column::UpdatedAt),
        _ => query.order_by_desc(conversation::Column::UpdatedAt),
    };

    let rows = query.all(conn).await?;
    let mut summaries: Vec<DbConversationSummary> = rows.into_iter().map(conv_to_summary).collect();
    fill_child_counts(conn, &mut summaries).await?;
    Ok(summaries)
}

/// List delegation children of a single parent conversation, newest first.
/// Returns rows where `parent_id == parent_conversation_id`. Soft-deleted
/// children are filtered out so a removed sub-session stays hidden in the
/// parent's tool-call view too.
pub async fn list_children(
    conn: &DatabaseConnection,
    parent_conversation_id: i32,
) -> Result<Vec<DbConversationSummary>, DbError> {
    let rows = conversation::Entity::find()
        .filter(conversation::Column::ParentId.eq(parent_conversation_id))
        .filter(conversation::Column::DeletedAt.is_null())
        .order_by_desc(conversation::Column::CreatedAt)
        .order_by_desc(conversation::Column::Id)
        .all(conn)
        .await?;
    let mut summaries: Vec<DbConversationSummary> = rows.into_iter().map(conv_to_summary).collect();
    fill_child_counts(conn, &mut summaries).await?;
    Ok(summaries)
}
