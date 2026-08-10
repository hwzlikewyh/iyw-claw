//! Delegation settings persistence + Tauri/HTTP command surface.
//!
//! These knobs survive across restarts:
//!   * `delegation.enabled` — feature switch (product default true; new
//!     installs and keyless upgrades migrate to true and record the migration
//!     version; existing explicit values are preserved)
//!   * `delegation.depth_limit` — max chain depth a child is allowed to sit at
//!   * `delegation.agent_defaults` — per-agent spawn overrides (JSON blob)
//!   * `delegation.completed_cache_max_mb` — per-parent byte budget (in MB) for
//!     the broker's in-memory cache of completed result text (`0` = unlimited)
//!
//! On startup `apply_persisted_config` reads these keys from `app_metadata`
//! and pushes them into the live `DelegationBroker`. On UI save,
//! `set_delegation_settings_core` writes these keys and immediately
//! re-applies — the broker has no concept of "pending config", it just
//! owns the current `DelegationConfig`. The previously-persisted
//! `delegation.default_timeout_seconds` key is ignored on read (the broker
//! no longer applies a timeout; cancellation flows through MCP
//! `notifications/cancelled` instead).

use std::collections::BTreeMap;
use std::path::PathBuf;
#[cfg(any(test, feature = "tauri-runtime"))]
use std::sync::Arc;

use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

use crate::acp::delegation::broker::{DelegationBroker, DelegationConfig};
use crate::acp::delegation::types::AgentDelegationDefaults;
use crate::app_error::AppCommandError;
use crate::db::service::app_metadata_service;
use crate::models::AgentType;

pub const KEY_DELEGATION_ENABLED: &str = "delegation.enabled";
pub const KEY_DELEGATION_DEPTH: &str = "delegation.depth_limit";
/// Single JSON-serialized key for the per-agent delegation overrides.
/// Stored as one blob (rather than one row per agent×option) because the
/// option set is dynamic and per-agent — flat keys can't enumerate it.
pub const KEY_DELEGATION_AGENT_DEFAULTS: &str = "delegation.agent_defaults";
/// Per-parent completed-result cache budget, in MB. `0` = unlimited.
pub const KEY_DELEGATION_COMPLETED_CACHE_MB: &str = "delegation.completed_cache_max_mb";

pub const DEPTH_MIN: u32 = 1;
pub const DEPTH_MAX: u32 = 8;

/// Product default for the completed-result cache budget, in MB. Used by
/// `DelegationSettings::default()` and as the serde fallback when a payload
/// omits the field (absent ≠ unlimited).
pub const DEFAULT_COMPLETED_CACHE_MB: u32 = 512;

fn default_completed_cache_max_mb() -> u32 {
    DEFAULT_COMPLETED_CACHE_MB
}

/// Newtype so the Tauri managed-state lookup can distinguish the delegation
/// UDS path from other `PathBuf`s in the state graph.
#[derive(Clone)]
pub struct DelegationSocketPath(pub PathBuf);

/// 有效开关来源，UI 必须如实展示（默认/用户/组织策略/安全关闭）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DelegationEffectiveSource {
    /// 后台安全 kill switch（管理员紧急关闭）。
    KillSwitch,
    /// 后台强制组织策略。
    OrgPolicy,
    /// 用户显式偏好。
    UserPreference,
    /// 迁移自旧持久键的一次性默认。
    Migrated,
    /// 产品默认值。
    ProductDefault,
}

/// 合并后的有效开关与来源（只读计算字段，保存时忽略）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegationEffectiveState {
    pub enabled: bool,
    pub source: DelegationEffectiveSource,
    /// 后台 kill switch 是否正在强制关闭本功能。
    pub kill_switch_active: bool,
}

/// 一次性默认值迁移版本：新安装与无旧键升级用户都写 true 并记录该版本。
pub const DELEGATION_MIGRATION_VERSION: &str = "1";
pub const KEY_DELEGATION_MIGRATION_VERSION: &str = "delegation.migration_version";

/// 后台策略来源键（由管理端/后端写入，UI 只读）。
pub const KEY_DELEGATION_ORG_POLICY: &str = "delegation.org_policy";
pub const KEY_DELEGATION_KILL_SWITCH: &str = "delegation.kill_switch";
pub const KILL_SWITCH_ENV: &str = "IYW_CLAW_FEATURE_KILL_SWITCH";

/// 读取后台 kill switch 与组织策略（`Some` 即生效，优先于用户设置）。
async fn backend_delegation_policy(conn: &DatabaseConnection) -> (Option<bool>, Option<bool>) {
    let env_kill = std::env::var(KILL_SWITCH_ENV)
        .ok()
        .map(|raw| raw.split(',').any(|item| item.trim() == "delegation"));
    let db_kill = app_metadata_service::get_value(conn, KEY_DELEGATION_KILL_SWITCH)
        .await
        .ok()
        .flatten()
        .and_then(|raw| raw.parse::<bool>().ok());
    // 持久键仅在显式布尔时生效；损坏值视为未设置（不回退直连）。
    let kill_switch = env_kill.or(db_kill);
    let org_policy = app_metadata_service::get_value(conn, KEY_DELEGATION_ORG_POLICY)
        .await
        .ok()
        .flatten()
        .and_then(|raw| raw.parse::<bool>().ok());
    (kill_switch, org_policy)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationSettings {
    pub enabled: bool,
    pub depth_limit: u32,
    /// Per-agent overrides applied by the delegation broker when iyw-claw-mcp
    /// spawns a subagent. Missing modes use the product-owned automatic mode;
    /// missing config values keep the agent's model and effort defaults.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub agent_defaults: BTreeMap<AgentType, AgentDelegationDefaults>,
    /// Per-parent byte budget (in MB) for the broker's in-memory cache of
    /// completed sub-agent result text. `0` = unlimited. Converted to bytes in
    /// `into_broker_config`. Absent in a payload → the product default (not
    /// unlimited), so an older client can't silently disable the valve.
    #[serde(default = "default_completed_cache_max_mb")]
    pub completed_cache_max_mb: u32,
    /// 只读有效状态：由后台策略 + 用户设置 + 迁移合并计算，保存时忽略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective: Option<DelegationEffectiveState>,
}

impl Default for DelegationSettings {
    fn default() -> Self {
        Self {
            // 产品默认：新安装默认开启多智能体协同。
            enabled: true,
            depth_limit: 1,
            agent_defaults: BTreeMap::new(),
            completed_cache_max_mb: DEFAULT_COMPLETED_CACHE_MB,
            effective: None,
        }
    }
}

impl DelegationSettings {
    fn clamped(self) -> Self {
        Self {
            enabled: self.enabled,
            depth_limit: self.depth_limit.clamp(DEPTH_MIN, DEPTH_MAX),
            agent_defaults: self
                .agent_defaults
                .into_iter()
                .filter(|(_, v)| !v.is_empty())
                .collect(),
            // No upper clamp: the cache budget is a user memory choice, not a
            // safety rail. `0` stays `0` (unlimited).
            completed_cache_max_mb: self.completed_cache_max_mb,
            effective: self.effective,
        }
    }

    /// 有效开关（后台 kill switch / 组织策略优先，其次用户显式值）。
    /// 无 `effective` 时回退到 `enabled`（旧客户端路径），但 kill switch
    /// 强制关闭时仍以 kill switch 为准，用户无法绕过。
    fn effective_enabled(&self) -> bool {
        self.effective
            .map(|state| state.enabled)
            .unwrap_or(self.enabled)
    }

    fn into_broker_config(self) -> DelegationConfig {
        DelegationConfig {
            enabled: self.effective_enabled(),
            depth_limit: self.depth_limit,
            agent_defaults: self.agent_defaults,
            // MB → bytes. `saturating_mul` guards a pathologically large MB
            // value from wrapping on 32-bit `usize` targets.
            completed_cache_cap_bytes: (self.completed_cache_max_mb as usize)
                .saturating_mul(1024 * 1024),
        }
    }
}

/// 一次性迁移：新安装与升级用户都要求 delegation 默认开启。
/// 不存在持久键时写 true 并记录 migration version；已存在键时原样保留。
/// 幂等，可安全地在每次读取前调用。
pub async fn migrate_delegation_defaults(conn: &DatabaseConnection) {
    let has_key = app_metadata_service::get_value(conn, KEY_DELEGATION_ENABLED)
        .await
        .ok()
        .flatten()
        .is_some();
    if !has_key {
        let _ = app_metadata_service::upsert_value(conn, KEY_DELEGATION_ENABLED, "true").await;
        let _ = app_metadata_service::upsert_value(
            conn,
            KEY_DELEGATION_MIGRATION_VERSION,
            DELEGATION_MIGRATION_VERSION,
        )
        .await;
    }
}

/// 合并后台策略与用户设置，计算有效开关和来源。
async fn effective_delegation_state(
    conn: &DatabaseConnection,
    settings: &DelegationSettings,
) -> DelegationEffectiveState {
    let (kill_switch, org_policy) = backend_delegation_policy(conn).await;
    let migrated = app_metadata_service::get_value(conn, KEY_DELEGATION_MIGRATION_VERSION)
        .await
        .ok()
        .flatten()
        .map(|_| settings.enabled);
    let flag = crate::acp::session_config_reconciler::merge::resolve_feature_flag(
        kill_switch,
        org_policy,
        Some(settings.enabled),
        migrated,
        crate::acp::session_config_reconciler::merge::PRODUCT_DEFAULT_DELEGATION_ENABLED,
    );
    DelegationEffectiveState {
        enabled: flag.enabled,
        source: match flag.source {
            crate::acp::session_config_reconciler::merge::EffectiveSource::KillSwitch => {
                DelegationEffectiveSource::KillSwitch
            }
            crate::acp::session_config_reconciler::merge::EffectiveSource::OrgPolicy => {
                DelegationEffectiveSource::OrgPolicy
            }
            crate::acp::session_config_reconciler::merge::EffectiveSource::UserPreference => {
                DelegationEffectiveSource::UserPreference
            }
            crate::acp::session_config_reconciler::merge::EffectiveSource::Migrated => {
                DelegationEffectiveSource::Migrated
            }
            crate::acp::session_config_reconciler::merge::EffectiveSource::ProductDefault => {
                DelegationEffectiveSource::ProductDefault
            }
        },
        kill_switch_active: kill_switch == Some(false),
    }
}

/// Read all persisted keys from `app_metadata`, falling back to defaults
/// for any missing or malformed value. Never errors hard — corrupt
/// persistence is treated as "no preference yet."
pub async fn load_delegation_settings(conn: &DatabaseConnection) -> DelegationSettings {
    migrate_delegation_defaults(conn).await;
    let mut settings = DelegationSettings::default();
    if let Ok(Some(raw)) = app_metadata_service::get_value(conn, KEY_DELEGATION_ENABLED).await {
        if let Ok(v) = raw.parse::<bool>() {
            settings.enabled = v;
        }
    }
    if let Ok(Some(raw)) = app_metadata_service::get_value(conn, KEY_DELEGATION_DEPTH).await {
        if let Ok(v) = raw.parse::<u32>() {
            settings.depth_limit = v;
        }
    }
    if let Ok(Some(raw)) =
        app_metadata_service::get_value(conn, KEY_DELEGATION_COMPLETED_CACHE_MB).await
    {
        if let Ok(v) = raw.parse::<u32>() {
            settings.completed_cache_max_mb = v;
        }
    }
    if let Ok(Some(raw)) =
        app_metadata_service::get_value(conn, KEY_DELEGATION_AGENT_DEFAULTS).await
    {
        // Corrupt JSON → keep defaults (empty map). Matches the "never errors
        // hard" contract on the other two keys above.
        if let Ok(parsed) =
            serde_json::from_str::<BTreeMap<AgentType, AgentDelegationDefaults>>(&raw)
        {
            settings.agent_defaults = parsed;
        }
    }
    let mut settings = settings.clamped();
    settings.effective = Some(effective_delegation_state(conn, &settings).await);
    settings
}

/// Pull settings from the DB and push the resulting `DelegationConfig` onto
/// the broker. Idempotent — safe to call on startup, after settings save, or
/// after any external write to `app_metadata`.
pub async fn apply_persisted_config(conn: &DatabaseConnection, broker: &DelegationBroker) {
    let settings = load_delegation_settings(conn).await;
    broker.set_config(settings.into_broker_config()).await;
}

/// Persist + apply. Used by both the Tauri command and the HTTP handler so
/// the clamp / re-apply chain is in exactly one place.
pub async fn set_delegation_settings_core(
    conn: &DatabaseConnection,
    broker: &DelegationBroker,
    desired: DelegationSettings,
) -> Result<DelegationSettings, AppCommandError> {
    // 后台策略优先：kill switch 或组织策略强制关闭时，用户显式开启不能生效
    // （保存仍落盘用户偏好，但运行配置与 UI 展示以有效状态为准）。
    let (kill_switch, org_policy) = backend_delegation_policy(conn).await;
    let policy_blocks = kill_switch == Some(false) || org_policy == Some(false);
    if policy_blocks && desired.enabled {
        // 有效状态按真实来源计算：kill switch 强制关闭展示"管理员关闭"，
        // 组织策略强制关闭展示"由组织管理"。
        let effective = effective_delegation_state(conn, &desired).await;
        let mut forced = desired;
        forced.enabled = false;
        forced.effective = Some(effective);
        broker.set_config(forced.clone().into_broker_config()).await;
        return Ok(forced);
    }
    let clamped = desired.clamped();
    app_metadata_service::upsert_value(conn, KEY_DELEGATION_ENABLED, &clamped.enabled.to_string())
        .await
        .map_err(AppCommandError::from)?;
    app_metadata_service::upsert_value(
        conn,
        KEY_DELEGATION_DEPTH,
        &clamped.depth_limit.to_string(),
    )
    .await
    .map_err(AppCommandError::from)?;
    app_metadata_service::upsert_value(
        conn,
        KEY_DELEGATION_COMPLETED_CACHE_MB,
        &clamped.completed_cache_max_mb.to_string(),
    )
    .await
    .map_err(AppCommandError::from)?;
    // Whole-blob replace semantics: save mirrors what the UI sent. Empty map
    // serializes to "{}" — still write it so a user can clear all overrides
    // back to the agent defaults.
    let agent_defaults_json = serde_json::to_string(&clamped.agent_defaults).map_err(|e| {
        AppCommandError::configuration_invalid(format!("serialize agent_defaults: {e}"))
    })?;
    app_metadata_service::upsert_value(conn, KEY_DELEGATION_AGENT_DEFAULTS, &agent_defaults_json)
        .await
        .map_err(AppCommandError::from)?;
    broker
        .set_config(clamped.clone().into_broker_config())
        .await;
    let mut saved = clamped;
    saved.effective = Some(effective_delegation_state(conn, &saved).await);
    Ok(saved)
}

// -------- Tauri commands -----------------------------------------------------

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_delegation_settings(
    #[cfg(feature = "tauri-runtime")] db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<DelegationSettings, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        Ok(load_delegation_settings(&db.conn).await)
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        // Server mode reaches this via the web handler, not this command.
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn set_delegation_settings(
    #[cfg(feature = "tauri-runtime")] db: tauri::State<'_, crate::db::AppDatabase>,
    #[cfg(feature = "tauri-runtime")] broker: tauri::State<'_, Arc<DelegationBroker>>,
    settings: DelegationSettings,
) -> Result<DelegationSettings, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        set_delegation_settings_core(&db.conn, broker.inner(), settings).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = settings;
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}
