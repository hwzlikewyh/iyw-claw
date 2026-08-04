//! 受控配置模型：每个 provider adapter 声明受控字段、schema/version、
//! 用户字段保留规则与 fingerprint 算法。

use serde::{Deserialize, Serialize};

use crate::models::agent::AgentType;

/// 受控配置 schema 版本；变更受控字段集时必须递增。
pub const SESSION_CONFIG_SCHEMA_VERSION: u32 = 1;

/// 受控字段类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedFieldKind {
    /// 模型网关 base_url / provider。
    Gateway,
    /// 默认模型。
    Model,
    /// 受管 MCP 服务器条目。
    Mcp,
    /// Skill 搜索路径。
    SkillSearchPath,
    /// 多智能体协同开关（delegation）。
    Delegation,
    /// 实时反馈开关（feedback）。
    Feedback,
    /// 受管 PATH。
    ManagedPath,
}

/// 单个受控字段声明。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedFieldSpec {
    /// 受控字段路径（如 `model_provider`、`env.ANTHROPIC_BASE_URL`）。
    pub path: &'static str,
    pub kind: ManagedFieldKind,
    /// 是否为必需字段：缺失时对账必须失败并阻止 spawn。
    pub required: bool,
}

/// provider adapter 声明：支持的 schema/version 与受控字段路径。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderConfigSpec {
    pub agent: AgentType,
    pub schema_version: u32,
    /// 受控字段清单（规范化、排序、fingerprint 依据）。
    pub fields: &'static [ManagedFieldSpec],
}

/// Codex（星河）：`config.toml` 受控字段。
pub fn codex_spec() -> ProviderConfigSpec {
    ProviderConfigSpec {
        agent: AgentType::Codex,
        schema_version: SESSION_CONFIG_SCHEMA_VERSION,
        fields: &[
            ManagedFieldSpec {
                path: "model_provider",
                kind: ManagedFieldKind::Gateway,
                required: true,
            },
            ManagedFieldSpec {
                path: "model",
                kind: ManagedFieldKind::Model,
                required: true,
            },
            ManagedFieldSpec {
                path: "model_providers.iyw-claw.base_url",
                kind: ManagedFieldKind::Gateway,
                required: true,
            },
            ManagedFieldSpec {
                path: "model_providers.iyw-claw.wire_api",
                kind: ManagedFieldKind::Gateway,
                required: true,
            },
            ManagedFieldSpec {
                path: "model_providers.iyw-claw.requires_openai_auth",
                kind: ManagedFieldKind::Gateway,
                required: true,
            },
        ],
    }
}

/// Claude Code（远山）：`settings.json` 受控字段。
pub fn claude_code_spec() -> ProviderConfigSpec {
    ProviderConfigSpec {
        agent: AgentType::ClaudeCode,
        schema_version: SESSION_CONFIG_SCHEMA_VERSION,
        fields: &[
            ManagedFieldSpec {
                path: "env.ANTHROPIC_BASE_URL",
                kind: ManagedFieldKind::Gateway,
                required: true,
            },
            ManagedFieldSpec {
                path: "env.ANTHROPIC_MODEL",
                kind: ManagedFieldKind::Model,
                required: true,
            },
            ManagedFieldSpec {
                path: "env.ANTHROPIC_MAX_RETRIES",
                kind: ManagedFieldKind::Gateway,
                required: true,
            },
            ManagedFieldSpec {
                path: "env.ANTHROPIC_DEFAULT_OPUS_MODEL",
                kind: ManagedFieldKind::Model,
                required: true,
            },
        ],
    }
}

/// 取指定 agent 的受控配置 spec；仅 Codex 与 Claude Code 受管。
pub fn session_config_spec_for(agent: AgentType) -> Result<ProviderConfigSpec, String> {
    match agent {
        AgentType::Codex => Ok(codex_spec()),
        AgentType::ClaudeCode => Ok(claude_code_spec()),
        other => Err(format!("no managed session config spec for {other}")),
    }
}

/// 规范化受控字段值并按 key 排序后计算 SHA-256 fingerprint。
///
/// 仅基于受控字段的稳定值（不含 token / key / 完整用户路径），
/// 同输入必得同 fingerprint，是"配置内容幂等"的校验依据。
pub fn fingerprint_controlled_fields(fields: &[(&str, String)]) -> String {
    use sha2::{Digest, Sha256};

    let mut normalized: Vec<(String, String)> = fields
        .iter()
        .map(|(path, value)| (path.to_string(), value.trim().to_string()))
        .collect();
    normalized.sort();
    let mut hasher = Sha256::new();
    for (path, value) in normalized {
        hasher.update(path.as_bytes());
        hasher.update([0u8]);
        hasher.update(value.as_bytes());
        hasher.update([0u8]);
    }
    format!("{:x}", hasher.finalize())
}

/// 规范化 JSON 配置并提取受控字段值（供回读校验与 fingerprint）。
pub fn extract_json_controlled_fields(
    value: &serde_json::Value,
    spec: &ProviderConfigSpec,
) -> Vec<(&'static str, String)> {
    spec.fields
        .iter()
        .filter_map(|field| {
            let segments: Vec<&str> = field.path.split('.').collect();
            let mut current = value;
            for segment in &segments {
                current = current.get(*segment)?;
            }
            current.as_str().map(|text| (field.path, text.to_string()))
        })
        .collect()
}

/// 规范化 TOML 配置并提取受控字段值（供回读校验与 fingerprint）。
pub fn extract_toml_controlled_fields(
    value: &toml::Value,
    spec: &ProviderConfigSpec,
) -> Vec<(&'static str, String)> {
    spec.fields
        .iter()
        .filter_map(|field| {
            let segments: Vec<&str> = field.path.split('.').collect();
            let mut current = value;
            for segment in &segments {
                current = current.get(*segment)?;
            }
            match current {
                toml::Value::String(text) => Some((field.path, text.clone())),
                toml::Value::Boolean(flag) => Some((field.path, flag.to_string())),
                toml::Value::Integer(number) => Some((field.path, number.to_string())),
                _ => None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_order_independent() {
        let first = fingerprint_controlled_fields(&[
            ("model", "gpt-5".to_string()),
            ("model_provider", "iyw-claw".to_string()),
            ("env.ANTHROPIC_BASE_URL", "https://example.test".to_string()),
        ]);
        let shuffled = fingerprint_controlled_fields(&[
            ("env.ANTHROPIC_BASE_URL", "https://example.test".to_string()),
            ("model_provider", "iyw-claw".to_string()),
            ("model", "gpt-5".to_string()),
        ]);
        assert_eq!(first, shuffled);
    }

    #[test]
    fn fingerprint_normalizes_whitespace() {
        let trimmed = fingerprint_controlled_fields(&[("model", "gpt-5".to_string())]);
        let padded = fingerprint_controlled_fields(&[("model", "  gpt-5  ".to_string())]);
        assert_eq!(trimmed, padded);
    }

    #[test]
    fn fingerprint_differs_when_value_changes() {
        let a = fingerprint_controlled_fields(&[("model", "gpt-5".to_string())]);
        let b = fingerprint_controlled_fields(&[("model", "gpt-5.1".to_string())]);
        assert_ne!(a, b);
    }

    #[test]
    fn codex_spec_declares_managed_gateway_fields() {
        let spec = codex_spec();
        assert_eq!(spec.schema_version, SESSION_CONFIG_SCHEMA_VERSION);
        let paths: Vec<&str> = spec.fields.iter().map(|field| field.path).collect();
        assert!(paths.contains(&"model_provider"));
        assert!(paths.contains(&"model"));
        assert!(paths.contains(&"model_providers.iyw-claw.base_url"));
        assert!(paths.contains(&"model_providers.iyw-claw.wire_api"));
        assert!(paths.contains(&"model_providers.iyw-claw.requires_openai_auth"));
        assert!(spec.fields.iter().all(|field| field.required));
    }

    #[test]
    fn claude_code_spec_declares_managed_env_fields() {
        let spec = claude_code_spec();
        let paths: Vec<&str> = spec.fields.iter().map(|field| field.path).collect();
        assert!(paths.contains(&"env.ANTHROPIC_BASE_URL"));
        assert!(paths.contains(&"env.ANTHROPIC_MODEL"));
        assert!(paths.contains(&"env.ANTHROPIC_MAX_RETRIES"));
        assert!(paths.contains(&"env.ANTHROPIC_DEFAULT_OPUS_MODEL"));
        assert!(spec.fields.iter().all(|field| field.required));
    }

    #[test]
    fn extract_json_fields_reads_nested_env_paths() {
        let spec = claude_code_spec();
        let value = serde_json::json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://gateway.example/v1",
                "ANTHROPIC_MODEL": "gpt-5",
                "ANTHROPIC_MAX_RETRIES": "10",
                "ANTHROPIC_DEFAULT_OPUS_MODEL": "gpt-5"
            }
        });
        let fields = extract_json_controlled_fields(&value, &spec);
        assert_eq!(fields.len(), spec.fields.len());
        assert!(fields.iter().any(|(path, value)| {
            *path == "env.ANTHROPIC_BASE_URL" && value == "https://gateway.example/v1"
        }));
    }

    #[test]
    fn extract_toml_fields_reads_nested_provider_paths() {
        let spec = codex_spec();
        let value: toml::Value = toml::from_str(
            r#"
model_provider = "iyw-claw"
model = "gpt-5"

[model_providers.iyw-claw]
base_url = "https://gateway.example/v1"
wire_api = "responses"
requires_openai_auth = true
"#,
        )
        .expect("valid TOML");
        let fields = extract_toml_controlled_fields(&value, &spec);
        assert_eq!(fields.len(), spec.fields.len());
        assert!(fields.iter().any(|(path, value)| {
            *path == "model_providers.iyw-claw.base_url" && value == "https://gateway.example/v1"
        }));
        assert!(fields.iter().any(|(path, value)| *path
            == "model_providers.iyw-claw.requires_openai_auth"
            && value == "true"));
    }

    #[test]
    fn unsupported_agent_has_no_spec() {
        assert!(session_config_spec_for(AgentType::Gemini).is_err());
    }
}
