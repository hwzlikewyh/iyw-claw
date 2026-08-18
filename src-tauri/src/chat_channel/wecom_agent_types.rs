use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct WecomAgentConfig {
    pub corp_id: String,
    pub agent_id: String,
    #[serde(default)]
    pub callback_path: String,
    #[serde(default)]
    pub external_base_url: String,
    #[serde(default)]
    pub setup_state: String,
    #[serde(default)]
    pub callback_verified_at: Option<String>,
    #[serde(default)]
    pub default_user_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WecomAgentSecrets {
    pub version: u8,
    pub app_secret: String,
    pub callback_token: String,
    pub encoding_aes_key: String,
}

impl WecomAgentSecrets {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let secrets: Self = serde_json::from_str(raw)
            .map_err(|_| "企业微信自建应用凭证格式无效，请重新保存".to_string())?;
        if secrets.version != 1 {
            return Err("企业微信自建应用凭证版本不受支持，请重新保存".to_string());
        }
        if secrets.app_secret.trim().is_empty()
            || secrets.callback_token.trim().is_empty()
            || secrets.encoding_aes_key.trim().len() != 43
        {
            return Err("企业微信自建应用凭证不完整，请重新保存".to_string());
        }
        Ok(secrets)
    }
}
