//! Live user-feedback settings persistence + the submit command surface.
//!
//! One knob survives across restarts:
//!   * `feedback.enabled` — feature switch (product default true; new installs
//!     and keyless upgrades migrate to true and record the migration version;
//!     existing explicit values are preserved). When on, `iyw-claw-mcp`
//!     exposes the `check_user_feedback` tool so an agent can pull mid-turn
//!     user notes; the conversation UI shows the "send a note" bar.
//!
//! Backend policy keys (`feedback.kill_switch` / `feedback.org_policy` and the
//! `IYW_CLAW_FEATURE_KILL_SWITCH` env) override the user value; the UI reads
//! the computed `effective` state and never presents a policy override as a
//! user setting.
//!
//! On startup `apply_persisted_feedback_config` reads this key from
//! `app_metadata` and pushes it into the shared [`FeedbackRuntimeConfig`] that
//! MCP injection reads. On UI save, `set_feedback_settings_core` writes the key
//! and immediately re-applies — mirroring the delegation settings flow exactly
//! (`crate::commands::delegation`).
//!
//! Submitting a note is a live ACP operation (it targets a running connection),
//! so `submit_session_feedback` lives here too but delegates straight to
//! `ConnectionManager::submit_feedback`; the manager owns the turn-in-flight
//! gate and the broadcast.

use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

use crate::acp::feedback::{FeedbackConfig, FeedbackRuntimeConfig};
use crate::app_error::AppCommandError;
use crate::db::service::app_metadata_service;
use crate::web::event_bridge::{emit_event, EventEmitter, FEEDBACK_SETTINGS_CHANGED_EVENT};

pub const KEY_FEEDBACK_ENABLED: &str = "feedback.enabled";
/// 一次性默认值迁移版本：新安装与无旧键升级用户都写 true 并记录该版本。
pub const FEEDBACK_MIGRATION_VERSION: &str = "1";
pub const KEY_FEEDBACK_MIGRATION_VERSION: &str = "feedback.migration_version";
/// 后台策略来源键（由管理端/后端写入，UI 只读）。
pub const KEY_FEEDBACK_ORG_POLICY: &str = "feedback.org_policy";
pub const KEY_FEEDBACK_KILL_SWITCH: &str = "feedback.kill_switch";
pub const KILL_SWITCH_ENV: &str = "IYW_CLAW_FEATURE_KILL_SWITCH";

/// 有效开关来源，UI 必须如实展示。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackEffectiveSource {
    KillSwitch,
    OrgPolicy,
    UserPreference,
    Migrated,
    ProductDefault,
}

/// 合并后的有效开关与来源（只读计算字段，保存时忽略）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackEffectiveState {
    pub enabled: bool,
    pub source: FeedbackEffectiveSource,
    pub kill_switch_active: bool,
}

/// 读取后台 kill switch 与组织策略。
async fn backend_feedback_policy(conn: &DatabaseConnection) -> (Option<bool>, Option<bool>) {
    let env_kill = std::env::var(KILL_SWITCH_ENV)
        .ok()
        .map(|raw| raw.split(',').any(|item| item.trim() == "feedback"));
    let db_kill = app_metadata_service::get_value(conn, KEY_FEEDBACK_KILL_SWITCH)
        .await
        .ok()
        .flatten()
        .and_then(|raw| raw.parse::<bool>().ok());
    let kill_switch = env_kill.or(db_kill);
    let org_policy = app_metadata_service::get_value(conn, KEY_FEEDBACK_ORG_POLICY)
        .await
        .ok()
        .flatten()
        .and_then(|raw| raw.parse::<bool>().ok());
    (kill_switch, org_policy)
}

/// 一次性迁移：无持久键时写 true 并记录 migration version；有键则保留。
pub async fn migrate_feedback_defaults(conn: &DatabaseConnection) {
    let has_key = app_metadata_service::get_value(conn, KEY_FEEDBACK_ENABLED)
        .await
        .ok()
        .flatten()
        .is_some();
    if !has_key {
        let _ = app_metadata_service::upsert_value(conn, KEY_FEEDBACK_ENABLED, "true").await;
        let _ = app_metadata_service::upsert_value(
            conn,
            KEY_FEEDBACK_MIGRATION_VERSION,
            FEEDBACK_MIGRATION_VERSION,
        )
        .await;
    }
}

/// 合并后台策略与用户设置，计算有效开关和来源。
async fn effective_feedback_state(
    conn: &DatabaseConnection,
    settings: &FeedbackSettings,
) -> FeedbackEffectiveState {
    let (kill_switch, org_policy) = backend_feedback_policy(conn).await;
    let migrated = app_metadata_service::get_value(conn, KEY_FEEDBACK_MIGRATION_VERSION)
        .await
        .ok()
        .flatten()
        .map(|_| settings.enabled);
    let flag = crate::acp::session_config_reconciler::merge::resolve_feature_flag(
        kill_switch,
        org_policy,
        Some(settings.enabled),
        migrated,
        crate::acp::session_config_reconciler::merge::PRODUCT_DEFAULT_FEEDBACK_ENABLED,
    );
    FeedbackEffectiveState {
        enabled: flag.enabled,
        source: match flag.source {
            crate::acp::session_config_reconciler::merge::EffectiveSource::KillSwitch => {
                FeedbackEffectiveSource::KillSwitch
            }
            crate::acp::session_config_reconciler::merge::EffectiveSource::OrgPolicy => {
                FeedbackEffectiveSource::OrgPolicy
            }
            crate::acp::session_config_reconciler::merge::EffectiveSource::UserPreference => {
                FeedbackEffectiveSource::UserPreference
            }
            crate::acp::session_config_reconciler::merge::EffectiveSource::Migrated => {
                FeedbackEffectiveSource::Migrated
            }
            crate::acp::session_config_reconciler::merge::EffectiveSource::ProductDefault => {
                FeedbackEffectiveSource::ProductDefault
            }
        },
        kill_switch_active: kill_switch == Some(false),
    }
}

/// 产品默认开启；`effective` 为只读计算字段。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FeedbackSettings {
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective: Option<FeedbackEffectiveState>,
}

impl Default for FeedbackSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            effective: None,
        }
    }
}

impl FeedbackSettings {
    /// 有效开关（kill switch / 组织策略优先，其次用户显式值）。
    fn effective_enabled(&self) -> bool {
        self.effective.map(|state| state.enabled).unwrap_or(self.enabled)
    }

    fn into_runtime_config(self) -> FeedbackConfig {
        FeedbackConfig {
            enabled: self.effective_enabled(),
        }
    }
}

/// Read the persisted key from `app_metadata`, falling back to the default for
/// a missing or malformed value. Never errors hard — corrupt persistence is
/// treated as "no preference yet" (matches `load_delegation_settings`).
pub async fn load_feedback_settings(conn: &DatabaseConnection) -> FeedbackSettings {
    migrate_feedback_defaults(conn).await;
    let mut settings = FeedbackSettings::default();
    if let Ok(Some(raw)) = app_metadata_service::get_value(conn, KEY_FEEDBACK_ENABLED).await {
        if let Ok(v) = raw.parse::<bool>() {
            settings.enabled = v;
        }
    }
    settings.effective = Some(effective_feedback_state(conn, &settings).await);
    settings
}

/// Pull settings from the DB and push the resulting `FeedbackConfig` onto the
/// shared runtime handle. Idempotent — safe on startup, after settings save, or
/// after any external write to `app_metadata`.
pub async fn apply_persisted_feedback_config(
    conn: &DatabaseConnection,
    config: &FeedbackRuntimeConfig,
) {
    let settings = load_feedback_settings(conn).await;
    config.set(settings.into_runtime_config()).await;
}

/// Persist + apply + broadcast. Used by both the Tauri command and the HTTP
/// handler so the write + re-apply + notify chain lives in exactly one place.
///
/// The broadcast is load-bearing, not cosmetic: the settings UI runs in a
/// separate window, so a conversation's feedback bar (in another window / WS
/// client) only learns the flag flipped via this backend
/// [`FEEDBACK_SETTINGS_CHANGED_EVENT`] side-channel — a frontend-only signal
/// would never cross the window boundary.
pub async fn set_feedback_settings_core(
    conn: &DatabaseConnection,
    config: &FeedbackRuntimeConfig,
    emitter: &EventEmitter,
    desired: FeedbackSettings,
) -> Result<FeedbackSettings, AppCommandError> {
    // 后台策略优先：kill switch 或组织策略强制关闭时，用户显式开启不能生效
    // （保存仍落盘用户偏好，但运行配置与 UI 展示以有效状态为准）。
    let (kill_switch, org_policy) = backend_feedback_policy(conn).await;
    let policy_blocks = kill_switch == Some(false) || org_policy == Some(false);
    if policy_blocks && desired.enabled {
        // 有效状态按真实来源计算：kill switch 强制关闭展示"管理员关闭"，
        // 组织策略强制关闭展示"由组织管理"。用户偏好仍落盘，运行配置以
        // 有效状态为准。
        let mut forced = desired;
        forced.enabled = false;
        forced.effective = Some(effective_feedback_state(conn, &forced).await);
        app_metadata_service::upsert_value(conn, KEY_FEEDBACK_ENABLED, "false")
            .await
            .map_err(AppCommandError::from)?;
        config.set(forced.clone().into_runtime_config()).await;
        emit_event(emitter, FEEDBACK_SETTINGS_CHANGED_EVENT, &forced);
        return Ok(forced);
    }
    app_metadata_service::upsert_value(conn, KEY_FEEDBACK_ENABLED, &desired.enabled.to_string())
        .await
        .map_err(AppCommandError::from)?;
    let mut saved = desired;
    saved.effective = Some(effective_feedback_state(conn, &saved).await);
    config.set(saved.clone().into_runtime_config()).await;
    emit_event(emitter, FEEDBACK_SETTINGS_CHANGED_EVENT, &saved);
    Ok(saved)
}

// -------- Tauri commands -----------------------------------------------------

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_feedback_settings(
    #[cfg(feature = "tauri-runtime")] db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<FeedbackSettings, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        Ok(load_feedback_settings(&db.conn).await)
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn set_feedback_settings(
    #[cfg(feature = "tauri-runtime")] app: tauri::AppHandle,
    #[cfg(feature = "tauri-runtime")] db: tauri::State<'_, crate::db::AppDatabase>,
    #[cfg(feature = "tauri-runtime")] config: tauri::State<'_, FeedbackRuntimeConfig>,
    settings: FeedbackSettings,
) -> Result<FeedbackSettings, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        // Tauri's `app.emit` fans out to every window, so the feedback bar in an
        // open conversation window converges even when this save originates in
        // settings UI mounted elsewhere.
        let emitter = EventEmitter::Tauri(app);
        set_feedback_settings_core(&db.conn, &config, &emitter, settings).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = settings;
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

/// Submit a live-feedback note to a running connection. Tauri-only wrapper; the
/// web handler mirrors this. Returns the stored note so the caller can render it
/// optimistically (it also arrives via the `FeedbackSubmitted` event).
///
/// The gate lives in `ConnectionManager::submit_feedback`, keyed on the
/// connection's actual `check_user_feedback` capability (not the possibly
/// later-toggled global setting). Rejections the frontend recognizes:
/// `FeedbackDisabled` (this session has no feedback tool), `NoActiveTurn` (turn
/// ended → fall back to an ordinary prompt), `InvalidFeedback` (empty/oversized).
#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn submit_session_feedback(
    connection_id: String,
    text: String,
    manager: tauri::State<'_, crate::acp::manager::ConnectionManager>,
) -> Result<crate::acp::feedback::FeedbackItem, crate::acp::error::AcpError> {
    manager.submit_feedback(&connection_id, text).await
}
