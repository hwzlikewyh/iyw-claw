use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(super) use super::spool::{cleanup, file};
use super::{Content, Runtime};
use crate::chat_channel::{
    error::ChatChannelError,
    types::{ChannelMessageTarget, MessageLevel, RichMessage},
};
use crate::db::service::{
    chat_channel_service, chat_channel_target_service, sender_context_service,
};

#[derive(Serialize, Deserialize)]
pub(super) struct Envelope {
    pub version: u8,
    pub target_id: String,
    pub identity: String,
    pub target: ChannelMessageTarget,
    pub context: Option<String>,
    pub progress: Option<String>,
    pub payload: Payload,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind")]
pub(super) enum Payload {
    Text {
        message: Message,
    },
    File {
        key: String,
        name: String,
        mime: String,
        size: u64,
        digest: String,
    },
}

#[derive(Serialize, Deserialize)]
pub(super) struct Message {
    title: Option<String>,
    body: String,
    fields: Vec<(String, String)>,
    level: MessageLevel,
}

impl From<RichMessage> for Message {
    fn from(value: RichMessage) -> Self {
        Self {
            title: value.title,
            body: value.body,
            fields: value.fields,
            level: value.level,
        }
    }
}

impl Message {
    pub fn rich(&self) -> RichMessage {
        RichMessage {
            title: self.title.clone(),
            body: self.body.clone(),
            fields: self.fields.clone(),
            level: self.level,
        }
    }
}

pub(super) fn hash(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

pub(super) async fn identity(
    runtime: &Runtime,
    channel_id: i32,
) -> Result<String, ChatChannelError> {
    let channel = chat_channel_service::get_by_id(&runtime.db, channel_id)
        .await
        .map_err(|_| failure("CHANNEL_QUERY_FAILED"))?
        .ok_or(ChatChannelError::NotFound(channel_id))?;
    if !channel.enabled {
        return Err(failure("CHANNEL_DISABLED"));
    }
    let secret = crate::keyring_store::try_get_channel_token(channel_id)
        .map_err(|_| failure("CHANNEL_CREDENTIAL_UNAVAILABLE"))?
        .unwrap_or_default();
    let legacy_bot = if channel.channel_type == "wecom" {
        let data_dir = runtime
            .spool
            .parent()
            .ok_or_else(|| failure("CHANNEL_DATA_DIR_UNAVAILABLE"))?;
        crate::chat_channel::backends::wecom::authorized_bot_id(data_dir).await?
    } else {
        String::new()
    };
    Ok(hash(
        format!(
            "{}\n{}\n{}\n{}",
            channel.channel_type,
            account_config(&channel.config_json)?,
            secret,
            legacy_bot
        )
        .as_bytes(),
    ))
}

fn account_config(encoded: &str) -> Result<String, ChatChannelError> {
    let config: serde_json::Value =
        serde_json::from_str(encoded).map_err(|_| failure("CHANNEL_CONFIG_INVALID"))?;
    let mut identity = serde_json::Map::new();
    for key in [
        "app_id",
        "client_id",
        "bot_id",
        "corp_id",
        "agent_id",
        "base_url",
        "lark_region",
    ] {
        if let Some(value) = config.get(key) {
            identity.insert(key.into(), value.clone());
        }
    }
    Ok(serde_json::Value::Object(identity).to_string())
}

pub(super) async fn prepare(
    runtime: &Runtime,
    target: &ChannelMessageTarget,
    content: Content,
) -> Result<Envelope, ChatChannelError> {
    let row = chat_channel_target_service::find_by_target(&runtime.db, target)
        .await
        .map_err(|_| failure("TARGET_QUERY_FAILED"))?
        .ok_or_else(|| failure("TARGET_NOT_REGISTERED"))?;
    let identity = identity(runtime, target.channel_id).await?;
    let (payload, progress) = match content {
        Content::Text(body) => (
            Payload::Text {
                message: body.into(),
            },
            None,
        ),
        Content::Progress(body, task) => (
            Payload::Text {
                message: body.into(),
            },
            task,
        ),
        Content::File(file) => (super::spool::stage(runtime, file).await?, None),
    };
    let mut address = target.clone();
    address.provider_payload = None;
    Ok(Envelope {
        version: 1,
        target_id: row.target_id,
        identity,
        target: address,
        context: None,
        progress,
        payload,
    })
}

pub(super) async fn target(
    runtime: &Runtime,
    envelope: &Envelope,
) -> Result<ChannelMessageTarget, ChatChannelError> {
    let channel_id = envelope.target.channel_id;
    if envelope.version != 1 {
        return Err(failure("DELIVERY_VERSION_UNSUPPORTED"));
    }
    if identity(runtime, channel_id).await? != envelope.identity {
        return Err(failure("DELIVERY_ACCOUNT_CHANGED"));
    }
    let (_, mut target) =
        chat_channel_target_service::resolve(&runtime.db, channel_id, &envelope.target_id)
            .await
            .map_err(|_| failure("TARGET_UNAVAILABLE"))?;
    if !target.matches_thread(&envelope.target) {
        return Err(failure("DELIVERY_TARGET_CHANGED"));
    }
    let token = context(runtime, &target).await;
    let payload = target
        .provider_payload
        .get_or_insert_with(|| serde_json::json!({}));
    if !payload.is_object() {
        *payload = serde_json::json!({});
    }
    payload["iyw_managed_delivery"] = true.into();
    if let Some(token) = token {
        payload["context_token"] = token.into();
    } else if let Some(object) = payload.as_object_mut() {
        object.remove("context_token");
    }
    Ok(target)
}

pub(super) async fn context(runtime: &Runtime, target: &ChannelMessageTarget) -> Option<String> {
    sender_context_service::get_weixin_context_token(
        &runtime.db,
        target.channel_id,
        target.chat_id.as_deref()?,
    )
    .await
    .ok()
    .flatten()
    .filter(|value| !value.is_empty())
}

pub(super) fn failure(message: &str) -> ChatChannelError {
    ChatChannelError::ConfigurationInvalid(message.into())
}
