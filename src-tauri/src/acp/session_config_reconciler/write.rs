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

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::acp::session_config_reconciler::model::{claude_code_spec, codex_spec};

    #[test]
    fn codex_reconcile_creates_missing_file_and_preserves_user_fields() {
        let dir = tempfile::tempdir().expect("tempdir");
        let profile = dir.path();
        let spec = codex_spec();
        let outcome = reconcile_managed_files(AgentType::Codex, profile, &spec)
            .expect("reconcile should succeed");
        assert_eq!(outcome.changed, true);
        assert_eq!(outcome.error_code, None);
        assert_eq!(outcome.controlled_fields, spec.fields.len());
        assert!(!outcome.fingerprint.is_empty());

        // 幂等：再次对账无变化，但 fingerprint 稳定且不重复写入。
        let second = reconcile_managed_files(AgentType::Codex, profile, &spec)
            .expect("second reconcile should succeed");
        assert_eq!(second.changed, false);
        assert_eq!(second.fingerprint, outcome.fingerprint);

        // 用户自定义字段（如 MCP 服务）必须完整保留。
        let path = profile.join("config.toml");
        let raw = fs::read_to_string(&path).expect("read config.toml");
        assert!(raw.contains("model_provider"));
        assert!(raw.contains("iyw-claw"));
    }

    #[test]
    fn codex_reconcile_preserves_user_mcp_tables() {
        let dir = tempfile::tempdir().expect("tempdir");
        let profile = dir.path();
        let path = profile.join("config.toml");
        fs::write(
            &path,
            r#"
model = "gpt-5"

[mcp_servers.my-custom-server]
command = "npx"
args = ["-y", "my-tool"]
"#,
        )
        .expect("seed config.toml");
        let spec = codex_spec();
        let outcome = reconcile_managed_files(AgentType::Codex, profile, &spec)
            .expect("reconcile should succeed");
        assert_eq!(outcome.error_code, None);
        let raw = fs::read_to_string(&path).expect("read back config.toml");
        assert!(raw.contains("my-custom-server"), "user MCP must survive");
        assert!(raw.contains("npx"));
    }

    #[test]
    fn claude_reconcile_creates_missing_file_and_writes_env_fields() {
        let dir = tempfile::tempdir().expect("tempdir");
        let profile = dir.path();
        let spec = claude_code_spec();
        let outcome = reconcile_managed_files(AgentType::ClaudeCode, profile, &spec)
            .expect("reconcile should succeed");
        assert_eq!(outcome.changed, true);
        assert_eq!(outcome.error_code, None);
        assert_eq!(outcome.controlled_fields, spec.fields.len());

        let path = profile.join("settings.json");
        let raw = fs::read_to_string(&path).expect("read settings.json");
        assert!(raw.contains("ANTHROPIC_BASE_URL"));
        assert!(raw.contains("ANTHROPIC_MODEL"));
    }

    #[test]
    fn claude_reconcile_preserves_user_json_fields() {
        let dir = tempfile::tempdir().expect("tempdir");
        let profile = dir.path();
        let path = profile.join("settings.json");
        fs::write(
            &path,
            r#"{"permissions":{"allow":["Bash"]},"customKey":"keep-me"}"#,
        )
        .expect("seed settings.json");
        let spec = claude_code_spec();
        reconcile_managed_files(AgentType::ClaudeCode, profile, &spec)
            .expect("reconcile should succeed");
        let raw = fs::read_to_string(&path).expect("read back settings.json");
        assert!(raw.contains("customKey"), "user JSON fields must survive");
        assert!(raw.contains("Bash"));
    }

    #[test]
    fn corrupted_json_is_rejected_without_overwrite() {
        let dir = tempfile::tempdir().expect("tempdir");
        let profile = dir.path();
        let path = profile.join("settings.json");
        fs::write(&path, "{ not valid json").expect("seed corrupt file");
        let spec = claude_code_spec();
        let error = reconcile_managed_files(AgentType::ClaudeCode, profile, &spec)
            .expect_err("corrupt file must fail reconcile");
        assert_eq!(error.code(), "session_config_parse_failed");
        let raw = fs::read_to_string(&path).expect("file must survive untouched");
        assert_eq!(raw, "{ not valid json");
    }

    #[test]
    fn unsupported_agent_fails() {
        let dir = tempfile::tempdir().expect("tempdir");
        let spec = codex_spec();
        let error = reconcile_managed_files(AgentType::Gemini, dir.path(), &spec)
            .expect_err("unsupported agent must fail");
        assert_eq!(error.code(), "session_config_failed");
    }
}

