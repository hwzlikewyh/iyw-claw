//! 受控配置文件的幂等写入、回读校验与 fingerprint 生成。
//!
//! 流程：加载现有文件（不存在则从最小模板创建）→ 解析为结构化模型
//! （解析失败保留原文件并返回可操作错误）→ 只改受控字段 → 写同目录
//! 临时文件、sync、原子替换 → 回读解析并逐字段比较 → 生成 fingerprint。
//! 无变化时避免实际重写，但必须完成回读校验并记录本次 fingerprint。

use std::path::Path;

use crate::models::agent::AgentType;

use super::model::{ProviderConfigSpec, extract_json_controlled_fields, extract_toml_controlled_fields};
use super::{ReconcileError, ReconcileOutcome};

/// 执行指定 agent 的受控配置文件对账。
///
/// 仅管理 Codex（`config.toml`）与 Claude Code（`settings.json`）；
/// 其他 agent 返回错误（调用方不应进入 reconciler）。
pub fn reconcile_managed_files(
    agent: AgentType,
    profile_root: &Path,
    spec: &ProviderConfigSpec,
) -> Result<ReconcileOutcome, ReconcileError> {
    let started = std::time::Instant::now();
    match agent {
        AgentType::Codex => reconcile_codex(profile_root, spec, started),
        AgentType::ClaudeCode => reconcile_claude_code(profile_root, spec, started),
        other => Err(ReconcileError::Failed(format!(
            "no managed session config for {other:?}"
        ))),
    }
}

fn reconcile_codex(
    profile_root: &Path,
    spec: &ProviderConfigSpec,
    started: std::time::Instant,
) -> Result<ReconcileOutcome, ReconcileError> {
    let path = profile_root.join("config.toml");
    let raw = super::super::provider_overlay_files::read_optional(&path)
        .map_err(|error| ReconcileError::Failed(error))?;
    let base_url = crate::acp::provider_overlay::model_gateway_base_url_for(AgentType::Codex);
    let next = crate::acp::provider_overlay::patch_codex_toml(&raw, &base_url)
        .map_err(ReconcileError::ParseFailed)?;
    let changed = raw != next;
    if changed {
        super::super::provider_overlay_files::write_if_changed(&path, &raw, &next)
            .map_err(|error| ReconcileError::WriteFailed(error))?;
    }
    verify_toml(&path, spec, changed, started)
}

fn reconcile_claude_code(
    profile_root: &Path,
    spec: &ProviderConfigSpec,
    started: std::time::Instant,
) -> Result<ReconcileOutcome, ReconcileError> {
    let path = profile_root.join("settings.json");
    let raw = super::super::provider_overlay_files::read_optional(&path)
        .map_err(|error| ReconcileError::Failed(error))?;
    let value = if raw.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(&raw).map_err(|error| {
            ReconcileError::ParseFailed(format!("{}: {error}", path.display()))
        })?
    };
    let base_url =
        crate::acp::provider_overlay::model_gateway_base_url_for(AgentType::ClaudeCode);
    let next_value = crate::acp::provider_overlay::patch_json_config(
        AgentType::ClaudeCode,
        value,
        &base_url,
    )
    .map_err(ReconcileError::ParseFailed)?;
    let serialized = serde_json::to_string_pretty(&next_value)
        .map_err(|error| ReconcileError::ParseFailed(error.to_string()))?;
    let next = serialized + "\n";
    let changed = raw != next;
    if changed {
        super::super::provider_overlay_files::write_if_changed(&path, &raw, &next)
            .map_err(|error| ReconcileError::WriteFailed(error))?;
    }
    verify_json(&path, spec, changed, started)
}

fn verify_toml(
    path: &Path,
    spec: &ProviderConfigSpec,
    changed: bool,
    started: std::time::Instant,
) -> Result<ReconcileOutcome, ReconcileError> {
    let raw = std::fs::read_to_string(path).map_err(|error| {
        ReconcileError::VerificationFailed(format!("read back {}: {error}", path.display()))
    })?;
    let value: toml::Value = raw.parse().map_err(|error| {
        ReconcileError::VerificationFailed(format!("re-parse {}: {error}", path.display()))
    })?;
    let fields = extract_toml_controlled_fields(&value, spec);
    let missing = spec.fields.len().saturating_sub(fields.len());
    if missing > 0 {
        return Err(ReconcileError::VerificationFailed(format!(
            "{}: {missing} controlled field(s) missing after reconcile",
            path.display()
        )));
    }
    Ok(ReconcileOutcome {
        fingerprint: super::model::fingerprint_controlled_fields(&fields),
        changed,
        controlled_fields: fields.len(),
        duration_ms: started.elapsed().as_millis() as u64,
        error_code: None,
    })
}

fn verify_json(
    path: &Path,
    spec: &ProviderConfigSpec,
    changed: bool,
    started: std::time::Instant,
) -> Result<ReconcileOutcome, ReconcileError> {
    let raw = std::fs::read_to_string(path).map_err(|error| {
        ReconcileError::VerificationFailed(format!("read back {}: {error}", path.display()))
    })?;
    let value: serde_json::Value = serde_json::from_str(&raw).map_err(|error| {
        ReconcileError::VerificationFailed(format!("re-parse {}: {error}", path.display()))
    })?;
    let fields = extract_json_controlled_fields(&value, spec);
    let missing = spec.fields.len().saturating_sub(fields.len());
    if missing > 0 {
        return Err(ReconcileError::VerificationFailed(format!(
            "{}: {missing} controlled field(s) missing after reconcile",
            path.display()
        )));
    }
    Ok(ReconcileOutcome {
        fingerprint: super::model::fingerprint_controlled_fields(&fields),
        changed,
        controlled_fields: fields.len(),
        duration_ms: started.elapsed().as_millis() as u64,
        error_code: None,
    })
}
