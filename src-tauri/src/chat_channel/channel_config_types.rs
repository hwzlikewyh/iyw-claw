use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct WecomConfig {
    #[serde(default)]
    pub default_chatid: String,
    #[serde(default = "default_wecom_chat_type")]
    pub default_chat_type: u8,
    #[serde(default)]
    pub poll_interval_secs: Option<u64>,
}

fn default_wecom_chat_type() -> u8 {
    1
}

#[derive(Debug, Clone, Deserialize)]
pub struct WecomAiBotConfig {
    pub bot_id: String,
    #[serde(default)]
    pub default_chatid: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LarkConfig {
    pub app_id: String,
    pub chat_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WeixinConfig {
    pub base_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DingtalkConfig {
    pub client_id: String,
}
