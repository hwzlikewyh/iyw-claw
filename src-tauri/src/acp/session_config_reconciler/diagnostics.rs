//! 会话配置 fingerprint / 诊断的非共享状态模块。
//!
//! 每次 reconcile 记录：agent、new/resume、config schema、受控字段数量、
//! changed、fingerprint、耗时和错误 code。禁止记录配置正文、token、key、
//! 完整用户路径。本模块是进程内状态（非共享：不挂在 AppState 上），
//! 供 UI 展示"最近一次新会话对账时间和结果"。

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::agent::AgentType;

use super::{model::ProviderConfigSpec, ReconcileOutcome};

/// 会话类型：新建或恢复。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    New,
    Resume,
}

/// 单次对账诊断记录（脱敏：不含配置正文 / token / key / 完整用户路径）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconcileDiagnostic {
    pub agent: AgentType,
    pub kind: SessionKind,
    /// 配置 schema 版本。
    pub schema_version: u32,
    /// 受控字段数量。
    pub controlled_fields: usize,
    pub changed: bool,
    pub fingerprint: String,
    /// 耗时（毫秒）。
    pub duration_ms: u64,
    /// 错误 code；成功为 `None`。
    pub error_code: Option<String>,
    pub occurred_at: DateTime<Utc>,
}

/// 进程内诊断环形缓冲（保留最近 N 条）。
const DIAGNOSTIC_CAPACITY: usize = 64;

static DIAGNOSTICS: OnceLock<Mutex<VecDeque<ReconcileDiagnostic>>> = OnceLock::new();

fn diagnostics_store() -> &'static Mutex<VecDeque<ReconcileDiagnostic>> {
    DIAGNOSTICS.get_or_init(|| Mutex::new(VecDeque::with_capacity(DIAGNOSTIC_CAPACITY)))
}

/// 记录一次成功的对账。
pub fn record_success(
    agent: AgentType,
    kind: SessionKind,
    spec: &ProviderConfigSpec,
    outcome: &ReconcileOutcome,
) {
    record(ReconcileDiagnostic {
        agent,
        kind,
        schema_version: spec.schema_version,
        controlled_fields: outcome.controlled_fields,
        changed: outcome.changed,
        fingerprint: outcome.fingerprint.clone(),
        duration_ms: outcome.duration_ms,
        error_code: None,
        occurred_at: Utc::now(),
    });
}

/// 记录一次失败的对账（携带稳定错误 code）。
pub fn record_failure(
    agent: AgentType,
    kind: SessionKind,
    spec: &ProviderConfigSpec,
    error_code: &str,
    duration_ms: u64,
) {
    record(ReconcileDiagnostic {
        agent,
        kind,
        schema_version: spec.schema_version,
        controlled_fields: spec.fields.len(),
        changed: false,
        fingerprint: String::new(),
        duration_ms,
        error_code: Some(error_code.to_string()),
        occurred_at: Utc::now(),
    });
}

fn record(diagnostic: ReconcileDiagnostic) {
    let mut store = diagnostics_store()
        .lock()
        .expect("session config diagnostics mutex poisoned");
    if store.len() >= DIAGNOSTIC_CAPACITY {
        store.pop_front();
    }
    store.push_back(diagnostic);
}

/// 读取最近一次指定 agent 的对账诊断。
pub fn last_diagnostic_for(agent: AgentType) -> Option<ReconcileDiagnostic> {
    let store = diagnostics_store()
        .lock()
        .expect("session config diagnostics mutex poisoned");
    store.iter().rev().find(|item| item.agent == agent).cloned()
}

/// 读取全部诊断快照（最近在前）。
pub fn diagnostics_snapshot() -> Vec<ReconcileDiagnostic> {
    let store = diagnostics_store()
        .lock()
        .expect("session config diagnostics mutex poisoned");
    store.iter().rev().cloned().collect()
}

/// 清理诊断（测试或"清除状态"入口）。
pub fn clear() {
    let mut store = diagnostics_store()
        .lock()
        .expect("session config diagnostics mutex poisoned");
    store.clear();
}
