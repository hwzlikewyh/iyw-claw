use super::*;

pub(super) struct Builder<'a> {
    files: &'a [PackageFile],
    pub(super) components: Vec<SkillPluginComponent>,
    pub(super) bindings: Vec<SkillPluginBinding>,
    skills: BTreeSet<String>,
    runtimes: BTreeSet<String>,
    connectors: BTreeSet<String>,
    capabilities: BTreeMap<String, String>,
    apps: BTreeSet<String>,
}

impl<'a> Builder<'a> {
    pub(super) fn new(files: &'a [PackageFile]) -> Self {
        Self {
            files,
            components: vec![],
            bindings: vec![],
            skills: BTreeSet::new(),
            runtimes: BTreeSet::new(),
            connectors: BTreeSet::new(),
            capabilities: BTreeMap::new(),
            apps: BTreeSet::new(),
        }
    }

    fn file(&self, path: &str) -> bool {
        !path.is_empty()
            && !path.starts_with('/')
            && !path.contains("../")
            && find_file(self.files, path).is_some_and(|bytes| !bytes.is_empty())
    }

    pub(super) fn add_runtimes(&mut self, values: Vec<RuntimeV2>) -> Result<(), AppCommandError> {
        for value in values {
            if !valid_key(&value.key)
                || !matches!(value.kind.as_str(), "node" | "python" | "binary")
                || value.dependencies != "bundled"
                || !self.file(&value.entrypoint)
                || !self.runtimes.insert(value.key.clone())
            {
                return Err(invalid_plugin("Plugin v2 runtime is invalid"));
            }
            self.components.push(component(
                "runtime",
                value.key.clone(),
                value.entrypoint.clone(),
                String::new(),
                config(value)?,
            ));
        }
        Ok(())
    }

    pub(super) fn add_connectors(
        &mut self,
        values: Vec<ConnectorV2>,
    ) -> Result<(), AppCommandError> {
        for value in values {
            if !valid_key(&value.key)
                || value.transport != "stdio"
                || value.routing.mode != "host_gateway"
                || !matches!(value.activation.mode.as_str(), "lazy" | "manual")
                || !matches!(
                    value.activation.scope.as_str(),
                    "workspace" | "installation"
                )
                || !self.runtimes.contains(&value.runtime_key)
                || !self.connectors.insert(value.key.clone())
            {
                return Err(invalid_plugin("Plugin v2 connector is invalid"));
            }
            self.components.push(component(
                "connector",
                value.key.clone(),
                String::new(),
                value.key.clone(),
                config(value)?,
            ));
        }
        Ok(())
    }

    pub(super) fn add_skills(&mut self, values: Vec<SkillV2>) -> Result<(), AppCommandError> {
        let actual = self
            .files
            .iter()
            .filter_map(|file| {
                let path = file.path.to_string_lossy().replace('\\', "/");
                let parts: Vec<_> = path.split('/').collect();
                (parts.len() == 3 && parts[0] == "skills" && parts[2] == "SKILL.md")
                    .then(|| parts[..2].join("/"))
            })
            .collect::<BTreeSet<_>>();
        let mut declared = BTreeSet::new();
        for value in values {
            let mut required = BTreeSet::new();
            if !valid_key(&value.key)
                || !actual.contains(&value.path)
                || !declared.insert(value.path.clone())
                || !self.skills.insert(value.key.clone())
                || value
                    .requires_connectors
                    .iter()
                    .any(|key| !self.connectors.contains(key) || !required.insert(key.clone()))
            {
                return Err(invalid_plugin("Plugin v2 Skill is invalid"));
            }
            for connector_key in &value.requires_connectors {
                self.bindings.push(SkillPluginBinding {
                    skill_key: value.key.clone(),
                    connector_key: connector_key.clone(),
                });
            }
            self.components.push(component(
                "skill",
                value.key.clone(),
                value.path.clone(),
                String::new(),
                config(value)?,
            ));
        }
        if declared != actual {
            return Err(invalid_plugin(
                "Plugin v2 Skills do not match package files",
            ));
        }
        Ok(())
    }

    pub(super) fn add_capabilities(
        &mut self,
        slug: &str,
        values: Vec<CapabilityV2>,
    ) -> Result<(), AppCommandError> {
        let mut ids = BTreeSet::new();
        for value in values {
            let schema = find_file(self.files, &value.schema_path)
                .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok());
            if !valid_key(&value.key)
                || !valid_capability_id(slug, &value.id)
                || value.tool_name.is_empty()
                || value.tool_name.len() > 128
                || value.description.trim().is_empty()
                || value.intent_terms.is_empty()
                || !self.connectors.contains(&value.connector_key)
                || !ids.insert(value.id.clone())
                || !value.schema_path.starts_with("contracts/")
                || !value.schema_path.ends_with(".schema.json")
                || schema
                    .as_ref()
                    .and_then(|item| item.get("type"))
                    .and_then(Value::as_str)
                    != Some("object")
                || self
                    .capabilities
                    .insert(value.key.clone(), value.connector_key.clone())
                    .is_some()
            {
                return Err(invalid_plugin("Plugin v2 capability is invalid"));
            }
            self.components.push(component(
                "capability",
                value.key.clone(),
                value.schema_path.clone(),
                value.connector_key.clone(),
                config(value)?,
            ));
        }
        Ok(())
    }

    pub(super) fn add_apps(&mut self, values: Vec<AppV2>) -> Result<(), AppCommandError> {
        for value in values {
            if !valid_key(&value.key)
                || !valid_resource_uri(&value.resource_uri)
                || !self.connectors.contains(&value.connector_key)
                || self.capabilities.get(&value.capability_key) != Some(&value.connector_key)
                || value.display_modes.is_empty()
                || value
                    .display_modes
                    .iter()
                    .any(|mode| !matches!(mode.as_str(), "inline" | "fullscreen"))
                || !self.apps.insert(value.key.clone())
            {
                return Err(invalid_plugin("Plugin v2 app is invalid"));
            }
            self.components.push(component(
                "app",
                value.key.clone(),
                String::new(),
                value.connector_key.clone(),
                config(value)?,
            ));
        }
        Ok(())
    }
}

fn valid_capability_id(slug: &str, value: &str) -> bool {
    let prefix = format!("plugin.{slug}.");
    let Some(route) = value.strip_prefix(&prefix) else {
        return false;
    };
    let parts = route.split('.').collect::<Vec<_>>();
    if parts.len() < 3 {
        return false;
    }
    let Some(version) = parts.last().and_then(|part| part.strip_prefix('v')) else {
        return false;
    };
    !version.is_empty()
        && version.bytes().all(|byte| byte.is_ascii_digit())
        && version.as_bytes()[0] != b'0'
        && parts[..parts.len() - 1].iter().all(|part| valid_key(part))
}

fn valid_resource_uri(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("ui://") else {
        return false;
    };
    if rest
        .chars()
        .any(|character| matches!(character, '?' | '#' | '%' | '\\') || character.is_control())
    {
        return false;
    }
    let Some((authority, path)) = rest.split_once('/') else {
        return false;
    };
    !authority.is_empty()
        && valid_key(authority)
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}
