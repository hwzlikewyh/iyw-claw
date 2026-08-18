use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Cow;
use std::fmt;

pub const CUSTOM_AGENT_WIRE_PREFIX: &str = "custom:";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AgentType {
    ClaudeCode,
    Codex,
    OpenCode,
    Gemini,
    OpenClaw,
    Cline,
    Hermes,
    CodeBuddy,
    KimiCode,
    Pi,
    Grok,
    Cursor,
    DeepSeek,
    Custom(&'static str),
}

pub const BUILTIN_AGENT_TYPES: &[AgentType] = &[
    AgentType::ClaudeCode,
    AgentType::Codex,
    AgentType::OpenCode,
    AgentType::Gemini,
    AgentType::OpenClaw,
    AgentType::Cline,
    AgentType::Hermes,
    AgentType::CodeBuddy,
    AgentType::KimiCode,
    AgentType::Pi,
    AgentType::Grok,
    AgentType::Cursor,
    AgentType::DeepSeek,
];

impl AgentType {
    pub fn custom(registry_id: &str) -> Option<Self> {
        if !is_valid_custom_agent_id(registry_id) {
            return None;
        }
        super::agent_interner::intern(registry_id).map(Self::Custom)
    }

    pub fn custom_id(self) -> Option<&'static str> {
        match self {
            Self::Custom(registry_id) => Some(registry_id),
            _ => None,
        }
    }

    pub fn is_custom(self) -> bool {
        self.custom_id().is_some()
    }

    pub const fn is_legacy_builtin(self) -> bool {
        matches!(
            self,
            Self::ClaudeCode
                | Self::Codex
                | Self::OpenCode
                | Self::Gemini
                | Self::OpenClaw
                | Self::Cline
                | Self::Hermes
                | Self::CodeBuddy
                | Self::KimiCode
                | Self::Pi
                | Self::Grok
        )
    }

    pub fn as_wire(self) -> Cow<'static, str> {
        match self {
            Self::ClaudeCode => Cow::Borrowed("claude_code"),
            Self::Codex => Cow::Borrowed("codex"),
            Self::OpenCode => Cow::Borrowed("open_code"),
            Self::Gemini => Cow::Borrowed("gemini"),
            Self::OpenClaw => Cow::Borrowed("open_claw"),
            Self::Cline => Cow::Borrowed("cline"),
            Self::Hermes => Cow::Borrowed("hermes"),
            Self::CodeBuddy => Cow::Borrowed("code_buddy"),
            Self::KimiCode => Cow::Borrowed("kimi_code"),
            Self::Pi => Cow::Borrowed("pi"),
            Self::Grok => Cow::Borrowed("grok"),
            Self::Cursor => Cow::Borrowed("cursor"),
            Self::DeepSeek => Cow::Borrowed("deepseek"),
            Self::Custom(registry_id) => {
                Cow::Owned(format!("{CUSTOM_AGENT_WIRE_PREFIX}{registry_id}"))
            }
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "claude_code" => Some(Self::ClaudeCode),
            "codex" => Some(Self::Codex),
            "open_code" => Some(Self::OpenCode),
            "gemini" => Some(Self::Gemini),
            "open_claw" => Some(Self::OpenClaw),
            "cline" => Some(Self::Cline),
            "hermes" => Some(Self::Hermes),
            "code_buddy" => Some(Self::CodeBuddy),
            "kimi_code" => Some(Self::KimiCode),
            "pi" => Some(Self::Pi),
            "grok" => Some(Self::Grok),
            "cursor" => Some(Self::Cursor),
            "deepseek" => Some(Self::DeepSeek),
            other => other
                .strip_prefix(CUSTOM_AGENT_WIRE_PREFIX)
                .and_then(Self::custom),
        }
    }
}

pub fn is_valid_custom_agent_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && !value.starts_with('.')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && !is_builtin_identity(value)
}

fn is_builtin_identity(value: &str) -> bool {
    matches!(
        value,
        "claude_code"
            | "claude-acp"
            | "codex"
            | "codex-acp"
            | "open_code"
            | "opencode"
            | "gemini"
            | "open_claw"
            | "openclaw-acp"
            | "cline"
            | "hermes"
            | "code_buddy"
            | "codebuddy-code"
            | "kimi_code"
            | "kimi-code"
            | "pi"
            | "pi-acp"
            | "grok"
            | "grok-build"
            | "cursor"
            | "deepseek"
            | "deepseek-acp"
    )
}

impl Serialize for AgentType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.as_wire())
    }
}

impl<'de> Deserialize<'de> for AgentType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Cow::<'de, str>::deserialize(deserializer)?;
        Self::from_wire(&raw).ok_or_else(|| D::Error::custom(format!("unknown agent type: {raw}")))
    }
}

impl fmt::Display for AgentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentType::ClaudeCode => write!(f, "Claude Code"),
            AgentType::Codex => write!(f, "Codex CLI"),
            AgentType::OpenCode => write!(f, "OpenCode"),
            AgentType::Gemini => write!(f, "Gemini CLI"),
            AgentType::OpenClaw => write!(f, "OpenClaw"),
            AgentType::Cline => write!(f, "Cline"),
            AgentType::Hermes => write!(f, "Hermes Agent"),
            AgentType::CodeBuddy => write!(f, "CodeBuddy"),
            AgentType::KimiCode => write!(f, "Kimi Code"),
            AgentType::Pi => write!(f, "Pi"),
            AgentType::Grok => write!(f, "知微"),
            AgentType::Cursor => write!(f, "Cursor"),
            AgentType::DeepSeek => write!(f, "DeepSeek Harness"),
            AgentType::Custom(registry_id) => write!(f, "{registry_id}"),
        }
    }
}
