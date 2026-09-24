use rmcp::ErrorData;
use serde::Deserialize;
use serde_json::json;

const MAX_QUERY_CHARS: usize = 256;
const MAX_CURSOR_CHARS: usize = 128;
const MAX_LIMIT: usize = 20;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SearchParams {
    #[serde(default)]
    pub query: String,
    pub limit: Option<usize>,
    pub source: Option<SearchSource>,
    #[serde(default)]
    pub mode: SearchMode,
    group_id: Option<String>,
    cursor: Option<String>,
}

#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(super) enum SearchSource {
    All,
    Local,
    Remote,
}

#[derive(Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(super) enum SearchMode {
    #[default]
    Search,
    Browse,
}

impl SearchParams {
    pub(super) fn validate(&self) -> Result<(), ErrorData> {
        if self
            .limit
            .is_some_and(|limit| !(1..=MAX_LIMIT).contains(&limit))
        {
            return Err(invalid("limit must be between 1 and 20"));
        }
        if self.query.chars().count() > MAX_QUERY_CHARS {
            return Err(invalid("query is too long"));
        }
        if self.group_id.as_ref().is_some_and(|id| {
            id.is_empty() || id.chars().count() > super::tool_identity::CAPABILITY_ID_MAX_CHARS
        }) || self
            .cursor
            .as_ref()
            .is_some_and(|cursor| cursor.is_empty() || cursor.chars().count() > MAX_CURSOR_CHARS)
        {
            return Err(invalid("group_id or cursor is empty or too long"));
        }
        if self.mode == SearchMode::Browse {
            if self.source != Some(SearchSource::Remote) || !self.query.trim().is_empty() {
                return Err(invalid("browse requires source=remote and no query"));
            }
        } else if self.query.trim().is_empty() || self.group_id.is_some() || self.cursor.is_some() {
            return Err(invalid(
                "search requires a query; group_id/cursor are only valid in browse mode",
            ));
        }
        Ok(())
    }

    pub(super) fn remote_only(&self) -> bool {
        self.source == Some(SearchSource::Remote)
    }
}

fn invalid(message: &str) -> ErrorData {
    ErrorData::invalid_params(
        message.to_owned(),
        Some(json!({
            "code": "capability_schema_mismatch", "execution_status": "not_started"
        })),
    )
}
