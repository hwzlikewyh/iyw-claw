use crate::acp::session_state::LiveSessionSnapshot;
use crate::models::chat_channel::ChatChannelMessageLogInfo;
use crate::models::AgentType;
use serde::{Deserialize, Serialize};

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelRuntimeQuery {
    pub channel_id: i32,
    pub before: Option<ConversationCursor>,
    pub search: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationCursor {
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub id: i32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelConversationQuery {
    pub channel_id: i32,
    pub conversation_id: i32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelConversationInfo {
    pub channel_id: i32,
    pub conversation_id: i32,
    pub folder_id: i32,
    pub title: Option<String>,
    pub agent_type: AgentType,
    pub model: Option<String>,
    pub message_count: i32,
    pub updated_at: String,
    pub phase: String,
    pub started_at: Option<String>,
    pub last_activity_at: Option<String>,
    pub current_tool: Option<String>,
    pub error: Option<String>,
    pub connection_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelConversationPage {
    pub items: Vec<ChannelConversationInfo>,
    pub next_before: Option<ConversationCursor>,
    pub sampled_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelConversationRuntime {
    pub conversation: ChannelConversationInfo,
    pub snapshot: Option<LiveSessionSnapshot>,
    pub deliveries: Vec<ChatChannelMessageLogInfo>,
    pub sampled_at: String,
}
