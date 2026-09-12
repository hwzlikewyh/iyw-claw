use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::acp::error::AcpError;
use crate::models::{AgentType, ContentBlock, TurnRole};

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkTarget {
    pub session_id: String,
    pub message_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkSessionOptions {
    pub connection_id: String,
    pub conversation_id: Option<i32>,
    pub folder_id: Option<i32>,
    pub target: Option<ForkTarget>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkPoint {
    pub version: u8,
    pub message_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_occurrence: Option<usize>,
}

pub fn supports_history(agent: AgentType) -> bool {
    matches!(
        agent,
        AgentType::ClaudeCode | AgentType::Codex | AgentType::OpenCode
    )
}

pub fn validate_runtime(agent: AgentType, version: Option<&str>) -> Result<(), AcpError> {
    let minimum = match agent {
        AgentType::ClaudeCode => semver::Version::new(0, 73, 0),
        AgentType::Codex => semver::Version::new(1, 8, 0),
        AgentType::OpenCode => semver::Version::new(1, 18, 27),
        _ => return Err(AcpError::protocol("该智能体尚不支持历史消息分叉")),
    };
    let supported = version
        .and_then(|version| semver::Version::parse(version).ok())
        .is_some_and(|version| version >= minimum);
    if !supported {
        return Err(AcpError::protocol(format!(
            "历史消息分叉需要智能体版本 {minimum} 或更新版本，请先更新智能体"
        )));
    }
    Ok(())
}

pub async fn resolve(agent: AgentType, target: ForkTarget) -> Result<ForkPoint, AcpError> {
    if !supports_history(agent) {
        return Err(AcpError::protocol("该智能体尚不支持历史消息分叉"));
    }
    tokio::task::spawn_blocking(move || resolve_sync(agent, target))
        .await
        .map_err(|error| AcpError::protocol(format!("分叉消息读取失败：{error}")))?
}

fn resolve_sync(agent: AgentType, target: ForkTarget) -> Result<ForkPoint, AcpError> {
    let detail = crate::parsers::parser_for_agent(agent)
        .get_conversation(&target.session_id)
        .map_err(|error| AcpError::protocol(format!("分叉历史读取失败：{error}")))?;
    let index = detail
        .turns
        .iter()
        .position(|turn| {
            matches!(turn.role, TurnRole::Assistant)
                && turn.fork_message_id.as_deref() == Some(target.message_id.as_str())
        })
        .ok_or_else(|| AcpError::protocol("分叉消息已变化或尚未保存，请刷新会话后重试"))?;
    let mut point = ForkPoint {
        version: 1,
        message_id: target.message_id,
        message_fingerprint: None,
        message_occurrence: None,
    };
    // 星河旧历史没有原生 item ID，适配器支持正文指纹及全历史出现序号定位。
    if agent == AgentType::Codex {
        let text = turn_text(&detail.turns[index].blocks);
        if !text.is_empty() {
            point.message_fingerprint =
                Some(format!("sha256:{:x}", Sha256::digest(text.as_bytes())));
            point.message_occurrence = Some(
                detail.turns[..=index]
                    .iter()
                    .filter(|turn| {
                        matches!(turn.role, TurnRole::Assistant) && turn_text(&turn.blocks) == text
                    })
                    .count(),
            );
        }
    }
    Ok(point)
}

fn turn_text(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}
