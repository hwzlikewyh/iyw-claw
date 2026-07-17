use sea_orm::DatabaseConnection;

use super::i18n::{self, Lang};
use super::session_dispatch::SessionCommandMessage;
use super::types::{ButtonStyle, InteractiveMessage, MessageButton, RichMessage};
use crate::acp::registry::all_acp_agents;
use crate::db::service::{folder_service, sender_context_service};
use crate::models::agent::AgentType;

pub async fn handle_folder_picker(
    db: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
    lang: Lang,
    prefix: &str,
) -> SessionCommandMessage {
    let folders = match folder_service::list_folders(db).await {
        Ok(folders) => folders,
        Err(error) => {
            return RichMessage::error(format!(
                "{}{error}",
                i18n::failed_to_list_folders_label(lang)
            ))
            .into()
        }
    };
    if folders.is_empty() {
        return RichMessage::info(i18n::no_folders_found(lang))
            .with_title(i18n::folder_title(lang))
            .into();
    }
    let context = sender_context_service::get_or_create(db, channel_id, sender_id)
        .await
        .ok();
    let mut body = String::new();
    let mut buttons = Vec::new();
    for (index, folder) in folders.iter().take(10).enumerate() {
        let current = context.as_ref().and_then(|value| value.current_folder_id) == Some(folder.id);
        let marker = if current { " [*]" } else { "" };
        body.push_str(&format!(
            "{}. {}{} ({})\n",
            index + 1,
            folder.name,
            marker,
            folder.path
        ));
        buttons.push(button(
            format!("cfg:folder:{}", folder.id),
            format!("{}. {}", index + 1, folder.name),
        ));
    }
    body.push_str(&format!("\n{}", i18n::folder_select_hint(lang, prefix)));
    SessionCommandMessage::Interactive(InteractiveMessage {
        base: RichMessage::info(body.trim_end()).with_title(i18n::folder_title(lang)),
        buttons,
        callback_context: serde_json::json!({ "kind": "folder" }),
    })
}

pub async fn handle_agent_picker(
    db: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
    lang: Lang,
    prefix: &str,
) -> SessionCommandMessage {
    let agents = all_acp_agents();
    let context = sender_context_service::get_or_create(db, channel_id, sender_id)
        .await
        .ok();
    let mut body = String::new();
    let mut buttons = Vec::new();
    for (index, agent) in agents.iter().enumerate() {
        let wire_id = agent_wire_id(*agent);
        let current = context
            .as_ref()
            .and_then(|value| value.current_agent_type.as_deref())
            == Some(wire_id.as_str());
        let marker = if current { " [*]" } else { "" };
        body.push_str(&format!("{}. {}{}\n", index + 1, agent, marker));
        buttons.push(button(format!("cfg:agent:{wire_id}"), agent.to_string()));
    }
    body.push_str(&format!("\n{}", i18n::agent_select_hint(lang, prefix)));
    SessionCommandMessage::Interactive(InteractiveMessage {
        base: RichMessage::info(body.trim_end()).with_title(i18n::agent_title(lang)),
        buttons,
        callback_context: serde_json::json!({ "kind": "agent" }),
    })
}

pub async fn handle_callback(
    db: &DatabaseConnection,
    data: &str,
    channel_id: i32,
    sender_id: &str,
    lang: Lang,
    prefix: &str,
) -> RichMessage {
    if let Some(folder_id) = data.strip_prefix("cfg:folder:") {
        return match folder_id.parse::<i32>() {
            Ok(folder_id) => select_folder(db, folder_id, channel_id, sender_id, lang).await,
            Err(_) => RichMessage::info(expired_callback(lang, prefix)),
        };
    }
    if let Some(agent) = data.strip_prefix("cfg:agent:") {
        return select_agent(db, agent, channel_id, sender_id, lang).await;
    }
    RichMessage::info(expired_callback(lang, prefix))
}

async fn select_folder(
    db: &DatabaseConnection,
    folder_id: i32,
    channel_id: i32,
    sender_id: &str,
    lang: Lang,
) -> RichMessage {
    let folder = match folder_service::get_folder_by_id(db, folder_id).await {
        Ok(Some(folder)) => folder,
        _ => return RichMessage::info(i18n::folder_not_found(lang)),
    };
    let _ = sender_context_service::update_folder(db, channel_id, sender_id, Some(folder.id)).await;
    RichMessage::info(format!("{} ({})", folder.name, folder.path))
        .with_title(i18n::folder_selected_title(lang))
}

async fn select_agent(
    db: &DatabaseConnection,
    name: &str,
    channel_id: i32,
    sender_id: &str,
    lang: Lang,
) -> RichMessage {
    let normalized = name.to_lowercase().replace([' ', '-'], "_");
    let agent: AgentType = match serde_json::from_value(normalized.into()) {
        Ok(agent) => agent,
        Err(_) => return RichMessage::info(format!("{}{}", i18n::unknown_agent_label(lang), name)),
    };
    let _ =
        sender_context_service::update_agent(db, channel_id, sender_id, Some(agent_wire_id(agent)))
            .await;
    RichMessage::info(agent.to_string()).with_title(i18n::agent_selected_title(lang))
}

fn button(id: String, label: String) -> MessageButton {
    MessageButton {
        id,
        label: truncate(&label, 40),
        style: ButtonStyle::Default,
    }
}

fn truncate(label: &str, max_chars: usize) -> String {
    if label.chars().count() <= max_chars {
        label.to_string()
    } else {
        format!(
            "{}...",
            label.chars().take(max_chars - 3).collect::<String>()
        )
    }
}

fn agent_wire_id(agent: AgentType) -> String {
    serde_json::to_value(agent)
        .ok()
        .and_then(|value| value.as_str().map(ToString::to_string))
        .unwrap_or_default()
}

fn expired_callback(lang: Lang, prefix: &str) -> String {
    match lang {
        Lang::ZhCn | Lang::ZhTw => {
            format!("这个按钮已失效。请重新发送 {prefix}folder 或 {prefix}agent。")
        }
        _ => format!("This button is no longer valid. Send {prefix}folder or {prefix}agent again."),
    }
}
