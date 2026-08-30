//! Automation CRUD + cron scheduling math. Mode-agnostic: every fn takes a plain
//! `&DatabaseConnection` so both the Tauri command and the Axum handler share it.
//! `config` is stored as an opaque JSON string and replayed wholesale at fire.

use std::str::FromStr;

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use cron::Schedule;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set,
};

use crate::db::entities::automation::{IsolationMode, TriggerKind};
use crate::db::entities::{automation, automation_run, conversation};
use crate::db::error::DbError;
use crate::models::{
    AutomationConfig, AutomationDraft, AutomationInfo, AutomationRunInfo, AutomationRunStatus,
};

fn normalize_legacy_isolation(m: &mut automation::Model) {
    if m.isolation != IsolationMode::WorktreePerRun {
        return;
    }
    m.isolation = IsolationMode::SharedInRoot;
    m.branch = None;
    m.is_remote_branch = false;
}

fn normalize_draft(mut draft: AutomationDraft) -> AutomationDraft {
    if draft.isolation == IsolationMode::WorktreePerRun {
        tracing::info!(
            requested_isolation = "worktree_per_run",
            effective_isolation = "shared_in_root",
            "normalized deprecated automation isolation"
        );
        draft.isolation = IsolationMode::SharedInRoot;
        draft.branch = None;
        draft.is_remote_branch = false;
    }
    draft
}

fn to_info(mut m: automation::Model) -> AutomationInfo {
    normalize_legacy_isolation(&mut m);
    AutomationInfo {
        id: m.id,
        name: m.name,
        enabled: m.enabled,
        trigger_kind: m.trigger_kind,
        cron: m.cron,
        timezone: m.timezone,
        next_run_at: m.next_run_at,
        agent_type: m.agent_type,
        root_folder_id: m.root_folder_id,
        isolation: m.isolation,
        branch: m.branch,
        is_remote_branch: m.is_remote_branch,
        config: serde_json::from_str(&m.config).unwrap_or(serde_json::Value::Null),
        last_run_at: m.last_run_at,
        last_run_status: m.last_run_status,
        last_run_conversation_id: m.last_run_conversation_id,
        unseen_failures: m.unseen_failures,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

fn run_to_info(m: automation_run::Model) -> AutomationRunInfo {
    AutomationRunInfo {
        id: m.id,
        automation_id: m.automation_id,
        status: m.status,
        trigger: m.trigger,
        scheduled_for: m.scheduled_for,
        started_at: m.started_at,
        ended_at: m.ended_at,
        conversation_id: m.conversation_id,
        worktree_folder_id: m.worktree_folder_id,
        stop_reason: m.stop_reason,
        error: m.error,
        summary: m.summary,
        created_at: m.created_at,
    }
}

// ── cron math ──────────────────────────────────────────────────────────────

/// Translate the day-of-week field from the UI/POSIX convention (0-6 = Sun-Sat,
/// with 7 also = Sun) to the `cron` crate's convention (1-7 = Sun-Sat). The
/// builder, humanizer, and templates all speak 0-6, but `cron` 0.12 evaluates
/// `weekday().number_from_sunday()` (Sun=1 .. Sat=7) and rejects 0 — so without
/// this every weekly automation would fire a day early and Sunday would be
/// unschedulable. Numeric tokens are expanded to an explicit set, shifted by
/// `(n % 7) + 1`, then re-emitted as a sorted list — this also sidesteps
/// wrap-around ranges (`6-7` → would be `7-1`). Symbolic day names (`mon`, …)
/// are passed through untouched: the crate's own name table is self-consistent.
fn remap_dow_field(field: &str) -> Result<String, DbError> {
    let field = field.trim();
    if field == "*" {
        return Ok("*".to_string());
    }
    // Day names are already crate-native; don't touch them.
    if field.chars().any(|c| c.is_ascii_alphabetic()) {
        return Ok(field.to_string());
    }
    let invalid = || DbError::Validation(format!("invalid cron day-of-week field '{field}'"));
    let mut days: Vec<u32> = Vec::new();
    for part in field.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return Err(invalid());
        }
        // Optional step: `BASE/STEP`.
        let (base, step) = match part.split_once('/') {
            Some((b, s)) => {
                let step: u32 = s.trim().parse().map_err(|_| invalid())?;
                if step == 0 {
                    return Err(invalid());
                }
                (b.trim(), step)
            }
            None => (part, 1),
        };
        // Resolve the base into an inclusive [lo, hi] range over the UI domain.
        let (lo, hi) = if base == "*" {
            (0u32, 6u32)
        } else if let Some((a, b)) = base.split_once('-') {
            let a: u32 = a.trim().parse().map_err(|_| invalid())?;
            let b: u32 = b.trim().parse().map_err(|_| invalid())?;
            if a > b {
                // Wrap-around ranges (e.g. `5-1`) are unsupported; use a list.
                return Err(invalid());
            }
            (a, b)
        } else {
            let n: u32 = base.parse().map_err(|_| invalid())?;
            (n, n)
        };
        if hi > 7 {
            return Err(invalid());
        }
        for d in (lo..=hi).step_by(step as usize) {
            days.push((d % 7) + 1);
        }
    }
    days.sort_unstable();
    days.dedup();
    if days.is_empty() {
        return Err(invalid());
    }
    Ok(days
        .iter()
        .map(|d| d.to_string())
        .collect::<Vec<_>>()
        .join(","))
}

/// Normalize a user-supplied 5-field cron (`min hour dom mon dow`) to the
/// 6-field form (`sec min hour dom mon dow`) the `cron` crate requires, by
/// remapping the day-of-week field (see [`remap_dow_field`]) and prepending a
/// zero-seconds field. 6/7-field expressions are assumed crate-native and pass
/// through unchanged (nothing first-party emits them).
fn normalize_cron(expr: &str) -> Result<String, DbError> {
    let trimmed = expr.trim();
    let fields: Vec<&str> = trimmed.split_whitespace().collect();
    match fields.len() {
        5 => {
            let dow = remap_dow_field(fields[4])?;
            Ok(format!(
                "0 {} {} {} {} {}",
                fields[0], fields[1], fields[2], fields[3], dow
            ))
        }
        6 | 7 => Ok(trimmed.to_string()),
        n => Err(DbError::Validation(format!(
            "cron must have 5 fields (min hour dom mon dow), got {n}"
        ))),
    }
}

/// Next fire instant (UTC) of a 5-field cron evaluated in `timezone`, strictly
/// after `after`. `None` when the schedule has no future occurrence. This is the
/// single source of truth shared by create/update, the scheduler, and the
/// editor's "next run" preview — so preview and actual fire can never diverge.
pub fn compute_next_run(
    cron_expr: &str,
    timezone: &str,
    after: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>, DbError> {
    let normalized = normalize_cron(cron_expr)?;
    let schedule = Schedule::from_str(&normalized)
        .map_err(|e| DbError::Validation(format!("invalid cron '{cron_expr}': {e}")))?;
    let tz: Tz = timezone
        .parse()
        .map_err(|_| DbError::Validation(format!("invalid timezone '{timezone}'")))?;
    let after_tz = after.with_timezone(&tz);
    let next = schedule.after(&after_tz).next();
    Ok(next.map(|dt| dt.with_timezone(&Utc)))
}

fn validate_draft(draft: &AutomationDraft) -> Result<(), DbError> {
    if draft.name.trim().is_empty() {
        return Err(DbError::Validation("name is required".into()));
    }
    let cfg: AutomationConfig = serde_json::from_value(draft.config.clone()).unwrap_or_default();
    if cfg.display_text.trim().is_empty() && cfg.prompt_blocks.is_empty() {
        return Err(DbError::Validation("prompt is required".into()));
    }
    // Automations now run in the selected folder, where a remote-only branch
    // cannot be checked out safely. Reject direct API attempts as the UI only
    // offers local branches.
    if draft.is_remote_branch {
        return Err(DbError::Validation(
            "remote branches are not supported for automations; choose a local branch".into(),
        ));
    }
    if draft.trigger_kind == TriggerKind::Schedule {
        let cron = draft.cron.as_deref().unwrap_or("").trim();
        if cron.is_empty() {
            return Err(DbError::Validation(
                "cron is required for scheduled automations".into(),
            ));
        }
        // Parses cron + timezone (and surfaces an error before we ever store it).
        compute_next_run(cron, &draft.timezone, Utc::now())?;
    }
    Ok(())
}

/// The `next_run_at` to persist for a draft: only scheduled + enabled automations
/// have one. Manual or disabled automations store `None` (scheduler skips them).
fn next_run_for(
    draft: &AutomationDraft,
    now: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>, DbError> {
    if !draft.enabled || draft.trigger_kind != TriggerKind::Schedule {
        return Ok(None);
    }
    compute_next_run(draft.cron.as_deref().unwrap_or(""), &draft.timezone, now)
}

// ── CRUD ───────────────────────────────────────────────────────────────────

async fn find_active(conn: &DatabaseConnection, id: i32) -> Result<automation::Model, DbError> {
    let row = automation::Entity::find_by_id(id)
        .one(conn)
        .await?
        .ok_or_else(|| DbError::NotFound(format!("automation {id}")))?;
    if row.deleted_at.is_some() {
        return Err(DbError::NotFound(format!("automation {id}")));
    }
    Ok(row)
}

pub async fn list(conn: &DatabaseConnection) -> Result<Vec<AutomationInfo>, DbError> {
    let rows = automation::Entity::find()
        .filter(automation::Column::DeletedAt.is_null())
        .order_by_desc(automation::Column::UpdatedAt)
        .all(conn)
        .await?;
    Ok(rows.into_iter().map(to_info).collect())
}

pub async fn get(conn: &DatabaseConnection, id: i32) -> Result<AutomationInfo, DbError> {
    Ok(to_info(find_active(conn, id).await?))
}

pub async fn list_runs(
    conn: &DatabaseConnection,
    automation_id: i32,
    limit: u64,
) -> Result<Vec<AutomationRunInfo>, DbError> {
    let rows = automation_run::Entity::find()
        .filter(automation_run::Column::AutomationId.eq(automation_id))
        .order_by_desc(automation_run::Column::CreatedAt)
        .limit(limit)
        .all(conn)
        .await?;
    Ok(rows.into_iter().map(run_to_info).collect())
}

pub async fn create(
    conn: &DatabaseConnection,
    draft: AutomationDraft,
) -> Result<AutomationInfo, DbError> {
    let draft = normalize_draft(draft);
    validate_draft(&draft)?;
    let now = Utc::now();
    let next_run_at = next_run_for(&draft, now)?;
    let config_str = serde_json::to_string(&draft.config)
        .map_err(|e| DbError::Validation(format!("config not serializable: {e}")))?;

    let active = automation::ActiveModel {
        id: NotSet,
        name: Set(draft.name.trim().to_string()),
        enabled: Set(draft.enabled),
        trigger_kind: Set(draft.trigger_kind),
        cron: Set(draft.cron),
        timezone: Set(draft.timezone),
        next_run_at: Set(next_run_at),
        agent_type: Set(draft.agent_type),
        root_folder_id: Set(draft.root_folder_id),
        isolation: Set(draft.isolation),
        branch: Set(draft.branch),
        is_remote_branch: Set(draft.is_remote_branch),
        config: Set(config_str),
        last_run_at: Set(None),
        last_run_status: Set(None),
        last_run_conversation_id: Set(None),
        unseen_failures: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
    };
    Ok(to_info(active.insert(conn).await?))
}

pub async fn update(
    conn: &DatabaseConnection,
    id: i32,
    draft: AutomationDraft,
) -> Result<AutomationInfo, DbError> {
    let draft = normalize_draft(draft);
    validate_draft(&draft)?;
    let row = find_active(conn, id).await?;
    let now = Utc::now();
    let next_run_at = next_run_for(&draft, now)?;
    let config_str = serde_json::to_string(&draft.config)
        .map_err(|e| DbError::Validation(format!("config not serializable: {e}")))?;

    let mut active = row.into_active_model();
    active.name = Set(draft.name.trim().to_string());
    active.enabled = Set(draft.enabled);
    active.trigger_kind = Set(draft.trigger_kind);
    active.cron = Set(draft.cron);
    active.timezone = Set(draft.timezone);
    active.next_run_at = Set(next_run_at);
    active.agent_type = Set(draft.agent_type);
    active.root_folder_id = Set(draft.root_folder_id);
    active.isolation = Set(draft.isolation);
    active.branch = Set(draft.branch);
    active.is_remote_branch = Set(draft.is_remote_branch);
    active.config = Set(config_str);
    active.updated_at = Set(now);
    Ok(to_info(active.update(conn).await?))
}

/// Bind a generated default folder without overwriting a concurrently selected
/// explicit folder. Returns the folder id that won the compare-and-set.
pub async fn set_root_folder_if_missing(
    conn: &DatabaseConnection,
    id: i32,
    folder_id: i32,
) -> Result<i32, DbError> {
    let result = automation::Entity::update_many()
        .col_expr(automation::Column::RootFolderId, Expr::value(folder_id))
        .col_expr(automation::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(automation::Column::Id.eq(id))
        .filter(automation::Column::DeletedAt.is_null())
        .filter(automation::Column::RootFolderId.is_null())
        .exec(conn)
        .await?;
    if result.rows_affected == 1 {
        return Ok(folder_id);
    }
    find_active(conn, id)
        .await?
        .root_folder_id
        .ok_or_else(|| DbError::Validation(format!("automation {id} has no target folder")))
}

pub async fn set_enabled(
    conn: &DatabaseConnection,
    id: i32,
    enabled: bool,
) -> Result<AutomationInfo, DbError> {
    let row = find_active(conn, id).await?;
    let now = Utc::now();
    let next_run_at = if enabled && row.trigger_kind == TriggerKind::Schedule {
        compute_next_run(row.cron.as_deref().unwrap_or(""), &row.timezone, now)?
    } else {
        None
    };
    let mut active = row.into_active_model();
    active.enabled = Set(enabled);
    active.next_run_at = Set(next_run_at);
    active.updated_at = Set(now);
    Ok(to_info(active.update(conn).await?))
}

/// Soft-delete: hide from the list, stop scheduling. Run history is retained.
pub async fn delete(conn: &DatabaseConnection, id: i32) -> Result<(), DbError> {
    let row = find_active(conn, id).await?;
    let mut active = row.into_active_model();
    active.deleted_at = Set(Some(Utc::now()));
    active.enabled = Set(false);
    active.next_run_at = Set(None);
    active.update(conn).await?;
    Ok(())
}

/// Clear all unseen-failure badges (called when the user opens the view).
pub async fn mark_all_seen(conn: &DatabaseConnection) -> Result<(), DbError> {
    automation::Entity::update_many()
        .col_expr(automation::Column::UnseenFailures, Expr::value(0))
        .filter(automation::Column::UnseenFailures.gt(0))
        .exec(conn)
        .await?;
    Ok(())
}

// ── run lifecycle ────────────────────────────────────────────────────────────

fn run_status_str(s: &AutomationRunStatus) -> &'static str {
    match s {
        AutomationRunStatus::Running => "running",
        AutomationRunStatus::Succeeded => "succeeded",
        AutomationRunStatus::Failed => "failed",
        AutomationRunStatus::Cancelled => "cancelled",
        AutomationRunStatus::Skipped => "skipped",
    }
}

/// True if the automation already has a run in flight (overlap guard).
pub async fn has_active_run(
    conn: &DatabaseConnection,
    automation_id: i32,
) -> Result<bool, DbError> {
    let count = automation_run::Entity::find()
        .filter(automation_run::Column::AutomationId.eq(automation_id))
        .filter(automation_run::Column::Status.eq(AutomationRunStatus::Running))
        .count(conn)
        .await?;
    Ok(count > 0)
}

/// Insert a fresh `running` run row at launch.
pub async fn start_run(
    conn: &DatabaseConnection,
    automation_id: i32,
    trigger: &str,
    scheduled_for: Option<DateTime<Utc>>,
) -> Result<AutomationRunInfo, DbError> {
    let now = Utc::now();
    let active = automation_run::ActiveModel {
        id: NotSet,
        automation_id: Set(automation_id),
        status: Set(AutomationRunStatus::Running),
        trigger: Set(trigger.to_string()),
        scheduled_for: Set(scheduled_for),
        started_at: Set(Some(now)),
        ended_at: Set(None),
        conversation_id: Set(None),
        connection_id: Set(None),
        worktree_folder_id: Set(None),
        stop_reason: Set(None),
        error: Set(None),
        summary: Set(None),
        created_at: Set(now),
    };
    let run = active.insert(conn).await?;
    // Reflect the in-flight state on the parent so the list view shows "running"
    // (settle_run overwrites this with the terminal outcome).
    if let Some(auto) = automation::Entity::find_by_id(automation_id)
        .one(conn)
        .await?
    {
        let mut am = auto.into_active_model();
        am.last_run_status = Set(Some("running".to_string()));
        am.last_run_at = Set(Some(now));
        let _ = am.update(conn).await;
    }
    Ok(run_to_info(run))
}

/// Record a fire suppressed because a prior run was still active (overlap skip).
pub async fn record_skipped_run(
    conn: &DatabaseConnection,
    automation_id: i32,
    trigger: &str,
    scheduled_for: Option<DateTime<Utc>>,
) -> Result<AutomationRunInfo, DbError> {
    let now = Utc::now();
    let active = automation_run::ActiveModel {
        id: NotSet,
        automation_id: Set(automation_id),
        status: Set(AutomationRunStatus::Skipped),
        trigger: Set(trigger.to_string()),
        scheduled_for: Set(scheduled_for),
        started_at: Set(None),
        ended_at: Set(Some(now)),
        conversation_id: Set(None),
        connection_id: Set(None),
        worktree_folder_id: Set(None),
        stop_reason: Set(None),
        error: Set(Some("previous run still active".to_string())),
        summary: Set(None),
        created_at: Set(now),
    };
    Ok(run_to_info(active.insert(conn).await?))
}

/// Bind the produced conversation + live connection + worktree to a run after
/// launch. Only sets the provided fields (None leaves the column unchanged).
pub async fn attach_run_runtime<C: ConnectionTrait>(
    conn: &C,
    run_id: i32,
    conversation_id: Option<i32>,
    connection_id: Option<String>,
    worktree_folder_id: Option<i32>,
) -> Result<(), DbError> {
    let row = automation_run::Entity::find_by_id(run_id)
        .one(conn)
        .await?
        .ok_or_else(|| DbError::NotFound(format!("automation_run {run_id}")))?;
    let mut active = row.into_active_model();
    if conversation_id.is_some() {
        active.conversation_id = Set(conversation_id);
    }
    if connection_id.is_some() {
        active.connection_id = Set(connection_id);
    }
    if worktree_folder_id.is_some() {
        active.worktree_folder_id = Set(worktree_folder_id);
    }
    active.update(conn).await?;
    Ok(())
}

/// Persist the observed ACP stop reason before run settlement. The reconcile
/// path uses this as durable proof that `PendingReview` came from `end_turn`,
/// rather than from a user-driven conversation status update.
pub async fn record_stop_reason(
    conn: &DatabaseConnection,
    conversation_id: i32,
    stop_reason: &str,
) -> Result<u64, DbError> {
    let result = automation_run::Entity::update_many()
        .col_expr(
            automation_run::Column::StopReason,
            Expr::value(stop_reason.to_string()),
        )
        .filter(automation_run::Column::ConversationId.eq(conversation_id))
        .filter(automation_run::Column::StopReason.is_null())
        .exec(conn)
        .await?;
    Ok(result.rows_affected)
}

/// Settle a run to a terminal state. CAS on `status = running` so an event-driven
/// settle and the reconcile backstop can never double-settle. Denormalizes the
/// outcome onto the parent automation and bumps `unseen_failures` on failure.
/// Returns `true` if this call performed the settle, `false` if already settled.
pub async fn settle_run(
    conn: &DatabaseConnection,
    run_id: i32,
    status: AutomationRunStatus,
    stop_reason: Option<String>,
    error: Option<String>,
    summary: Option<String>,
) -> Result<bool, DbError> {
    use sea_orm::TransactionTrait;
    let txn = conn.begin().await?;
    let now = Utc::now();

    // CAS: flip only a still-running row (idempotent across event + reconcile).
    let flipped = automation_run::Entity::update_many()
        .col_expr(
            automation_run::Column::Status,
            Expr::value(run_status_str(&status)),
        )
        .filter(automation_run::Column::Id.eq(run_id))
        .filter(automation_run::Column::Status.eq(AutomationRunStatus::Running))
        .exec(&txn)
        .await?;
    if flipped.rows_affected != 1 {
        txn.rollback().await?;
        return Ok(false);
    }

    // Fill the remaining run fields.
    let run = automation_run::Entity::find_by_id(run_id)
        .one(&txn)
        .await?
        .ok_or_else(|| DbError::NotFound(format!("automation_run {run_id}")))?;
    let automation_id = run.automation_id;
    let conversation_id = run.conversation_id;
    let mut rm = run.into_active_model();
    rm.ended_at = Set(Some(now));
    rm.stop_reason = Set(stop_reason);
    rm.error = Set(error);
    rm.summary = Set(summary);
    rm.update(&txn).await?;

    // Denormalize onto the parent automation (drives the list view + badge).
    if let Some(auto) = automation::Entity::find_by_id(automation_id)
        .one(&txn)
        .await?
    {
        let prev_unseen = auto.unseen_failures;
        let mut am = auto.into_active_model();
        am.last_run_at = Set(Some(now));
        am.last_run_status = Set(Some(run_status_str(&status).to_string()));
        am.last_run_conversation_id = Set(conversation_id);
        if status == AutomationRunStatus::Failed {
            am.unseen_failures = Set(prev_unseen + 1);
        }
        am.update(&txn).await?;
    }

    txn.commit().await?;
    Ok(true)
}

/// All currently-running runs — hydrates the completion index at boot and drives
/// the reconcile sweep.
pub async fn list_active_runs(
    conn: &DatabaseConnection,
) -> Result<Vec<AutomationRunInfo>, DbError> {
    let rows = automation_run::Entity::find()
        .filter(automation_run::Column::Status.eq(AutomationRunStatus::Running))
        .all(conn)
        .await?;
    Ok(rows.into_iter().map(run_to_info).collect())
}

/// On boot no ACP connections survive, so every still-`running` run is an
/// interruption. Fail them (never fake success, never re-fire — the automation
/// re-fires naturally on its next schedule). Returns how many were reconciled.
pub async fn boot_reconcile_interrupted(conn: &DatabaseConnection) -> Result<u64, DbError> {
    crate::db::retry_sqlite_maintenance("automation.boot_reconcile_interrupted", || {
        boot_reconcile_interrupted_once(conn)
    })
    .await
}

async fn boot_reconcile_interrupted_once(conn: &DatabaseConnection) -> Result<u64, DbError> {
    let active = list_active_runs(conn).await?;
    let mut n = 0;
    for r in active {
        if settle_run(
            conn,
            r.id,
            AutomationRunStatus::Failed,
            None,
            Some("interrupted by restart".to_string()),
            None,
        )
        .await?
        {
            n += 1;
        }
    }
    Ok(n)
}

// ── scheduling ───────────────────────────────────────────────────────────────

/// Ids of enabled, scheduled automations whose next fire is due (`next_run_at <=
/// now`). NULL `next_run_at` (disabled/manual/exhausted) is excluded.
pub async fn list_due(conn: &DatabaseConnection, now: DateTime<Utc>) -> Result<Vec<i32>, DbError> {
    let rows = automation::Entity::find()
        .filter(automation::Column::Enabled.eq(true))
        .filter(automation::Column::DeletedAt.is_null())
        .filter(automation::Column::TriggerKind.eq(TriggerKind::Schedule))
        .filter(automation::Column::NextRunAt.lte(now))
        .all(conn)
        .await?;
    Ok(rows.into_iter().map(|m| m.id).collect())
}

/// Atomically claim a due automation's current fire slot: advance `next_run_at`
/// to the next cron instant after `now` via a CAS on the read value, so exactly
/// one runner fires the slot — even across a desktop + server both pointing at
/// the same DB, and across restarts. Returns the claimed slot instant (to stamp
/// on the run), or `None` if not actually due or the race was lost.
///
/// `next_run_at` is recomputed forward from `now` (never replays every missed
/// minute), so a process that was down across a slot catches up with a single
/// fire, not a storm.
pub async fn claim_due(
    conn: &DatabaseConnection,
    automation_id: i32,
    now: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>, DbError> {
    use sea_orm::TransactionTrait;
    let txn = conn.begin().await?;

    let Some(row) = automation::Entity::find_by_id(automation_id)
        .one(&txn)
        .await?
    else {
        txn.rollback().await?;
        return Ok(None);
    };
    if !row.enabled || row.deleted_at.is_some() || row.trigger_kind != TriggerKind::Schedule {
        txn.rollback().await?;
        return Ok(None);
    }
    let Some(slot) = row.next_run_at else {
        txn.rollback().await?;
        return Ok(None);
    };
    if slot > now {
        txn.rollback().await?;
        return Ok(None);
    }

    let next = compute_next_run(row.cron.as_deref().unwrap_or(""), &row.timezone, now)?;
    let res = automation::Entity::update_many()
        .col_expr(automation::Column::NextRunAt, Expr::value(next))
        .filter(automation::Column::Id.eq(automation_id))
        .filter(automation::Column::NextRunAt.eq(slot))
        .exec(&txn)
        .await?;
    if res.rows_affected != 1 {
        txn.rollback().await?;
        return Ok(None);
    }
    txn.commit().await?;
    Ok(Some(slot))
}

/// Best-effort retention: soft-hide conversations owned by expired terminal
/// runs, then delete those run rows. Session files and worktrees are untouched.
pub async fn prune_old_runs(conn: &DatabaseConnection, keep_days: i64) -> Result<u64, DbError> {
    crate::db::retry_sqlite_maintenance("automation.prune_old_runs", || {
        prune_old_runs_once(conn, keep_days)
    })
    .await
}

async fn prune_old_runs_once(conn: &DatabaseConnection, keep_days: i64) -> Result<u64, DbError> {
    use sea_orm::TransactionTrait;

    let cutoff = Utc::now() - chrono::Duration::days(keep_days);
    // Only prune terminal rows. A still-`running` row must survive regardless of
    // age: deleting it would defeat the one-active-run unique index (letting a
    // duplicate fire) and orphan the live run's worktree/conversation. In normal
    // operation reconcile force-fails a run long before the retention window, so
    // this only guards the pathological "stuck running past retention" case.
    // NOTE: this deletes the run *rows*; the per-run worktree directory + branch
    // (`automation/<id>/run-<id>`) created for `worktree_per_run` are not yet
    // garbage-collected here — tracked as a follow-up (bounded GC of those
    // artifacts keyed on the run's worktree_folder_id + name signature).
    let txn = conn.begin().await?;
    let conversation_ids = automation_run::Entity::find()
        .select_only()
        .column(automation_run::Column::ConversationId)
        .filter(automation_run::Column::CreatedAt.lt(cutoff))
        .filter(automation_run::Column::Status.ne(AutomationRunStatus::Running))
        .filter(automation_run::Column::ConversationId.is_not_null())
        .into_tuple::<Option<i32>>()
        .all(&txn)
        .await?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    if !conversation_ids.is_empty() {
        conversation::Entity::update_many()
            .col_expr(conversation::Column::DeletedAt, Expr::value(Utc::now()))
            .filter(conversation::Column::Id.is_in(conversation_ids))
            .filter(conversation::Column::DeletedAt.is_null())
            .exec(&txn)
            .await?;
    }
    let res = automation_run::Entity::delete_many()
        .filter(automation_run::Column::CreatedAt.lt(cutoff))
        .filter(automation_run::Column::Status.ne(AutomationRunStatus::Running))
        .exec(&txn)
        .await?;
    txn.commit().await?;
    Ok(res.rows_affected)
}
