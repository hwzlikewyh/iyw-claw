use serde::Deserialize;
use serde_json::Value;

use super::features::FeatureSnapshot;

const DEFAULT_SEARCH_LIMIT: usize = 8;
const MAX_SEARCH_LIMIT: usize = 20;
const MAX_SEARCH_QUERY_CHARS: usize = 256;

#[derive(Debug, Clone, serde::Serialize)]
pub(super) struct CapabilitySummary {
    pub capability_id: &'static str,
    pub summary: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(super) struct CapabilityDetail {
    pub capability_id: &'static str,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone)]
pub(super) struct ResolvedCapability {
    pub tool_name: String,
    pub arguments: Value,
    pub delivery_ack: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmbeddedTool {
    name: String,
    description: String,
    input_schema: Value,
}

#[derive(Debug)]
struct CatalogEntry {
    id: &'static str,
    tool: EmbeddedTool,
}

pub(super) struct CapabilityCatalog {
    entries: Vec<CatalogEntry>,
}

impl CapabilityCatalog {
    pub(super) fn load() -> Result<Self, CatalogError> {
        let tools = serde_json::from_str::<Vec<EmbeddedTool>>(
            crate::acp::delegation::companion::TOOL_SCHEMA_JSON,
        )?;
        let entries = tools
            .into_iter()
            .map(|tool| {
                let id = stable_capability_id(&tool.name)
                    .ok_or_else(|| CatalogError::MissingStableId(tool.name.clone()))?;
                Ok(CatalogEntry { id, tool })
            })
            .collect::<Result<Vec<_>, CatalogError>>()?;
        Ok(Self { entries })
    }

    pub(super) fn search(
        &self,
        features: &FeatureSnapshot,
        query: &str,
        limit: Option<usize>,
    ) -> Result<Vec<CapabilitySummary>, SearchError> {
        let query = query.trim().to_ascii_lowercase();
        if query.is_empty() {
            return Err(SearchError::EmptyQuery);
        }
        if query.chars().count() > MAX_SEARCH_QUERY_CHARS {
            return Err(SearchError::QueryTooLong);
        }
        let limit = limit.unwrap_or(DEFAULT_SEARCH_LIMIT);
        if !(1..=MAX_SEARCH_LIMIT).contains(&limit) {
            return Err(SearchError::InvalidLimit);
        }
        let mut matches = self
            .available(features)
            .filter_map(|entry| search_match(entry, &query))
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then(left.1.capability_id.cmp(right.1.capability_id))
        });
        Ok(matches
            .into_iter()
            .take(limit)
            .map(|(_, summary)| summary)
            .collect())
    }

    pub(super) fn read(
        &self,
        features: &FeatureSnapshot,
        capability_id: &str,
    ) -> Option<CapabilityDetail> {
        let entry = self.find_available(features, capability_id)?;
        Some(CapabilityDetail {
            capability_id: entry.id,
            description: public_text(&entry.tool.description),
            input_schema: public_value(entry.tool.input_schema.clone()),
        })
    }

    pub(super) fn resolve(
        &self,
        features: &FeatureSnapshot,
        capability_id: &str,
        arguments: Value,
    ) -> Option<ResolvedCapability> {
        let entry = self.find_available(features, capability_id)?;
        features.authorize_call(&entry.tool.name).ok()?;
        Some(ResolvedCapability {
            tool_name: entry.tool.name.clone(),
            arguments,
            delivery_ack: None,
        })
    }

    fn available<'a>(
        &'a self,
        features: &'a FeatureSnapshot,
    ) -> impl Iterator<Item = &'a CatalogEntry> {
        self.entries
            .iter()
            .filter(|entry| features.should_list(&entry.tool.name))
    }

    fn find_available<'a>(
        &'a self,
        features: &FeatureSnapshot,
        capability_id: &str,
    ) -> Option<&'a CatalogEntry> {
        self.entries
            .iter()
            .find(|entry| entry.id == capability_id && features.should_list(&entry.tool.name))
    }
}

fn search_match(entry: &CatalogEntry, query: &str) -> Option<(usize, CapabilitySummary)> {
    let haystack = format!("{} {}", entry.id, entry.tool.description).to_ascii_lowercase();
    let score = query
        .split_whitespace()
        .filter(|token| haystack.contains(token))
        .count();
    if score == 0 && !haystack.contains(query) {
        return None;
    }
    let exact_bonus = usize::from(entry.id.eq_ignore_ascii_case(query)) * 100;
    Some((
        score + exact_bonus,
        CapabilitySummary {
            capability_id: entry.id,
            summary: public_text(&first_sentence(&entry.tool.description)),
        },
    ))
}

fn first_sentence(description: &str) -> String {
    description
        .split_once(". ")
        .map_or(description, |(first, _)| first)
        .trim()
        .to_string()
}

fn public_text(text: &str) -> String {
    CAPABILITY_BINDINGS
        .iter()
        .fold(text.to_string(), |value, (tool_name, capability_id)| {
            value.replace(tool_name, capability_id)
        })
}

fn public_value(value: Value) -> Value {
    match value {
        Value::String(text) => Value::String(public_text(&text)),
        Value::Array(values) => Value::Array(values.into_iter().map(public_value).collect()),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, public_value(value)))
                .collect(),
        ),
        other => other,
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum CatalogError {
    #[error("invalid embedded companion schema: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("companion tool `{0}` has no stable capability id")]
    MissingStableId(String),
}

#[derive(Debug, thiserror::Error)]
pub(super) enum SearchError {
    #[error("query must be a non-empty string")]
    EmptyQuery,
    #[error("query must be at most {MAX_SEARCH_QUERY_CHARS} characters")]
    QueryTooLong,
    #[error("limit must be between 1 and {MAX_SEARCH_LIMIT}")]
    InvalidLimit,
}

fn stable_capability_id(tool_name: &str) -> Option<&'static str> {
    CAPABILITY_BINDINGS
        .iter()
        .find_map(|(name, id)| (*name == tool_name).then_some(*id))
}

pub(super) fn tool_name_for_capability_id(capability_id: &str) -> Option<&'static str> {
    CAPABILITY_BINDINGS
        .iter()
        .find_map(|(name, id)| (*id == capability_id).then_some(*name))
}

const CAPABILITY_BINDINGS: [(&str, &str); 38] = [
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
    ("browser_click", "iyw.browser.element.click.v1"),
    ("browser_fill", "iyw.browser.element.fill.v1"),
    ("browser_press", "iyw.browser.keyboard.press.v1"),
    ("browser_scroll", "iyw.browser.page.scroll.v1"),
    ("browser_wait", "iyw.browser.page.wait.v1"),
    ("browser_screenshot", "iyw.browser.page.screenshot.v1"),
    ("browser_close_tab", "iyw.browser.tabs.close.v1"),
    ("present_task_files", "iyw.artifacts.present.v1"),
    ("delegate_to_agent", "iyw.delegation.tasks.create.v1"),
    ("get_delegation_status", "iyw.delegation.tasks.read.v1"),
    ("cancel_delegation", "iyw.delegation.tasks.cancel.v1"),
    ("check_user_feedback", "iyw.interaction.feedback.read.v1"),
    ("ask_user_question", "iyw.interaction.question.ask.v1"),
    ("get_session_info", "iyw.session.info.read.v1"),
    ("transcribe_audio", "iyw.audio.transcription.create.v1"),
    (
        "query_audio_transcription",
        "iyw.audio.transcription.read.v1",
    ),
    ("show_image", "iyw.image.present.v1"),
    ("analyze_image", "iyw.image.analyze.v1"),
    ("append_user_memory", "iyw.memory.confirmed.append.v1"),
    ("propose_user_memory", "iyw.memory.candidate.propose.v1"),
    ("memory_recall", "iyw.memory.recall.search.v1"),
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
