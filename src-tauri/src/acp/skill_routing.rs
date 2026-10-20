use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub const ROUTING_DESCRIPTION_MAX_CHARS: usize = 240;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillRoutingCard {
    pub capability: String,
    #[serde(alias = "core_triggers")]
    pub core_triggers: Vec<String>,
    pub exclusions: Vec<String>,
    pub aliases: Vec<String>,
    pub invocation: String,
}

impl SkillRoutingCard {
    pub fn character_count(&self) -> usize {
        self.capability.chars().count()
            + self.invocation.chars().count()
            + self
                .core_triggers
                .iter()
                .chain(&self.exclusions)
                .chain(&self.aliases)
                .map(|value| value.chars().count())
                .sum::<usize>()
    }

    fn normalize(mut self) -> Self {
        self.capability = self.capability.trim().to_string();
        self.invocation = self.invocation.trim().to_string();
        normalize_list(&mut self.core_triggers);
        normalize_list(&mut self.exclusions);
        normalize_list(&mut self.aliases);
        self
    }

    fn validate(&self) -> Result<(), SkillRoutingError> {
        if self.capability.is_empty() {
            return Err(SkillRoutingError::Invalid(
                "routing.capability must be non-empty".to_string(),
            ));
        }
        if self.core_triggers.is_empty() {
            return Err(SkillRoutingError::Invalid(
                "routing.coreTriggers must contain at least one trigger".to_string(),
            ));
        }
        if self.invocation.is_empty() {
            return Err(SkillRoutingError::Invalid(
                "routing.invocation must be non-empty".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillRoutingStatus {
    Valid,
    #[default]
    Missing,
    Invalid,
}

#[derive(Debug, Clone, Default)]
pub struct SkillRoutingMetadata {
    pub card: Option<SkillRoutingCard>,
    pub status: SkillRoutingStatus,
    pub error: Option<String>,
}

pub fn read_skill_routing(path: &Path) -> SkillRoutingMetadata {
    match fs::read_to_string(path) {
        Ok(content) => skill_routing_metadata(&content),
        Err(error) => SkillRoutingMetadata {
            status: SkillRoutingStatus::Invalid,
            error: Some(format!("failed to read Skill entry: {error}")),
            ..SkillRoutingMetadata::default()
        },
    }
}

pub fn skill_routing_metadata(content: &str) -> SkillRoutingMetadata {
    match parse_skill_routing(content) {
        Ok(card) => SkillRoutingMetadata {
            card: Some(card),
            status: SkillRoutingStatus::Valid,
            error: None,
        },
        Err(SkillRoutingError::Missing) => SkillRoutingMetadata::default(),
        Err(error) => SkillRoutingMetadata {
            status: SkillRoutingStatus::Invalid,
            error: Some(error.to_string()),
            ..SkillRoutingMetadata::default()
        },
    }
}

pub fn parse_skill_routing(content: &str) -> Result<SkillRoutingCard, SkillRoutingError> {
    let frontmatter = read_frontmatter(content).ok_or(SkillRoutingError::Missing)?;
    let routing = frontmatter
        .get("routing")
        .cloned()
        .ok_or(SkillRoutingError::Missing)?;
    let card = serde_yaml::from_value::<SkillRoutingCard>(routing)
        .map_err(|error| SkillRoutingError::Invalid(error.to_string()))?
        .normalize();
    card.validate()?;
    Ok(card)
}

pub fn read_frontmatter(content: &str) -> Option<serde_yaml::Value> {
    let mut lines = content.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    let mut closed = false;
    let yaml = lines
        .take_while(|line| {
            let keep = !matches!(line.trim(), "---" | "...");
            closed |= !keep;
            keep
        })
        .collect::<Vec<_>>()
        .join("\n");
    closed.then(|| serde_yaml::from_str(&yaml).ok()).flatten()
}

pub fn validate_skill_description(content: &str) -> Result<(), String> {
    let frontmatter = read_frontmatter(content)
        .ok_or_else(|| "SKILL.md must contain valid YAML frontmatter".to_string())?;
    if frontmatter
        .get("description")
        .and_then(serde_yaml::Value::as_str)
        .is_some_and(|description| !description.trim().is_empty())
    {
        Ok(())
    } else {
        Err("SKILL.md must define a non-empty top-level description".to_string())
    }
}

pub fn validate_install_skill_description(content: &str) -> Result<(), String> {
    let repaired = repair_legacy_skill_description_content(content);
    validate_skill_description(repaired.as_deref().unwrap_or(content))
}

pub fn repair_legacy_skill_description_content(content: &str) -> Option<String> {
    let frontmatter = read_frontmatter(content)?;
    if frontmatter.get("description").is_some() {
        return None;
    }
    let description = [
        frontmatter.get("summary"),
        frontmatter.get("short-description"),
        frontmatter
            .get("metadata")
            .and_then(|value| value.get("short-description")),
    ]
    .into_iter()
    .flatten()
    .filter_map(serde_yaml::Value::as_str)
    .find(|value| !value.trim().is_empty())?;
    let description = serde_json::to_string(description.trim()).ok()?;
    let (opening, rest) = content.split_once('\n')?;
    let newline = if opening.ends_with('\r') {
        "\r\n"
    } else {
        "\n"
    };
    Some(format!(
        "{opening}\ndescription: {description}{newline}{rest}"
    ))
}

fn normalize_list(values: &mut Vec<String>) {
    let mut normalized = Vec::with_capacity(values.len());
    for value in values.drain(..) {
        let value = value.trim().to_string();
        if !value.is_empty() && !normalized.contains(&value) {
            normalized.push(value);
        }
    }
    *values = normalized;
}

#[derive(Debug, thiserror::Error)]
pub enum SkillRoutingError {
    #[error("Skill routing card is missing")]
    Missing,
    #[error("invalid Skill routing card: {0}")]
    Invalid(String),
}
