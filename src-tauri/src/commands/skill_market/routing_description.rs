use base64::Engine;

use crate::acp::skill_routing::{
    parse_skill_routing, validate_skill_description, SkillRoutingError,
};
use crate::app_error::AppCommandError;

use super::types::{SkillMarketUploadFile, SkillPackageType};

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
    validate_skill_description(content).map_err(|error| {
        routing_error(
            &file.path,
            error,
            "Use top-level description; summary and short-description do not replace it",
        )
    })?;
    match parse_skill_routing(content) {
        Ok(_) | Err(SkillRoutingError::Missing) => Ok(()),
        Err(error) => Err(routing_error(
            &file.path,
            error.to_string(),
            "Include concise capability, coreTriggers, exclusions, aliases, and invocation fields",
        )),
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
