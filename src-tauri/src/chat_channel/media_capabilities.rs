use serde_json::{json, Value};

use super::attachments::{AttachmentCapability, MAX_MESSAGE_ATTACHMENTS, MIB};

pub fn view(channel_type: &str) -> Value {
    let files = AttachmentCapability::for_channel(channel_type);
    let images: &[&str] = match channel_type {
        "lark" => &[
            "image/jpeg",
            "image/png",
            "image/webp",
            "image/gif",
            "image/bmp",
            "image/x-icon",
            "image/tiff",
            "image/heic",
        ],
        "wecom_ai_bot" => &["image/jpeg", "image/png", "image/gif"],
        "wecom_agent" => &["image/jpeg", "image/png"],
        "dingtalk" => &["image/jpeg", "image/png", "image/gif", "image/bmp"],
        "weixin" => &["image/jpeg", "image/png", "image/webp", "image/gif"],
        _ => &[],
    };
    json!({
        "text": true, "rich_text": true,
        "attachments": files.supported, "max_file_bytes": files.max_file_bytes,
        "min_file_bytes": if matches!(channel_type, "wecom_ai_bot" | "wecom_agent") { 5 } else { 1 },
        "images": !images.is_empty(), "image_mime_types": images,
        "max_image_bytes": if images.is_empty() { None } else { Some(10 * MIB) },
        "max_files_per_message": MAX_MESSAGE_ATTACHMENTS,
        "receives_attachments": matches!(channel_type, "lark" | "wecom_ai_bot" | "wecom_agent" | "dingtalk" | "weixin"),
        "conditional_attachment_receive": channel_type == "wecom",
        "receive_scope": if channel_type == "wecom_ai_bot" { "single_chat_media; mixed_messages_where_provider_supports" } else { "provider_authorized_conversations" },
        "requirements": requirements(channel_type),
    })
}

fn requirements(channel_type: &str) -> &'static str {
    match channel_type {
        "wecom" => "CLI discovery must expose media upload/download and message aibot; target must match a current robot session. Upload receipt determines media type; images may be delivered as files.",
        "wecom_ai_bot" => "Recipient must have messaged the bot; image/file callbacks are limited to single chat. Native media upload requires 5 bytes minimum.",
        "wecom_agent" => "Application media/message permissions and an authorized recipient; native media upload requires 5 bytes minimum.",
        "dingtalk" => "Robot media/message permissions; single-chat target requires senderStaffId, group target requires openConversationId.",
        "weixin" => "A valid recipient-specific context token is required. Expired context is a failed delivery.",
        "lark" => "Message resource read and media upload permissions; existing group mention filtering applies.",
        _ => "Attachments are not supported.",
    }
}

pub fn native_image(channel_type: &str, mime: &str, bytes: u64) -> bool {
    let profile = view(channel_type);
    profile["max_image_bytes"]
        .as_u64()
        .is_some_and(|limit| bytes <= limit)
        && profile["image_mime_types"]
            .as_array()
            .is_some_and(|types| types.iter().any(|kind| kind == mime))
}
