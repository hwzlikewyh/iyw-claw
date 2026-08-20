use serde::Deserialize;

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LarkRegion {
    #[default]
    Feishu,
    Lark,
}

impl LarkRegion {
    pub fn api_base_url(self) -> &'static str {
        match self {
            Self::Feishu => "https://open.feishu.cn",
            Self::Lark => "https://open.larksuite.com",
        }
    }
}

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
    #[serde(default = "default_wecom_chat_type")]
    pub default_chat_type: u8,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LarkConfig {
    pub app_id: String,
    #[serde(default)]
    pub chat_id: String,
    #[serde(default)]
    pub lark_region: LarkRegion,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WeixinConfig {
    pub base_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DingtalkConfig {
    pub client_id: String,
}
