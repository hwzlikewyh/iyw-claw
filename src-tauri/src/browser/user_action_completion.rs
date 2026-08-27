use serde::Deserialize;
use serde_json::Value;

use super::agent_tool_actions::AgentCliRequest;
use super::agent_tool_cancellation::AgentToolContext;
use super::agent_tool_support::{
    invalid_argument, MAX_SELECTOR_CHARS, MAX_TEXT_CHARS, SNAPSHOT_TIMEOUT,
};
use super::error::{BrowserError, BrowserErrorCode};
use super::manager::BrowserSessionManager;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct UserActionCompletion {
    #[serde(default)]
    pub(super) url_contains: Option<String>,
    #[serde(default)]
    pub(super) title_contains: Option<String>,
    #[serde(default)]
    pub(super) text_contains: Option<String>,
    #[serde(default)]
    pub(super) selector: Option<String>,
    #[serde(default)]
    pub(super) download_completed: bool,
}

impl UserActionCompletion {
    fn is_empty(&self) -> bool {
        self.url_contains.is_none()
            && self.title_contains.is_none()
            && self.text_contains.is_none()
            && self.selector.is_none()
            && !self.download_completed
    }
}

impl BrowserSessionManager {
    pub(super) async fn evaluate_user_action_completion(
        &self,
        context: AgentToolContext<'_>,
        tab_id: &str,
        completion: &UserActionCompletion,
    ) -> Result<bool, BrowserError> {
        let state = self.snapshot().await;
        let tab = state
            .tabs
            .iter()
            .find(|tab| tab.browser_tab_id == tab_id)
            .ok_or_else(|| BrowserError::tab_not_found(tab_id))?;
        if !state_conditions_match(completion, tab, &state, tab_id) {
            return Ok(false);
        }
        if completion.selector.is_none() && completion.text_contains.is_none() {
            return Ok(true);
        }
        self.set_user_held(tab_id, false).await?;
        let output = self.read_completion_snapshot(context, tab_id).await;
        self.set_user_held(tab_id, true).await?;
        let output = match output {
            Ok(output) => output,
            Err(error) if error.code == BrowserErrorCode::BrowserControlChanged => {
                return Ok(false)
            }
            Err(error) => return Err(error),
        };
        let text_matches = completion
            .text_contains
            .as_ref()
            .is_none_or(|value| output.to_string().contains(value));
        if !text_matches {
            return Ok(false);
        }
        let Some(selector) = completion.selector.as_ref() else {
            return Ok(true);
        };
        self.set_user_held(tab_id, false).await?;
        let selector_result = self
            .wait_for_completion_selector(context, tab_id, selector)
            .await;
        self.set_user_held(tab_id, true).await?;
        match selector_result {
            Ok(_) => Ok(true),
            Err(error) if error.code == BrowserErrorCode::BrowserOperationTimeout => Ok(false),
            Err(error) if error.code == BrowserErrorCode::BrowserControlChanged => Ok(false),
            Err(error) => Err(error),
        }
    }

    async fn read_completion_snapshot(
        &self,
        context: AgentToolContext<'_>,
        tab_id: &str,
    ) -> Result<Value, BrowserError> {
        self.run_agent_cli(AgentCliRequest {
            context,
            tab_id,
            args: vec!["snapshot".to_string(), "--compact".to_string()],
            timeout: SNAPSHOT_TIMEOUT,
        })
        .await
    }

    async fn wait_for_completion_selector(
        &self,
        context: AgentToolContext<'_>,
        tab_id: &str,
        selector: &str,
    ) -> Result<Value, BrowserError> {
        self.run_agent_cli(AgentCliRequest {
            context,
            tab_id,
            args: vec!["wait".to_string(), selector.to_string()],
            timeout: SNAPSHOT_TIMEOUT,
        })
        .await
    }
}

fn state_conditions_match(
    completion: &UserActionCompletion,
    tab: &super::types::BrowserTabSnapshot,
    state: &super::types::BrowserStateSnapshot,
    tab_id: &str,
) -> bool {
    completion
        .url_contains
        .as_ref()
        .is_none_or(|value| tab.url.contains(value))
        && completion
            .title_contains
            .as_ref()
            .is_none_or(|value| tab.title.contains(value))
        && (!completion.download_completed
            || state.downloads.iter().any(|download| {
                download.browser_tab_id.as_deref() == Some(tab_id)
                    && download.status == super::types_cdp::BrowserDownloadStatus::Completed
            }))
}

pub(super) fn parse_completion(
    input: &Value,
) -> Result<Option<UserActionCompletion>, BrowserError> {
    let Some(value) = input.get("completion") else {
        return Ok(None);
    };
    let completion = serde_json::from_value::<UserActionCompletion>(value.clone())
        .map_err(|_| invalid_argument("Invalid browser completion condition"))?;
    if completion.is_empty()
        || completion
            .url_contains
            .as_ref()
            .is_some_and(|value| value.chars().count() > 2_048)
        || completion
            .title_contains
            .as_ref()
            .is_some_and(|value| value.chars().count() > 512)
        || completion
            .selector
            .as_ref()
            .is_some_and(|value| value.chars().count() > MAX_SELECTOR_CHARS)
        || completion
            .text_contains
            .as_ref()
            .is_some_and(|value| value.chars().count() > MAX_TEXT_CHARS)
    {
        return Err(invalid_argument("Invalid browser completion condition"));
    }
    Ok(Some(completion))
}
