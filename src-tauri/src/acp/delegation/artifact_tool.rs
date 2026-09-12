use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_FILES: usize = 100;
const MAX_REFERENCE_CHARS: usize = 4096;
const MAX_NAME_CHARS: usize = 255;
const MAX_PAGE_SIZE: u64 = 100;

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactScope {
    #[default]
    Current,
    All,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactListQuery {
    #[serde(default)]
    pub scope: ArtifactScope,
    pub search: Option<String>,
    pub message_id: Option<String>,
    pub page: Option<u64>,
    pub page_size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ArtifactOperation {
    List(ArtifactListQuery),
    Get {
        artifact_id: i32,
        #[serde(default)]
        scope: ArtifactScope,
    },
    Update {
        artifact_id: i32,
        display_name: Option<String>,
        source: Option<String>,
    },
    Delete {
        artifact_id: i32,
    },
}

pub struct ArtifactContext {
    pub connection_id: String,
    pub conversation_id: i32,
    pub turn_generation: Option<i64>,
    pub working_dir: PathBuf,
}

pub enum ArtifactCall {
    Present(Vec<String>),
    Manage(ArtifactOperation),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PresentParams {
    files: Vec<String>,
    action: Option<String>,
}

pub fn parse_call(arguments: &Value) -> Result<ArtifactCall, String> {
    match arguments.get("action") {
        None => parse_present(arguments).map(ArtifactCall::Present),
        Some(Value::String(action)) if action == "present" => {
            parse_present(arguments).map(ArtifactCall::Present)
        }
        _ => {
            let mut operation: ArtifactOperation = serde_json::from_value(arguments.clone())
                .map_err(|error| format!("Invalid artifact operation: {error}"))?;
            operation.validate()?;
            Ok(ArtifactCall::Manage(operation))
        }
    }
}

fn parse_present(arguments: &Value) -> Result<Vec<String>, String> {
    let params: PresentParams = serde_json::from_value(arguments.clone())
        .map_err(|error| format!("Invalid artifact registration: {error}"))?;
    let _ = params.action;
    if params.files.is_empty() || params.files.len() > MAX_FILES {
        return Err(format!(
            "present_task_files requires 1 to {MAX_FILES} final deliverables"
        ));
    }
    params
        .files
        .into_iter()
        .map(|mut reference| {
            normalize_text(&mut reference, MAX_REFERENCE_CHARS, "files")?;
            Ok(reference)
        })
        .collect()
}

impl ArtifactOperation {
    pub fn action(&self) -> &'static str {
        match self {
            Self::List(_) => "list",
            Self::Get { .. } => "get",
            Self::Update { .. } => "update",
            Self::Delete { .. } => "delete",
        }
    }

    pub fn mutates(&self) -> bool {
        matches!(self, Self::Update { .. } | Self::Delete { .. })
    }

    pub fn artifact_id(&self) -> Option<i32> {
        match self {
            Self::List(_) => None,
            Self::Get { artifact_id, .. }
            | Self::Update { artifact_id, .. }
            | Self::Delete { artifact_id } => Some(*artifact_id),
        }
    }

    pub fn validate(&mut self) -> Result<(), String> {
        if self.artifact_id().is_some_and(|id| id <= 0) {
            return Err("artifact_id must be a positive integer".into());
        }
        match self {
            Self::List(query) => validate_query(query),
            Self::Update {
                display_name,
                source,
                ..
            } => {
                if display_name.is_none() && source.is_none() {
                    return Err("update requires display_name or source".into());
                }
                normalize_optional(display_name, MAX_NAME_CHARS, "display_name")?;
                normalize_optional(source, MAX_REFERENCE_CHARS, "source")
            }
            _ => Ok(()),
        }
    }
}

fn validate_query(query: &mut ArtifactListQuery) -> Result<(), String> {
    if query.page == Some(0)
        || query
            .page_size
            .is_some_and(|size| size == 0 || size > MAX_PAGE_SIZE)
    {
        return Err(format!(
            "page must be positive; page_size must be 1 to {MAX_PAGE_SIZE}"
        ));
    }
    if query
        .search
        .as_ref()
        .is_some_and(|value| value.trim().is_empty())
    {
        query.search = None;
    }
    normalize_optional(&mut query.search, MAX_REFERENCE_CHARS, "search")?;
    normalize_optional(&mut query.message_id, MAX_REFERENCE_CHARS, "message_id")
}

fn normalize_optional(value: &mut Option<String>, limit: usize, field: &str) -> Result<(), String> {
    match value {
        Some(value) => normalize_text(value, limit, field),
        None => Ok(()),
    }
}

fn normalize_text(value: &mut String, limit: usize, field: &str) -> Result<(), String> {
    *value = value.trim().to_owned();
    if value.is_empty() || value.contains('\0') || value.chars().count() > limit {
        return Err(format!(
            "{field} must contain 1 to {limit} characters without NUL"
        ));
    }
    Ok(())
}
