use std::collections::BTreeSet;

use crate::acp::skill_package::PackageFile;
use crate::app_error::AppCommandError;

use super::plugin_manifest::{
    invalid_plugin, NativeManifest, PortableConnector, PortableManifest, PortableSkill,
    CODEX_MANIFEST, MAX_MANIFEST_BYTES,
};
use super::plugin_types::{SkillPluginBinding, SkillPluginComponent, SkillPluginManifest};

pub(super) fn build_manifest(
    files: &[PackageFile],
    portable: PortableManifest,
    servers: BTreeSet<String>,
) -> Result<SkillPluginManifest, AppCommandError> {
    let (skills, bindings, paths) = build_skills(files, portable.components.skills)?;
    let connectors = build_connectors(portable.components.connectors, &servers)?;
    validate_codex_skills(files, &paths)?;
    let mut components = skills;
    components.extend(connectors);
    if components.is_empty() {
        return Err(invalid_plugin("Plugin has no runtime components"));
    }
    Ok(SkillPluginManifest {
        schema_version: portable.schema_version,
        name: portable.name,
        version: portable.version,
        targets: portable.targets,
        components,
        bindings,
    })
}

fn build_skills(
    files: &[PackageFile],
    values: Vec<PortableSkill>,
) -> Result<
    (
        Vec<SkillPluginComponent>,
        Vec<SkillPluginBinding>,
        BTreeSet<String>,
    ),
    AppCommandError,
> {
    let actual = plugin_skill_paths(files)?;
    let mut keys = BTreeSet::new();
    let mut declared = BTreeSet::new();
    let mut components = Vec::with_capacity(values.len());
    let mut bindings = Vec::new();
    for value in values {
        validate_portable_skill(&value, &mut keys, &mut declared, &actual)?;
        append_bindings(&value, &mut bindings)?;
        components.push(SkillPluginComponent {
            kind: "skill".to_string(),
            key: value.key,
            path: value.path,
            server_key: String::new(),
        });
    }
    if declared != actual {
        return Err(invalid_plugin(
            "Declared Skills do not match skills/*/SKILL.md",
        ));
    }
    Ok((components, bindings, actual))
}

fn validate_portable_skill(
    value: &PortableSkill,
    keys: &mut BTreeSet<String>,
    paths: &mut BTreeSet<String>,
    actual: &BTreeSet<String>,
) -> Result<(), AppCommandError> {
    if !valid_key(&value.key)
        || !valid_skill_path(&value.path)
        || !keys.insert(value.key.clone())
        || !paths.insert(value.path.clone())
        || !actual.contains(&value.path)
    {
        return Err(invalid_plugin("Plugin contains an invalid Skill component"));
    }
    Ok(())
}

fn append_bindings(
    skill: &PortableSkill,
    bindings: &mut Vec<SkillPluginBinding>,
) -> Result<(), AppCommandError> {
    let mut seen = BTreeSet::new();
    for connector_key in &skill.requires_connectors {
        if !valid_key(connector_key) || !seen.insert(connector_key) {
            return Err(invalid_plugin(
                "Plugin contains an invalid connector binding",
            ));
        }
        bindings.push(SkillPluginBinding {
            skill_key: skill.key.clone(),
            connector_key: connector_key.clone(),
        });
    }
    Ok(())
}

fn build_connectors(
    values: Vec<PortableConnector>,
    servers: &BTreeSet<String>,
) -> Result<Vec<SkillPluginComponent>, AppCommandError> {
    let mut keys = BTreeSet::new();
    let mut server_keys = BTreeSet::new();
    let mut result = Vec::with_capacity(values.len());
    for value in values {
        if !valid_key(&value.key)
            || !valid_key(&value.server_key)
            || !keys.insert(value.key.clone())
            || !server_keys.insert(value.server_key.clone())
        {
            return Err(invalid_plugin(
                "Plugin contains an invalid connector component",
            ));
        }
        result.push(SkillPluginComponent {
            kind: "connector".to_string(),
            key: value.key,
            path: String::new(),
            server_key: value.server_key,
        });
    }
    if &server_keys != servers {
        return Err(invalid_plugin(
            "Connector server keys do not match .mcp.json",
        ));
    }
    Ok(result)
}

fn validate_codex_skills(
    files: &[PackageFile],
    skill_paths: &BTreeSet<String>,
) -> Result<(), AppCommandError> {
    let codex: NativeManifest = super::plugin_manifest::parse_document(files, CODEX_MANIFEST)?;
    let valid = if skill_paths.is_empty() {
        codex.skills.is_empty()
    } else {
        codex.skills == "./skills/"
    };
    if !valid {
        return Err(invalid_plugin("Codex skills reference is invalid"));
    }
    Ok(())
}

fn plugin_skill_paths(files: &[PackageFile]) -> Result<BTreeSet<String>, AppCommandError> {
    let mut result = BTreeSet::new();
    for file in files {
        let path = file.path.to_string_lossy().replace('\\', "/");
        let parts = path.split('/').collect::<Vec<_>>();
        if parts.len() != 3 || parts[0] != "skills" || parts[2] != "SKILL.md" {
            continue;
        }
        let content = std::str::from_utf8(&file.bytes).ok();
        if file.bytes.len() > MAX_MANIFEST_BYTES
            || content.is_none_or(|value| value.trim().is_empty())
        {
            return Err(invalid_plugin(
                "Plugin Skill entry is empty, oversized, or invalid UTF-8",
            ));
        }
        result.insert(parts[..2].join("/"));
    }
    Ok(result)
}

pub(super) fn validate_summary_components(
    values: &[SkillPluginComponent],
) -> Result<(BTreeSet<String>, BTreeSet<String>), AppCommandError> {
    let mut skills = BTreeSet::new();
    let mut connectors = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let mut servers = BTreeSet::new();
    for value in values {
        let valid = match value.kind.as_str() {
            "skill" => {
                value.server_key.is_empty()
                    && valid_skill_path(&value.path)
                    && skills.insert(value.key.clone())
                    && paths.insert(value.path.clone())
            }
            "connector" => {
                value.path.is_empty()
                    && valid_key(&value.server_key)
                    && connectors.insert(value.key.clone())
                    && servers.insert(value.server_key.clone())
            }
            _ => false,
        };
        if !valid_key(&value.key) || !valid {
            return Err(invalid_plugin("Plugin install plan components are invalid"));
        }
    }
    Ok((skills, connectors))
}

pub(super) fn validate_summary_bindings(
    values: &[SkillPluginBinding],
    skills: &BTreeSet<String>,
    connectors: &BTreeSet<String>,
) -> Result<(), AppCommandError> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !skills.contains(&value.skill_key)
            || !connectors.contains(&value.connector_key)
            || !seen.insert((value.skill_key.as_str(), value.connector_key.as_str()))
        {
            return Err(invalid_plugin("Plugin install plan bindings are invalid"));
        }
    }
    Ok(())
}

fn valid_skill_path(value: &str) -> bool {
    let parts = value.split('/').collect::<Vec<_>>();
    parts.len() == 2 && parts[0] == "skills" && valid_key(parts[1])
}

pub(super) fn valid_key(value: &str) -> bool {
    value.len() <= 128
        && !value.is_empty()
        && value.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}
