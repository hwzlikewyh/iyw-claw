use std::collections::{BTreeMap, BTreeSet};

pub(super) const CAPABILITY_BINDINGS: [(&str, &str); 46] = [
    (
        "list_scheduled_task_projects",
        "iyw.automation.projects.list.v1",
    ),
    ("list_scheduled_tasks", "iyw.automation.tasks.list.v1"),
    ("create_scheduled_task", "iyw.automation.tasks.create.v1"),
    ("update_scheduled_task", "iyw.automation.tasks.update.v1"),
    ("delete_scheduled_task", "iyw.automation.tasks.delete.v1"),
    ("browser_list_tabs", "iyw.browser.tabs.list.v1"),
    ("browser_open", "iyw.browser.page.open.v1"),
    ("browser_snapshot", "iyw.browser.page.snapshot.v1"),
    ("browser_read", "iyw.browser.page.read.v1"),
    ("browser_click", "iyw.browser.element.click.v1"),
    ("browser_fill", "iyw.browser.element.fill.v1"),
    ("browser_press", "iyw.browser.keyboard.press.v1"),
    ("browser_scroll", "iyw.browser.page.scroll.v1"),
    ("browser_wait", "iyw.browser.page.wait.v1"),
    ("browser_screenshot", "iyw.browser.page.screenshot.v1"),
    ("browser_close_tab", "iyw.browser.tabs.close.v1"),
    ("browser_command", "iyw.browser.command.run.v1"),
    (
        "browser_request_user_action",
        "iyw.browser.user_action.request.v1",
    ),
    ("browser_present", "iyw.browser.window.present.v1"),
    ("browser_close_window", "iyw.browser.window.close.v1"),
    ("present_task_files", "iyw.artifacts.present.v1"),
    ("delegate_to_agent", "iyw.delegation.tasks.create.v1"),
    ("get_delegation_status", "iyw.delegation.tasks.read.v1"),
    ("cancel_delegation", "iyw.delegation.tasks.cancel.v1"),
    ("check_user_feedback", "iyw.interaction.feedback.read.v1"),
    ("ask_user_question", "iyw.interaction.question.ask.v1"),
    ("get_session_info", "iyw.session.info.read.v1"),
    ("transcribe_audio", "iyw.audio.transcription.create.v1"),
    (
        "transcribe_audio_flash",
        "iyw.audio.transcription.flash.create.v1",
    ),
    (
        "query_audio_transcription",
        "iyw.audio.transcription.read.v1",
    ),
    ("show_image", "iyw.image.present.v1"),
    ("analyze_image", "iyw.image.analyze.v1"),
    (
        "get_current_user_profile",
        "iyw.session.user_profile.read.v1",
    ),
    ("append_user_memory", "iyw.memory.confirmed.append.v1"),
    ("propose_user_memory", "iyw.memory.candidate.propose.v1"),
    ("memory_recall", "iyw.memory.recall.search.v1"),
    ("read_user_memory_documents", "iyw.memory.documents.read.v1"),
    ("list_message_channels", "iyw.channels.list.v1"),
    ("save_message_channel", "iyw.channels.save.v1"),
    ("delete_message_channel", "iyw.channels.delete.v1"),
    (
        "manage_channel_credential",
        "iyw.channels.credentials.manage.v1",
    ),
    ("operate_message_channel", "iyw.channels.operate.v1"),
    ("list_channel_targets", "iyw.channels.targets.list.v1"),
    ("list_channel_messages", "iyw.channels.messages.list.v1"),
    ("send_channel_messages", "iyw.channels.messages.send.v1"),
    ("manage_channel_settings", "iyw.channels.settings.manage.v1"),
];

pub(super) fn validate_bindings<'a>(
    schema_names: impl IntoIterator<Item = &'a str>,
) -> Result<(), RegistryError> {
    let schema_names = collect_unique(schema_names, RegistryError::DuplicateToolName)?;
    let binding_names = collect_unique(
        CAPABILITY_BINDINGS.iter().map(|(name, _)| *name),
        RegistryError::DuplicateToolName,
    )?;
    let ids = collect_unique(
        CAPABILITY_BINDINGS.iter().map(|(_, id)| *id),
        RegistryError::DuplicateCapabilityId,
    )?;
    for id in ids {
        if !valid_capability_id(id) {
            return Err(RegistryError::InvalidCapabilityId(id.to_string()));
        }
    }
    if schema_names != binding_names {
        return Err(RegistryError::CoverageMismatch {
            missing_bindings: difference(&schema_names, &binding_names),
            orphan_bindings: difference(&binding_names, &schema_names),
        });
    }
    Ok(())
}

pub(super) fn stable_capability_id(tool_name: &str) -> Option<&'static str> {
    CAPABILITY_BINDINGS
        .iter()
        .find_map(|(name, id)| (*name == tool_name).then_some(*id))
}

pub(super) fn tool_name_for_capability_id(capability_id: &str) -> Option<&'static str> {
    CAPABILITY_BINDINGS
        .iter()
        .find_map(|(name, id)| (*id == capability_id).then_some(*name))
}

fn collect_unique<'a>(
    values: impl IntoIterator<Item = &'a str>,
    duplicate: fn(String) -> RegistryError,
) -> Result<BTreeSet<&'a str>, RegistryError> {
    let mut counts = BTreeMap::new();
    for value in values {
        *counts.entry(value).or_insert(0_u8) += 1;
    }
    if let Some(value) = counts
        .iter()
        .find_map(|(value, count)| (*count > 1).then_some(*value))
    {
        return Err(duplicate(value.to_string()));
    }
    Ok(counts.into_keys().collect())
}

fn difference(left: &BTreeSet<&str>, right: &BTreeSet<&str>) -> Vec<String> {
    left.difference(right)
        .map(|value| (*value).to_string())
        .collect()
}

fn valid_capability_id(value: &str) -> bool {
    let parts = value.split('.').collect::<Vec<_>>();
    parts.len() >= 4
        && parts.first() == Some(&"iyw")
        && parts.last().is_some_and(|part| {
            part.strip_prefix('v').is_some_and(|version| {
                !version.is_empty() && version.chars().all(|c| c.is_ascii_digit())
            })
        })
        && parts[..parts.len() - 1].iter().all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        })
}

#[derive(Debug, thiserror::Error)]
pub(super) enum RegistryError {
    #[error("duplicate embedded tool name `{0}`")]
    DuplicateToolName(String),
    #[error("duplicate stable capability id `{0}`")]
    DuplicateCapabilityId(String),
    #[error("invalid stable capability id `{0}`")]
    InvalidCapabilityId(String),
    #[error(
        "capability binding coverage mismatch; missing bindings: {missing_bindings:?}; orphan bindings: {orphan_bindings:?}"
    )]
    CoverageMismatch {
        missing_bindings: Vec<String>,
        orphan_bindings: Vec<String>,
    },
}
