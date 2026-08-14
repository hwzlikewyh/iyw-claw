use base64::Engine;

use crate::app_error::AppCommandError;

use super::types::{SkillMarketUploadFile, SkillPackageType};

pub(crate) const ROUTING_DESCRIPTION_MAX_CHARS: usize = 240;

pub(super) fn validate_routing_descriptions(
    files: &[SkillMarketUploadFile],
    package_type: SkillPackageType,
) -> Result<(), AppCommandError> {
    for file in files
        .iter()
        .filter(|file| is_skill_entry(&file.path, package_type))
    {
        validate_skill_entry(file)?;
    }
    Ok(())
}

fn is_skill_entry(path: &str, package_type: SkillPackageType) -> bool {
    let normalized = path.replace('\\', "/");
    match package_type {
        SkillPackageType::Skill | SkillPackageType::Expert => normalized == "SKILL.md",
        SkillPackageType::Plugin => {
            let parts = normalized.split('/').collect::<Vec<_>>();
            parts.len() == 3 && parts[0] == "skills" && parts[2] == "SKILL.md"
        }
    }
}

fn validate_skill_entry(file: &SkillMarketUploadFile) -> Result<(), AppCommandError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&file.content_base64)
        .map_err(|error| routing_error(&file.path, "is not valid Base64", error.to_string()))?;
    let content = std::str::from_utf8(&bytes)
        .map_err(|error| routing_error(&file.path, "must be UTF-8", error.to_string()))?;
    let frontmatter = read_frontmatter(content).ok_or_else(|| {
        routing_error(
            &file.path,
            "must define a non-empty routing description",
            "Include capability, core triggers, exclusions, aliases, and invocation when relevant",
        )
    })?;
    let description = frontmatter_description(&frontmatter).ok_or_else(|| {
        routing_error(
            &file.path,
            "must define a non-empty routing description",
            "Use short-description or top-level description in the Skill frontmatter",
        )
    })?;
    let chars = description.chars().count();
    if chars > ROUTING_DESCRIPTION_MAX_CHARS {
        return Err(routing_error(
            &file.path,
            &format!(
                "description has {chars} characters; maximum is {ROUTING_DESCRIPTION_MAX_CHARS}"
            ),
            "Shorten the routing sentence without dropping required routing facts",
        ));
    }
    let card = frontmatter.get("routing").ok_or_else(|| {
        routing_error(
            &file.path,
            "must define a routing card",
            "Include capability, coreTriggers, exclusions, aliases, and invocation",
        )
    })?;
    validate_routing_card(&file.path, card)?;
    let card_chars = routing_card_chars(card);
    if card_chars > ROUTING_DESCRIPTION_MAX_CHARS {
        return Err(routing_error(
            &file.path,
            &format!("routing card has {card_chars} characters; maximum is {ROUTING_DESCRIPTION_MAX_CHARS}"),
            "Shorten routing fields without dropping exclusions or invocation",
        ));
    }
    Ok(())
}

fn validate_routing_card(path: &str, value: &serde_yaml::Value) -> Result<(), AppCommandError> {
    let Some(map) = value.as_mapping() else {
        return Err(routing_error(
            path,
            "routing card must be a mapping",
            "Use a YAML object",
        ));
    };
    for field in ["capability", "invocation"] {
        let Some(value) = map_field(map, field) else {
            return Err(routing_error(
                path,
                &format!("routing card is missing {field}"),
                "Add the required routing field",
            ));
        };
        if value.as_str().is_none_or(|text| text.trim().is_empty()) {
            return Err(routing_error(
                path,
                &format!("routing card {field} must be non-empty"),
                "Use concise routing text",
            ));
        }
    }
    for field in ["coreTriggers", "exclusions", "aliases"] {
        let Some(value) = map_field(map, field) else {
            return Err(routing_error(
                path,
                &format!("routing card is missing {field}"),
                "Add the required routing list",
            ));
        };
        if !value.is_sequence() {
            return Err(routing_error(
                path,
                &format!("routing card {field} must be a list"),
                "Use a YAML list, even when it is empty",
            ));
        }
    }
    Ok(())
}

fn map_field<'a>(map: &'a serde_yaml::Mapping, field: &str) -> Option<&'a serde_yaml::Value> {
    let snake = match field {
        "coreTriggers" => "core_triggers",
        _ => field,
    };
    map.get(field).or_else(|| map.get(snake))
}

fn read_frontmatter(content: &str) -> Option<serde_yaml::Value> {
    let mut lines = content.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    let yaml = lines
        .take_while(|line| !matches!(line.trim(), "---" | "..."))
        .collect::<Vec<_>>()
        .join("\n");
    serde_yaml::from_str(&yaml).ok()
}

fn frontmatter_description(value: &serde_yaml::Value) -> Option<String> {
    value
        .get("short-description")
        .or_else(|| value.get("description"))?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn routing_card_chars(value: &serde_yaml::Value) -> usize {
    match value {
        serde_yaml::Value::String(text) => text.chars().count(),
        serde_yaml::Value::Sequence(values) => values.iter().map(routing_card_chars).sum(),
        serde_yaml::Value::Mapping(map) => map.values().map(routing_card_chars).sum(),
        _ => 0,
    }
}

fn routing_error(
    path: &str,
    message: impl Into<String>,
    detail: impl Into<String>,
) -> AppCommandError {
    AppCommandError::invalid_input(format!(
        "Skill routing description in '{path}' {}",
        message.into()
    ))
    .with_detail(detail.into())
}
