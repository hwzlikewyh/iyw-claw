//! Automation execution engine: replays a saved composer snapshot through the
//! existing ACP launch chain, then settles the run from the event bus.
//!
//! Design (see docs/automations-spec.md §6/§9):
//! - Completion is correlated by `connection_id` (the `TurnComplete` event has no
//!   conversation_id), via an in-memory index that retains the launched
//!   automation snapshot. `stop_reason` is the settle authority.
//! - A per-tick reconcile backstop settles runs whose `TurnComplete` was dropped
//!   (broadcast lag) by reading the produced conversation's terminal status, and
//!   fails runs this process is not tracking that exceeded a generous deadline.
//! - The idle sweep is NOT a hazard: an in-flight turn sits in `Prompting`, which
//!   `sweep_idle` already skips (it only reaps `Connected`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, Weak};
use std::time::Duration;

use chrono::Utc;
use sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, Set};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::Mutex;
use tokio::time::MissedTickBehavior;

use crate::acp::manager::ConnectionManager;
use crate::acp::types::{AcpEvent, EventEnvelope, PromptInputBlock};
use crate::acp::InternalEventBus;
use crate::automation::default_folder::ensure_default_folder;
use crate::commands::acp::{build_session_runtime_env, verify_agent_installed};
use crate::commands::conversations::{
    create_automation_conversation_core, emit_conversation_upsert,
};
use crate::commands::folders::{
    emit_folder_upsert, get_folder_core, git_checkout, git_is_clean, git_list_branches,
    git_worktree_add, open_worktree_folder_core, resolve_worktree_folder_core,
};
use crate::db::entities::conversation::{self, ConversationStatus};
use crate::db::service::automation_service;
use crate::db::AppDatabase;
use crate::models::{
    AgentType, AutomationConfig, AutomationInfo, AutomationRunStatus, ContentBlock, IsolationMode,
    TurnRole,
};
use crate::web::event_bridge::{
    emit_event, AutomationChange, EventEmitter, AUTOMATION_CHANGED_EVENT,
};

/// Generous absolute cap before a run we are no longer tracking (lost index, or
/// another process) is force-failed by the reconcile sweep. Not a turn timeout —
/// owned, live runs are never force-failed here.
const MAX_RUN_MINUTES: i64 = 180;

/// Reconcile sweep cadence.
const RECONCILE_INTERVAL_SECS: u64 = 30;

/// Scheduler poll cadence. Cron is minute-granular, so 30s catches each slot.
const SCHEDULER_INTERVAL_SECS: u64 = 30;

/// Run-history prune cadence + retention window.
const PRUNE_INTERVAL_SECS: u64 = 6 * 60 * 60;
const RUN_RETENTION_DAYS: i64 = 30;

static ENGINE: OnceLock<Arc<AutomationEngine>> = OnceLock::new();

/// The process-global engine, set once at boot by [`build_engine`]. Read by the
/// manual "run now" / cancel commands (and, later, the scheduler).
pub fn engine() -> Option<Arc<AutomationEngine>> {
    ENGINE.get().cloned()
}

pub struct AutomationEngine {
    db: AppDatabase,
    manager: ConnectionManager,
    emitter: EventEmitter,
    bus: Arc<InternalEventBus>,
    data_dir: PathBuf,
    /// Live automation runs keyed by connection id. The source snapshot ensures
    /// an edit made during the run is not persisted before that revision succeeds.
    index: Arc<Mutex<HashMap<String, LiveRun>>>,
    /// Per-automation fire lock. Serializes the overlap-check + run-row insert (and
    /// the whole launch) so a manual run-now, a scheduled fire, and a double-click
    /// can't all pass `has_active_run` and start duplicate concurrent runs.
    automation_locks: Arc<Mutex<HashMap<i32, Weak<Mutex<()>>>>>,
    /// Serializes git checkout for `shared_in_root` runs on the same root folder.
    root_locks: Arc<Mutex<HashMap<i32, Weak<Mutex<()>>>>>,
    /// Held for the engine's lifetime: an exclusive advisory lock on the DB's
    /// sidecar lock file. The engine is only ever built while holding this lock
    /// (see [`build_engine`]), so its mere existence proves this process is the
    /// sole automation engine on the DB — which is exactly the precondition that
    /// makes the destructive boot reconcile safe. Kept open purely for its Drop:
    /// the OS releases the lock on exit/crash, so the next boot reconciles
    /// correctly.
    _engine_lock: std::fs::File,
}

struct ResolvedCwd {
    root_folder_id: i32,
    folder_id: i32,
    working_dir: String,
    worktree_folder_id: Option<i32>,
}

#[derive(Clone)]
struct LiveRun {
    run_id: i32,
    automation: AutomationInfo,
}

/// Build the engine and publish it to the process global, then return the handle
/// the caller spawns via [`run_automation_engine`].
///
/// Fails closed: returns `None` unless this process can take the data dir's
/// exclusive engine lock. So the engine runs *only* while provably the sole
/// engine on the DB (`engine()` stays unset otherwise, and manual run/cancel
/// return a clean "engine not running" error). `None` happens when another live
/// iyw-claw process already holds the lock (e.g. a desktop app and a server pointed
/// at the same `IYW_CLAW_DATA_DIR`), or — rarely — when the lock can't be
/// established at all (a real IO error on the lock file, e.g. a filesystem
/// without lock support): we never start a lockless engine, since its other
/// guards (`automation_locks`, `root_locks`) are process-local, not cross-process.
pub fn build_engine(
    db: AppDatabase,
    manager: ConnectionManager,
    emitter: EventEmitter,
    bus: Arc<InternalEventBus>,
    data_dir: PathBuf,
) -> Option<Arc<AutomationEngine>> {
    let engine_lock = match acquire_engine_ownership(&data_dir) {
        Ownership::Exclusive(file) => file,
        Ownership::Taken => {
            tracing::info!(
                "[automation] another iyw-claw process owns the automation engine for {}; \
                 this process will not drive automations",
                data_dir.display()
            );
            return None;
        }
        Ownership::Unavailable => {
            tracing::warn!(
                "[automation] could not establish the automation engine lock for {}; \
                 automations are disabled in this process",
                data_dir.display()
            );
            return None;
        }
    };
    let engine = Arc::new(AutomationEngine {
        db,
        manager,
        emitter,
        bus,
        data_dir,
        index: Arc::new(Mutex::new(HashMap::new())),
        automation_locks: Arc::new(Mutex::new(HashMap::new())),
        root_locks: Arc::new(Mutex::new(HashMap::new())),
        _engine_lock: engine_lock,
    });
    let _ = ENGINE.set(engine.clone());
    Some(engine)
}

/// Outcome of trying to become the sole automation engine for a data dir.
enum Ownership {
    /// Exclusive advisory lock held (file kept open for the process lifetime).
    /// This process is provably the sole engine, so the destructive boot
    /// reconcile is safe.
    Exclusive(std::fs::File),
    /// Another live process holds the lock; this process must not run an engine.
    Taken,
    /// The lock couldn't be established at all — a real IO error on the lock file
    /// (e.g. a filesystem without lock support), not contention. Rare, and we fail
    /// closed: without a proven lock we never start the engine.
    Unavailable,
}

/// Path of the per-DB engine lock: the DB filename plus a `.lock` suffix, so it
/// contends exactly when the `automation_run` table is shared — a debug desktop's
/// isolated `iyw-claw-dev.db` never blocks a release `iyw-claw.db`, and vice versa.
fn engine_lock_path(data_dir: &Path) -> PathBuf {
    data_dir.join(format!("{}.lock", crate::db::database_file_name()))
}

/// Take an exclusive, non-blocking advisory lock on the engine lock file, held
/// for the process lifetime. Uses the std cross-platform file lock (`flock` on
/// Unix, `LockFileEx` on Windows), so the single-engine invariant is enforced on
/// every platform. The aggressive boot reconcile (every `running` row is treated
/// as interrupted) is only sound when this process is the sole engine on the DB;
/// the held lock is the proof of that. The OS releases it on exit/crash, so the
/// next boot reconciles correctly.
fn acquire_engine_ownership(data_dir: &Path) -> Ownership {
    let path = engine_lock_path(data_dir);
    let file = match std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        // The file is a pure lock handle — we never write its contents, so
        // leaving any existing bytes is fine (and avoids a needless truncate).
        .truncate(false)
        .open(&path)
    {
        Ok(f) => f,
        Err(e) => {
            // Nearly unreachable: the SQLite DB lives in this same dir, so it is
            // writable. If it really can't open, fail closed rather than run a
            // lockless engine.
            tracing::warn!("[automation] engine lock open failed: {e}");
            return Ownership::Unavailable;
        }
    };
    match file.try_lock() {
        Ok(()) => Ownership::Exclusive(file),
        Err(std::fs::TryLockError::WouldBlock) => Ownership::Taken,
        Err(std::fs::TryLockError::Error(e)) => {
            // A real IO error (never `WouldBlock`) — e.g. a filesystem without
            // lock support. Fail closed: we won't run an engine we can't prove is
            // the only one.
            tracing::warn!("[automation] engine lock failed: {e}");
            Ownership::Unavailable
        }
    }
}

/// Long-running engine driver: boot recovery, then a single select loop over the
/// completion event stream + the reconcile interval. Spawn once per process in
/// each boot path (`lib.rs` setup via `tauri::async_runtime::spawn`, and
/// `bin/iyw_claw_server.rs` via `tokio::spawn`).
pub async fn run_automation_engine(engine: Arc<AutomationEngine>) {
    // Boot recovery: a fresh process has no live connections, so any run still
    // `running` in the DB is an interruption — fail it (never re-fire here). This
    // force-fails EVERY `running` row, which is only correct when this process is
    // the sole engine on the DB. That holds here: the engine is built only while
    // holding the exclusive data-dir lock (see `build_engine`), so a process
    // sharing the data dir never reaches this point against another's live runs.
    match automation_service::boot_reconcile_interrupted(&engine.db.conn).await {
        Ok(n) if n > 0 => {
            tracing::info!("[automation] boot reconcile failed {n} interrupted run(s)")
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("[automation] boot reconcile error: {e}"),
    }

    let mut rx = engine.bus.subscribe();
    let mut reconcile = delay_interval(RECONCILE_INTERVAL_SECS);
    let mut schedule = delay_interval(SCHEDULER_INTERVAL_SECS);
    let mut prune = delay_interval(PRUNE_INTERVAL_SECS);

    loop {
        tokio::select! {
            ev = rx.recv() => match ev {
                Ok(env) => engine.on_event(&env).await,
                // Dropped events under lag — the reconcile backstop recovers them.
                Err(RecvError::Lagged(n)) => {
                    tracing::warn!("[automation] event bus lagged {n}; reconcile will recover");
                }
                Err(RecvError::Closed) => break,
            },
            _ = reconcile.tick() => engine.reconcile_once().await,
            _ = schedule.tick() => {
                // Due-detection + CAS claim; each won slot fires off-thread so a
                // slow git/worktree launch never blocks the event/reconcile arms.
                let due = automation_service::list_due(&engine.db.conn, Utc::now())
                    .await
                    .unwrap_or_default();
                for id in due {
                    match automation_service::claim_due(&engine.db.conn, id, Utc::now()).await {
                        Ok(Some(slot)) => {
                            let eng = engine.clone();
                            tokio::spawn(async move {
                                if let Err(e) = eng.run_automation(id, "schedule", Some(slot)).await {
                                    tracing::info!("[automation] scheduled run {id}: {e}");
                                }
                            });
                        }
                        Ok(None) => {}
                        Err(e) => tracing::warn!("[automation] claim {id}: {e}"),
                    }
                }
            }
            _ = prune.tick() => {
                if let Err(e) =
                    automation_service::prune_old_runs(&engine.db.conn, RUN_RETENTION_DAYS).await
                {
                    tracing::warn!("[automation] prune error: {e}");
                }
            }
        }
    }
}

fn delay_interval(secs: u64) -> tokio::time::Interval {
    let mut i = tokio::time::interval(Duration::from_secs(secs));
    i.set_missed_tick_behavior(MissedTickBehavior::Delay);
    i
}

impl AutomationEngine {
    // ── fire ────────────────────────────────────────────────────────────────

    /// Fire one run of `automation_id`. Records the run row, launches the agent,
    /// and returns the new run id. Does NOT wait for completion (the event
    /// subscriber settles it). On any pre-completion failure the run is settled
    /// `failed` with a visible error (never a silent hang).
    pub async fn run_automation(
        &self,
        automation_id: i32,
        trigger: &str,
        scheduled_for: Option<chrono::DateTime<Utc>>,
    ) -> Result<i32, String> {
        // Serialize every fire of this automation: the overlap check + run-row
        // insert below is otherwise a check-then-act race that a manual run-now
        // racing a scheduled fire (or a double-click) defeats, starting two runs.
        let fire_lock = self.fire_lock(automation_id).await;
        let _fire_guard = fire_lock.lock().await;

        let auto = automation_service::get(&self.db.conn, automation_id)
            .await
            .map_err(|e| e.to_string())?;

        // Overlap guard: never run two of the same automation concurrently.
        if automation_service::has_active_run(&self.db.conn, automation_id)
            .await
            .map_err(|e| e.to_string())?
        {
            let _ = automation_service::record_skipped_run(
                &self.db.conn,
                automation_id,
                trigger,
                scheduled_for,
            )
            .await;
            self.emit(AutomationChange::Upsert { id: automation_id });
            return Err("previous run still active".to_string());
        }

        let run =
            automation_service::start_run(&self.db.conn, automation_id, trigger, scheduled_for)
                .await
                .map_err(|e| e.to_string())?;
        // Broadcast the running row immediately so every client sees it the
        // instant it exists — `launch` can take seconds (worktree add + agent
        // spawn) before it re-emits RunStarted with the live "View conversation"
        // link attached. The frontend refetches the whole run list on each event,
        // so the double emit is idempotent. A launch that fails before the
        // re-emit still surfaces via the RunSettled(failed) emit in the `Err` arm.
        self.emit(AutomationChange::RunStarted {
            automation_id,
            run_id: run.id,
        });

        match self.launch(&auto, run.id).await {
            Ok(()) => Ok(run.id),
            Err(e) => {
                let _ = automation_service::settle_run(
                    &self.db.conn,
                    run.id,
                    AutomationRunStatus::Failed,
                    None,
                    Some(e.clone()),
                    None,
                )
                .await;
                self.emit(AutomationChange::RunSettled {
                    automation_id,
                    run_id: run.id,
                    status: "failed".to_string(),
                });
                Err(e)
            }
        }
    }

    /// Replay the captured composer snapshot through the existing launch chain.
    async fn launch(&self, auto: &AutomationInfo, run_id: i32) -> Result<(), String> {
        let cfg: AutomationConfig =
            serde_json::from_value(auto.config.clone()).map_err(|e| format!("bad config: {e}"))?;
        let agent_type = parse_agent_type(&auto.agent_type)?;
        let prompt_text = automation_prompt_text(&cfg);
        if prompt_text.trim().is_empty() && cfg.prompt_blocks.is_empty() {
            return Err("prompt is empty".to_string());
        }
        let blocks = if cfg.prompt_blocks.is_empty() {
            vec![PromptInputBlock::Text {
                text: prompt_text.clone(),
            }]
        } else {
            cfg.prompt_blocks
                .iter()
                .map(|v| serde_json::from_value::<PromptInputBlock>(v.clone()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("bad prompt blocks: {e}"))?
        };
        if blocks.is_empty() {
            return Err("prompt is empty".to_string());
        }

        let cwd = self.resolve_cwd(auto, run_id).await?;

        // Announce the resolved working folder so every client's sidebar knows it
        // BEFORE the conversation upsert below lands — a conversation in a fresh
        // per-run worktree has no (client-)known folder to group under and would
        // never render otherwise. Re-broadcasting an already-open root folder
        // (shared_in_root) is an idempotent no-op on the client.
        if let Ok(detail) = get_folder_core(&self.db, cwd.folder_id).await {
            emit_folder_upsert(&self.emitter, detail);
        }

        // Recompute env from current settings (never snapshotted); hard-fail
        // visibly if the agent is disabled or not installed.
        let runtime_env = build_session_runtime_env(&self.db, agent_type, None, &self.data_dir)
            .await
            .map_err(|e| e.to_string())?;
        verify_agent_installed(agent_type, &runtime_env).map_err(|e| e.to_string())?;

        // A user cancel can arrive the instant run_automation's early RunStarted
        // makes the row visible. Re-read before spawning the CLI: if a concurrent
        // cancel_run settled this run while resolve_cwd was adding the worktree,
        // stop here — it already emitted RunSettled — rather than spawn an agent
        // for an already-cancelled run.
        if run_no_longer_running(&self.db.conn, run_id).await {
            // A per-run worktree may already exist (resolve_cwd ran, and its
            // folder was broadcast to the sidebar). Record it on the run so the
            // cancelled run still links its worktree for tracking/cleanup rather
            // than orphaning it, then stop before spawning the agent.
            if cwd.worktree_folder_id.is_some() {
                let _ = automation_service::attach_run_runtime(
                    &self.db.conn,
                    run_id,
                    None,
                    None,
                    cwd.worktree_folder_id,
                )
                .await;
            }
            return Ok(());
        }

        // Fresh connection (session_id=None), owner-labelled "automation".
        let conn_id = self
            .manager
            .spawn_agent(
                agent_type,
                Some(cwd.working_dir.clone()),
                None,
                runtime_env,
                "automation".to_string(),
                self.emitter.clone(),
                cfg.mode_id.clone(),
                cfg.config_values.clone(),
            )
            .await
            .map_err(|e| e.to_string())?;

        // Create the conversation row, then adopt it in send_prompt (Branch A).
        let title = first_chars(&prompt_text, 80);
        let mut launched = auto.clone();
        launched.root_folder_id = Some(cwd.root_folder_id);
        let conversation_id = match create_automation_conversation_core(
            &self.db.conn,
            run_id,
            cwd.folder_id,
            agent_type,
            Some(title),
            conn_id.clone(),
            cwd.worktree_folder_id,
        )
        .await
        {
            Ok(id) => id,
            Err(error) => {
                let _ = self.manager.disconnect(&conn_id).await;
                return Err(format!("failed to create automation conversation: {error}"));
            }
        };

        // Register for completion correlation BEFORE prompting, so a fast
        // TurnComplete can't race ahead of the index entry. The conversation
        // and run binding are already committed atomically at this point.
        self.index.lock().await.insert(
            conn_id.clone(),
            LiveRun {
                run_id,
                automation: launched,
            },
        );

        // Automation conversations are intentionally hidden from the ordinary
        // sidebar. Keep the upsert after the run binding so the shared helper
        // can reliably suppress it for both desktop and web clients.
        emit_conversation_upsert(&self.emitter, &self.db.conn, conversation_id).await;

        // Re-emit now that the run carries its connection + conversation, so the
        // running row's "View conversation" link goes live during the run (the
        // early emit in `run_automation` already showed the row itself).
        self.emit(AutomationChange::RunStarted {
            automation_id: auto.id,
            run_id,
        });

        // Final cancel gate before the prompt — the one step that makes the agent
        // do work. The connection is in the index now, so a cancel landing in the
        // tiny window after this read still tears it down via cancel_run's
        // manager.cancel. If it was cancelled while we wired up, abort like the
        // send-error path below and converge the conversation (cancel_run could
        // not when it ran before this row existed); the run stays cancelled.
        if run_no_longer_running(&self.db.conn, run_id).await {
            self.index.lock().await.remove(&conn_id);
            let _ = self.manager.disconnect(&conn_id).await;
            self.cancel_conversation(conversation_id).await;
            return Ok(());
        }

        match self
            .manager
            .send_prompt_linked_with_message_id(
                &self.db,
                &conn_id,
                blocks,
                Some(cwd.folder_id),
                Some(conversation_id),
                None,
                None,
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => {
                self.index.lock().await.remove(&conn_id);
                let _ = self.manager.disconnect(&conn_id).await;
                // The conversation row was created `InProgress`; with the prompt
                // never sent and the connection gone, no TurnComplete will flip it
                // and reconcile won't revisit a Failed run — so flip it terminal
                // here to avoid stranding a stuck-in-progress conversation.
                self.cancel_conversation(conversation_id).await;
                Err(e.to_string())
            }
        }
    }

    /// Resolve the working directory for a run from `(root_folder_id, isolation,
    /// branch)`, reusing the existing worktree/checkout machinery. A legacy
    /// folderless row receives its dedicated managed folder before launch.
    async fn resolve_cwd(&self, auto: &AutomationInfo, run_id: i32) -> Result<ResolvedCwd, String> {
        let root_folder_id = match auto.root_folder_id {
            Some(folder_id) => folder_id,
            None => {
                let generated = ensure_default_folder(&self.db, &self.data_dir, auto.id)
                    .await
                    .map_err(|error| error.to_string())?;
                let folder_id = automation_service::set_root_folder_if_missing(
                    &self.db.conn,
                    auto.id,
                    generated.id,
                )
                .await
                .map_err(|error| error.to_string())?;
                self.emit(AutomationChange::Upsert { id: auto.id });
                tracing::info!(
                    automation_id = auto.id,
                    folder_id,
                    "[automation] bound folderless automation before launch"
                );
                folder_id
            }
        };
        let root = get_folder_core(&self.db, root_folder_id)
            .await
            .map_err(|e| e.to_string())?;

        match auto.isolation {
            IsolationMode::WorktreePerRun => {
                let wt_path = create_run_worktree(&root.path, auto.id, run_id).await?;

                let wt = open_worktree_folder_core(&self.db, wt_path, root_folder_id)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(ResolvedCwd {
                    root_folder_id,
                    folder_id: wt.id,
                    working_dir: wt.path,
                    worktree_folder_id: Some(wt.id),
                })
            }
            IsolationMode::SharedInRoot => {
                let Some(branch) = auto.branch.clone() else {
                    // No branch pinned: run in the root tree as-is.
                    return Ok(ResolvedCwd {
                        root_folder_id,
                        folder_id: root_folder_id,
                        working_dir: root.path,
                        worktree_folder_id: None,
                    });
                };

                // Serialize checkout per root so concurrent shared runs can't
                // corrupt each other's index during the switch.
                let lock = self.root_lock(root_folder_id).await;
                let _guard = lock.lock().await;

                let resolution =
                    resolve_worktree_folder_core(&self.db, root.path.clone(), branch.clone())
                        .await
                        .map_err(|e| e.to_string())?;
                match resolution.path {
                    Some(path) => Ok(ResolvedCwd {
                        root_folder_id,
                        folder_id: resolution.folder_id.unwrap_or(root_folder_id),
                        working_dir: path,
                        worktree_folder_id: resolution.folder_id,
                    }),
                    None => {
                        // A remote pick stores the stripped leaf name, and a bare
                        // `git checkout <leaf>` silently prefers a same-named local
                        // branch (possibly divergent) over the intended remote.
                        // Refuse that ambiguity loudly rather than run the wrong
                        // branch. With no local match the checkout below DWIMs a
                        // unique remote into a tracking branch (and git raises its
                        // own error when multiple remotes share the name).
                        if auto.is_remote_branch {
                            let locals = git_list_branches(root.path.clone())
                                .await
                                .map_err(|e| e.to_string())?;
                            if locals.iter().any(|b| b == &branch) {
                                return Err(format!(
                                    "automation targets remote branch '{branch}' but a local \
                                     branch of that name exists — use a per-run worktree or \
                                     remove the local branch"
                                ));
                            }
                        }
                        // Switching the user's shared root tree to the target
                        // branch must not drag their uncommitted work along: a
                        // dirty tree would carry those edits onto the target
                        // branch (or make `git checkout` fail). Refuse loudly and
                        // tell them to commit/stash or use a per-run worktree.
                        if !git_is_clean(root.path.clone())
                            .await
                            .map_err(|e| e.to_string())?
                        {
                            return Err(format!(
                                "the shared root working tree has uncommitted changes, so it \
                                 can't be switched to '{branch}' — commit or stash them, or \
                                 use a per-run worktree for this automation"
                            ));
                        }
                        git_checkout(root.path.clone(), branch)
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok(ResolvedCwd {
                            root_folder_id,
                            folder_id: root_folder_id,
                            working_dir: root.path,
                            worktree_folder_id: None,
                        })
                    }
                }
            }
        }
    }

    async fn root_lock(&self, root_folder_id: i32) -> Arc<Mutex<()>> {
        let mut locks = self.root_locks.lock().await;
        locks.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = locks.get(&root_folder_id).and_then(Weak::upgrade) {
            return lock;
        }
        let lock = Arc::new(Mutex::new(()));
        locks.insert(root_folder_id, Arc::downgrade(&lock));
        lock
    }

    async fn fire_lock(&self, automation_id: i32) -> Arc<Mutex<()>> {
        let mut locks = self.automation_locks.lock().await;
        locks.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = locks.get(&automation_id).and_then(Weak::upgrade) {
            return lock;
        }
        let lock = Arc::new(Mutex::new(()));
        locks.insert(automation_id, Arc::downgrade(&lock));
        lock
    }

    // ── completion ────────────────────────────────────────────────────────────

    async fn on_event(&self, env: &EventEnvelope) {
        let AcpEvent::TurnComplete { stop_reason, .. } = &env.payload else {
            return;
        };
        let conn_id = env.connection_id.clone();
        let entry = { self.index.lock().await.get(&conn_id).cloned() };
        let Some(entry) = entry else {
            return; // not an automation run
        };
        let run_id = entry.run_id;
        let automation_id = entry.automation.id;

        let (status, status_str) = classify_stop_reason(stop_reason);
        let summary = self.capture_summary(&conn_id).await;
        if let Ok(Some(run)) = run_by_id(&self.db.conn, run_id).await {
            if let Some(conversation_id) = run.conversation_id {
                if let Err(error) = automation_service::record_stop_reason(
                    &self.db.conn,
                    conversation_id,
                    stop_reason,
                )
                .await
                {
                    tracing::warn!(
                        automation_id,
                        run_id,
                        conversation_id,
                        error = %error,
                        "[automation] failed to persist stop reason"
                    );
                }
            }
        }
        let error = if status == AutomationRunStatus::Failed {
            Some(format!("agent stopped: {stop_reason}"))
        } else {
            None
        };

        let settled = automation_service::settle_run(
            &self.db.conn,
            run_id,
            status.clone(),
            Some(stop_reason.clone()),
            error,
            summary.clone(),
        )
        .await;

        // One prompt, one turn, then disconnect (last_assistant_text is cleared
        // at the next turn start, so an automation connection is never reused).
        self.index.lock().await.remove(&conn_id);
        let _ = self.manager.disconnect(&conn_id).await;

        if let Ok(true) = settled {
            if status == AutomationRunStatus::Succeeded {
                self.persist_project_skill(&entry.automation, run_id, summary.as_deref())
                    .await;
            }
            self.emit(AutomationChange::RunSettled {
                automation_id,
                run_id,
                status: status_str.to_string(),
            });
        }
    }

    /// Best-effort: capture the turn's final assistant text on the TurnComplete
    /// tick (it's process-local and cleared at the next turn start).
    async fn capture_summary(&self, conn_id: &str) -> Option<String> {
        let (state, _) = self.manager.get_state_and_emitter(conn_id).await?;
        let text = state.read().await.last_assistant_text.clone();
        text.filter(|t| !t.trim().is_empty())
    }

    // ── reconcile backstop ────────────────────────────────────────────────────

    async fn reconcile_once(&self) {
        let active = match automation_service::list_active_runs(&self.db.conn).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("[automation] reconcile list error: {e}");
                return;
            }
        };
        if active.is_empty() {
            return;
        }
        // run_id -> connection_id for runs THIS process launched and is tracking.
        let owned: HashMap<i32, String> = {
            self.index
                .lock()
                .await
                .iter()
                .map(|(conn_id, live)| (live.run_id, conn_id.clone()))
                .collect()
        };
        let now = Utc::now();

        for run in active {
            if let Some(conn_id) = owned.get(&run.id) {
                // We own this run; `on_event` settles it authoritatively (with the
                // real stop_reason). Leave it alone while its connection is live —
                // settling here from coarse conversation status would race on_event
                // and discard stop_reason fidelity (and could mislabel a cancel).
                if self.manager.get_state_and_emitter(conn_id).await.is_some() {
                    continue;
                }
                // Connection gone but still Running: the TurnComplete can never
                // arrive. Settle from the conversation's terminal status, else
                // fail. Either way drop the now-dead index entry.
                let handled = self.settle_from_conversation(&run).await;
                self.index.lock().await.remove(conn_id);
                if !handled
                    && automation_service::settle_run(
                        &self.db.conn,
                        run.id,
                        AutomationRunStatus::Failed,
                        None,
                        Some("run lost its worker".to_string()),
                        None,
                    )
                    .await
                    .unwrap_or(false)
                {
                    self.emit_settled(&run, "failed");
                }
                continue;
            }

            // Not owned (lost index, or another process). Recover from the
            // conversation's terminal status (a dropped TurnComplete), else fail
            // once the run blows past a generous absolute deadline.
            if self.settle_from_conversation(&run).await {
                continue;
            }
            if let Some(started) = run.started_at {
                if now.signed_duration_since(started) > chrono::Duration::minutes(MAX_RUN_MINUTES)
                    && automation_service::settle_run(
                        &self.db.conn,
                        run.id,
                        AutomationRunStatus::Failed,
                        None,
                        Some("run exceeded max duration or lost its worker".to_string()),
                        None,
                    )
                    .await
                    .unwrap_or(false)
                {
                    self.emit_settled(&run, "failed");
                }
            }
        }
    }

    /// If the produced conversation reached a terminal status, settle the run
    /// accordingly (CAS) and emit. Returns true if the conversation was terminal
    /// (run handled — even if the CAS was lost to a concurrent settle); false if
    /// still InProgress or there is no produced conversation.
    async fn settle_from_conversation(&self, run: &crate::models::AutomationRunInfo) -> bool {
        let Some(conv_id) = run.conversation_id else {
            return false;
        };
        let Some(status) = self.conversation_status(conv_id).await else {
            return false;
        };
        let (run_status, status_str, error, can_persist_skill) = match status {
            ConversationStatus::PendingReview if run.stop_reason.as_deref() == Some("end_turn") => {
                (AutomationRunStatus::Succeeded, "succeeded", None, true)
            }
            ConversationStatus::PendingReview if run.stop_reason.is_none() => return false,
            ConversationStatus::PendingReview => (
                AutomationRunStatus::Failed,
                "failed",
                Some(
                    "conversation reached pending_review with a non-success stop reason"
                        .to_string(),
                ),
                false,
            ),
            ConversationStatus::Completed => {
                (AutomationRunStatus::Succeeded, "succeeded", None, false)
            }
            ConversationStatus::Cancelled => (
                AutomationRunStatus::Failed,
                "failed",
                Some("agent cancelled or refused".to_string()),
                false,
            ),
            ConversationStatus::InProgress => return false,
        };
        let recovered_summary = if run_status == AutomationRunStatus::Succeeded {
            self.conversation_summary(conv_id)
                .await
                .or_else(|| run.summary.clone())
        } else {
            None
        };
        if automation_service::settle_run(
            &self.db.conn,
            run.id,
            run_status.clone(),
            run.stop_reason.clone(),
            error,
            recovered_summary.clone(),
        )
        .await
        .unwrap_or(false)
        {
            // PendingReview plus the durable stop reason proves `end_turn`.
            // Completed remains user-driven and never updates a skill.
            if can_persist_skill {
                self.persist_recovered_project_skill(run, recovered_summary.as_deref())
                    .await;
            }
            self.emit_settled(run, status_str);
        }
        true
    }

    async fn conversation_status(&self, conv_id: i32) -> Option<ConversationStatus> {
        conversation::Entity::find_by_id(conv_id)
            .one(&self.db.conn)
            .await
            .ok()
            .flatten()
            .map(|m| m.status)
    }

    async fn conversation_summary(&self, conv_id: i32) -> Option<String> {
        let (detail, _) =
            crate::commands::conversations::get_folder_conversation_core(&self.db.conn, conv_id)
                .await
                .ok()?;
        detail
            .turns
            .iter()
            .rev()
            .find(|turn| matches!(turn.role, TurnRole::Assistant))
            .map(|turn| {
                turn.blocks
                    .iter()
                    .filter_map(|block| match block {
                        ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .filter(|text| !text.trim().is_empty())
    }

    /// Persist one automation-owned project skill after a successful run. The
    /// marker prevents repeated successful runs from rewriting the file until
    /// the automation itself has been edited.
    async fn persist_project_skill(
        &self,
        launched: &AutomationInfo,
        run_id: i32,
        result_summary: Option<&str>,
    ) {
        let current = match automation_service::get(&self.db.conn, launched.id).await {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(automation_id = launched.id, run_id, error = %error, "[automation] skill load failed");
                return;
            }
        };
        if !same_skill_source(launched, &current) {
            tracing::info!(
                automation_id = launched.id,
                run_id,
                "[automation] skill write deferred until edited automation succeeds"
            );
            return;
        }
        if let Err(error) =
            crate::automation::project_skill::persist(&self.db, launched, run_id, result_summary)
                .await
        {
            tracing::warn!(automation_id = launched.id, run_id, error = %error, "[automation] skill write skipped");
        }
    }

    async fn persist_recovered_project_skill(
        &self,
        run: &crate::models::AutomationRunInfo,
        result_summary: Option<&str>,
    ) {
        let Ok(automation) = automation_service::get(&self.db.conn, run.automation_id).await else {
            return;
        };
        if automation.updated_at > run.created_at {
            tracing::info!(
                automation_id = run.automation_id,
                run_id = run.id,
                "[automation] recovered skill write skipped after automation edit"
            );
            return;
        }
        self.persist_project_skill(&automation, run.id, result_summary)
            .await;
    }

    // ── cancel ────────────────────────────────────────────────────────────────

    /// Cancel a run: stop the live turn if we own it, then settle `cancelled`.
    /// Settling a run with no live connection clears a wedged row.
    pub async fn cancel_run(&self, run_id: i32) -> Result<(), String> {
        // Settle first (CAS) so a racing reconcile tick can't relabel this user
        // cancel as Failed via the conversation-status path.
        let settled = automation_service::settle_run(
            &self.db.conn,
            run_id,
            AutomationRunStatus::Cancelled,
            Some("cancelled".to_string()),
            None,
            None,
        )
        .await
        .map_err(|e| e.to_string())?;

        // Resolve the automation id to take its fire_lock; bail cleanly if the
        // run vanished (shouldn't happen right after a successful settle).
        let Some(run) = run_by_id(&self.db.conn, run_id).await.ok().flatten() else {
            return Ok(());
        };
        let automation_id = run.automation_id;

        // Announce the cancel immediately so the UI leaves "running" without
        // waiting on the teardown below, which may block on an in-progress launch
        // holding the fire_lock.
        if settled {
            self.emit(AutomationChange::RunSettled {
                automation_id,
                run_id,
                status: "cancelled".to_string(),
            });
        }

        // Serialize the teardown with launch. `run_automation` holds this
        // automation's fire_lock across its entire inline `launch()` (including
        // `send_prompt`), so taking it here forces the connection teardown to run
        // either BEFORE launch prompts (its gate then aborts on the cancelled
        // status set above) or AFTER the turn is truly in flight (so
        // `manager.cancel` aborts a real turn) — never interleaved with the prompt
        // enqueue, which would otherwise let the prompt reach the agent after the
        // cancel. Lock order matches launch (fire_lock then index), so no deadlock.
        let fire_lock = self.fire_lock(automation_id).await;
        let _guard = fire_lock.lock().await;

        // Re-read under the lock: a launch that has since finished may have
        // attached the conversation after the snapshot above.
        let conversation_id = run_by_id(&self.db.conn, run_id)
            .await
            .ok()
            .flatten()
            .and_then(|r| r.conversation_id);

        // Stop the live turn and drop the index entry / connection.
        let conn_id = {
            self.index
                .lock()
                .await
                .iter()
                .find(|(_, live)| live.run_id == run_id)
                .map(|(c, _)| c.clone())
        };
        if let Some(conn_id) = conn_id {
            let _ = self.manager.cancel(&self.db.conn, &conn_id).await;
            self.index.lock().await.remove(&conn_id);
            let _ = self.manager.disconnect(&conn_id).await;
        }

        // Converge the produced conversation. A run with no live worker (lost
        // index / another process / cancelled mid-launch) would otherwise strand
        // its conversation at InProgress in the sidebar.
        if let Some(conv_id) = conversation_id {
            if self.conversation_status(conv_id).await == Some(ConversationStatus::InProgress) {
                self.cancel_conversation(conv_id).await;
            }
        }
        Ok(())
    }

    // ── helpers ───────────────────────────────────────────────────────────────

    fn emit(&self, change: AutomationChange) {
        emit_event(&self.emitter, AUTOMATION_CHANGED_EVENT, change);
    }

    fn emit_settled(&self, run: &crate::models::AutomationRunInfo, status: &str) {
        self.emit(AutomationChange::RunSettled {
            automation_id: run.automation_id,
            run_id: run.id,
            status: status.to_string(),
        });
    }

    /// Flip a produced conversation to a terminal status — used when a launch
    /// fails after the row was created `InProgress`, so it isn't left stranded.
    async fn cancel_conversation(&self, conversation_id: i32) {
        if let Ok(Some(row)) = conversation::Entity::find_by_id(conversation_id)
            .one(&self.db.conn)
            .await
        {
            let mut active = row.into_active_model();
            active.status = Set(ConversationStatus::Cancelled);
            if active.update(&self.db.conn).await.is_ok() {
                // The create-time upsert announced this row as InProgress; converge
                // every sidebar to the terminal status (this direct flip emits no
                // ConversationStatusChanged of its own).
                emit_conversation_upsert(&self.emitter, &self.db.conn, conversation_id).await;
            }
        }
    }
}

async fn run_by_id(
    conn: &sea_orm::DatabaseConnection,
    run_id: i32,
) -> Result<Option<crate::db::entities::automation_run::Model>, sea_orm::DbErr> {
    crate::db::entities::automation_run::Entity::find_by_id(run_id)
        .one(conn)
        .await
}

/// True only when the run was positively read in a non-`Running` state (e.g. a
/// concurrent `cancel_run` settled it). A read error or missing row returns
/// `false` so a transient DB hiccup never aborts a legitimate launch — the
/// reconcile backstop still covers a genuinely lost run.
async fn run_no_longer_running(conn: &sea_orm::DatabaseConnection, run_id: i32) -> bool {
    matches!(
        run_by_id(conn, run_id).await,
        Ok(Some(run)) if run.status != AutomationRunStatus::Running
    )
}

fn parse_agent_type(s: &str) -> Result<AgentType, String> {
    serde_json::from_value(serde_json::Value::String(s.to_string()))
        .map_err(|_| format!("unknown agent type: {s}"))
}

fn same_skill_source(launched: &AutomationInfo, current: &AutomationInfo) -> bool {
    launched.name == current.name
        && launched.agent_type == current.agent_type
        && launched.root_folder_id == current.root_folder_id
        && launched.config == current.config
}

fn automation_prompt_text(config: &AutomationConfig) -> String {
    if !config.display_text.trim().is_empty() {
        return config.display_text.clone();
    }
    config
        .prompt_blocks
        .iter()
        .filter_map(|block| block.get("text").and_then(serde_json::Value::as_str))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `end_turn` → succeeded; explicit cancel → cancelled; everything else
/// (refusal / max_tokens / max_turn_requests / empty / unknown) → failed.
fn classify_stop_reason(stop_reason: &str) -> (AutomationRunStatus, &'static str) {
    match stop_reason {
        "end_turn" => (AutomationRunStatus::Succeeded, "succeeded"),
        "cancelled" => (AutomationRunStatus::Cancelled, "cancelled"),
        _ => (AutomationRunStatus::Failed, "failed"),
    }
}

fn first_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn basename(path: &str) -> &str {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
}

fn sibling_path(root_path: &str, name: &str) -> String {
    let trimmed = root_path.trim_end_matches(['/', '\\']);
    if let Some(index) = trimmed.rfind(['/', '\\']) {
        let separator = &trimmed[index..=index];
        return format!("{}{separator}{name}", &trimmed[..index]);
    }
    let root = crate::git_credential::absolutize(Path::new(root_path));
    root.parent()
        .map(|parent| parent.join(name))
        .unwrap_or_else(|| root.join(name))
        .to_string_lossy()
        .into_owned()
}

fn worktree_error_with_detail(error: &crate::app_error::AppCommandError) -> String {
    match error.detail.as_deref().filter(|detail| !detail.is_empty()) {
        Some(detail) => format!("{}: {detail}", error.message),
        None => error.message.clone(),
    }
}

async fn create_run_worktree(
    root_path: &str,
    automation_id: i32,
    run_id: i32,
) -> Result<String, String> {
    let branch = format!("automation-{automation_id}-run-{run_id}");
    let dir = format!(
        "{}-automation-{automation_id}-run-{run_id}",
        basename(root_path)
    );
    let worktree_path = sibling_path(root_path, &dir);
    let first_error = match git_worktree_add(
        root_path.to_string(),
        branch.clone(),
        worktree_path.clone(),
    )
    .await
    {
        Ok(()) => return Ok(worktree_path),
        Err(error) => error,
    };

    let suffix = short_suffix(run_id);
    let retry_path = sibling_path(root_path, &format!("{dir}-{suffix}"));
    git_worktree_add(
        root_path.to_string(),
        format!("{branch}-{suffix}"),
        retry_path.clone(),
    )
    .await
    .map(|()| retry_path)
    .map_err(|retry_error| {
        format!(
            "worktree add failed; first attempt: {}; retry: {}",
            worktree_error_with_detail(&first_error),
            worktree_error_with_detail(&retry_error)
        )
    })
}

fn short_suffix(run_id: i32) -> String {
    // Deterministic, leftover-avoiding suffix (no RNG needed at this layer).
    format!("r{run_id}b")
}
