use super::*;
use std::collections::BTreeMap;

pub(crate) fn plugin_skill_id(plugin: &str, component: &str) -> String {
    let raw = format!("{plugin}\0{component}");
    let component = regex::Regex::new(r"[^a-z0-9]+")
        .expect("constant Skill name pattern")
        .replace_all(&component.to_ascii_lowercase(), "-")
        .trim_matches('-')
        .chars()
        .take(16)
        .collect::<String>();
    format!(
        "iyw-plugin-{component}-{}",
        &hash_bytes(raw.as_bytes())[..24]
    )
}

impl ValidatedSkillPackage {
    /// 仅转换 Agent 投放副本；已验证的原插件包、清单及收据保持原摘要。
    pub(crate) fn plugin_skill_component(
        &self,
        root: &str,
        names: &BTreeMap<String, String>,
    ) -> Result<Self, AppCommandError> {
        let source_name = root.rsplit('/').next().unwrap_or(root);
        let name = names
            .get(source_name)
            .ok_or_else(|| invalid_package("Plugin Skill identity is missing"))?;
        let mut package = self.skill_component(root)?;
        for file in &mut package.files {
            let Ok(text) = std::str::from_utf8(&file.bytes) else {
                continue;
            };
            if text.contains('\0') {
                continue;
            }
            let mut content = rewrite_sibling_paths(text, &file.path, names)?;
            if file.path == Path::new("SKILL.md") {
                content = rename_entry(&content, name)?;
            }
            file.bytes = content.into_bytes();
            file.sha256 = hash_bytes(&file.bytes);
        }
        validate_skill_entry(&package.files)?;
        package.content_sha256 = hash_file_tree(&package.files);
        Ok(package)
    }
}

fn rewrite_sibling_paths(
    text: &str,
    path: &Path,
    names: &BTreeMap<String, String>,
) -> Result<String, AppCommandError> {
    let depth = path
        .parent()
        .map_or(0, |parent| parent.components().count());
    let prefix = "../".repeat(depth + 1);
    let mut content = text.to_string();
    for (old, new) in names {
        let pattern = format!(
            r"(?P<before>^|[^./a-zA-Z0-9_\\-]){}(?P<after>$|[^a-zA-Z0-9_.-])",
            regex::escape(&format!("{prefix}{old}"))
        );
        let pattern =
            regex::Regex::new(&pattern).map_err(|error| invalid_package(error.to_string()))?;
        content = pattern
            .replace_all(&content, format!("${{before}}{prefix}{new}${{after}}"))
            .into_owned();
    }
    Ok(content)
}

fn rename_entry(content: &str, name: &str) -> Result<String, AppCommandError> {
    let repaired = super::super::skill_routing::repair_legacy_skill_description_content(content);
    let content = repaired
        .as_deref()
        .unwrap_or(content)
        .trim_start_matches('\u{feff}');
    let mut metadata = super::super::skill_routing::read_frontmatter(content)
        .ok_or_else(|| invalid_package("Plugin Skill frontmatter is invalid"))?;
    let old = metadata["name"].as_str().unwrap_or_default().to_string();
    metadata["name"] = serde_yaml::Value::String(name.to_string());
    rename_routing(&mut metadata, (&old, name));
    let header =
        serde_yaml::to_string(&metadata).map_err(|error| invalid_package(error.to_string()))?;
    let mut offset = content.split_inclusive('\n').next().map_or(0, str::len);
    for line in content[offset..].split_inclusive('\n') {
        offset += line.len();
        if matches!(line.trim(), "---" | "...") {
            let body = rename_invocations(&content[offset..], (&old, name));
            return Ok(format!("---\n{header}---\n{body}"));
        }
    }
    Err(invalid_package("Plugin Skill frontmatter is not closed"))
}

fn rename_invocations(text: &str, names: (&str, &str)) -> String {
    if names.0.is_empty() {
        return text.to_string();
    }
    let pattern = format!(
        r"(?P<prefix>[$/]){}(?P<after>$|[^a-zA-Z0-9_-])",
        regex::escape(names.0)
    );
    regex::Regex::new(&pattern).map_or_else(
        |_| text.to_string(),
        |pattern| {
            pattern
                .replace_all(text, format!("${{prefix}}{}${{after}}", names.1))
                .into_owned()
        },
    )
}

fn rename_routing(metadata: &mut serde_yaml::Value, names: (&str, &str)) {
    if names.0.is_empty() {
        return;
    }
    let Some(invocation) = metadata
        .get_mut("routing")
        .and_then(|routing| routing.get_mut("invocation"))
    else {
        return;
    };
    let Some(value) = invocation.as_str() else {
        return;
    };
    let pattern = format!(
        r"(?P<prefix>[$/]){}(?P<after>$|[^a-zA-Z0-9_-])",
        regex::escape(names.0)
    );
    if let Ok(pattern) = regex::Regex::new(&pattern) {
        *invocation = serde_yaml::Value::String(
            pattern
                .replace_all(value, format!("${{prefix}}{}${{after}}", names.1))
                .into_owned(),
        );
    }
}
