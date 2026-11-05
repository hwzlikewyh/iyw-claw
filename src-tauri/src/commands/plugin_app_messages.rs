use super::plugin_app_tools::{find_instance, plugin_error};

type PluginAppMessageBlocks = (Vec<crate::acp::types::PromptInputBlock>, String);

pub(super) async fn send_app_message(
    db: &crate::db::AppDatabase,
    manager: &crate::acp::manager::ConnectionManager,
    instance_id: &str,
    params: serde_json::Value,
) -> Result<(), crate::plugin_runtime::types::PluginInvokeError> {
    let instance = find_instance(db, instance_id).await?;
    if params.get("role").and_then(serde_json::Value::as_str) != Some("user") {
        return Err(plugin_error(
            "plugin_app_invalid",
            "ui/message only accepts user messages",
        ));
    }
    let blocks = content_blocks(&params)?;
    crate::commands::agent_input::queue_agent_input_core(
        db,
        manager,
        instance.conversation_id as i32,
        format!("plugin-app-{}", uuid::Uuid::new_v4()),
        crate::acp::AgentInputPayload {
            blocks: blocks.0,
            display_text: blocks.1,
            mode_id: None,
        },
    )
    .await
    .map(|_| ())
    .map_err(|error| plugin_error("plugin_app_message_failed", error.to_string()))
}

fn content_blocks(
    params: &serde_json::Value,
) -> Result<PluginAppMessageBlocks, crate::plugin_runtime::types::PluginInvokeError> {
    let content = params
        .get("content")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| plugin_error("plugin_app_invalid", "ui/message content is invalid"))?;
    let mut result = (Vec::new(), String::new());
    for item in content {
        match item.get("type").and_then(serde_json::Value::as_str) {
            Some("text") => append_text(&mut result, item)?,
            Some("image") => append_image(&mut result.0, item)?,
            _ => {
                return Err(plugin_error(
                    "plugin_app_invalid",
                    "ui/message content block is unsupported",
                ))
            }
        }
    }
    if result.0.is_empty() {
        return Err(plugin_error(
            "plugin_app_invalid",
            "ui/message content is empty",
        ));
    }
    Ok(result)
}

fn append_text(
    result: &mut PluginAppMessageBlocks,
    item: &serde_json::Value,
) -> Result<(), crate::plugin_runtime::types::PluginInvokeError> {
    let text = item["text"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| plugin_error("plugin_app_invalid", "ui/message text is invalid"))?;
    if !result.1.is_empty() {
        result.1.push('\n');
    }
    result.1.push_str(text);
    result
        .0
        .push(crate::acp::types::PromptInputBlock::Text { text: text.into() });
    Ok(())
}

fn append_image(
    blocks: &mut Vec<crate::acp::types::PromptInputBlock>,
    item: &serde_json::Value,
) -> Result<(), crate::plugin_runtime::types::PluginInvokeError> {
    let data = item["data"].as_str().unwrap_or_default();
    let mime_type = item
        .get("mimeType")
        .or_else(|| item.get("mime_type"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if data.is_empty() || mime_type.is_empty() {
        return Err(plugin_error(
            "plugin_app_invalid",
            "ui/message image is invalid",
        ));
    }
    blocks.push(crate::acp::types::PromptInputBlock::Image {
        data: data.into(),
        mime_type: mime_type.into(),
        uri: item["uri"].as_str().map(str::to_string),
        local_path: None,
    });
    Ok(())
}
