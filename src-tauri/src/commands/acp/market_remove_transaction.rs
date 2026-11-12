use super::*;

/// 插件外层提交前保留技能撤销点，共用技能只释放所选安装的引用。
pub(crate) struct PluginSkillRemoval {
    snapshot: MarketSkillRemovalSnapshot,
    resolved: bool,
    touched: bool,
}

impl PluginSkillRemoval {
    pub(crate) fn stage(id: i64, slug: &str) -> Result<Self, AcpError> {
        let _guard = shared_skill_mutation_guard();
        Ok(Self {
            snapshot: stage_market_skill_removal_with((id, Some(slug)))?,
            resolved: false,
            touched: false,
        })
    }

    pub(crate) fn remove(&mut self, id: i64, slug: &str) -> Result<(), AcpError> {
        let _guard = shared_skill_mutation_guard();
        self.touched = true;
        remove_owned(&self.snapshot, (id, slug))?;
        release_market_root_references(id, &BTreeSet::new()).map(|_| ())
    }

    pub(crate) fn record(&self) -> Result<serde_json::Value, AcpError> {
        serde_json::to_value(&self.snapshot).map_err(|error| AcpError::protocol(error.to_string()))
    }

    pub(crate) fn recover(record: serde_json::Value) -> Result<(), AcpError> {
        let mut snapshot: MarketSkillRemovalSnapshot = serde_json::from_value(record)
            .map_err(|error| AcpError::protocol(error.to_string()))?;
        let _guard = shared_skill_mutation_guard();
        validate_snapshot(&snapshot)?;
        if !snapshot.transaction_root.exists() {
            let entries = snapshot.entries.iter().collect::<Vec<_>>();
            return restore_market_skill_removal_publications(&entries);
        }
        snapshot.rollback()
    }

    pub(crate) fn rollback(mut self) -> Result<(), AcpError> {
        self.resolved = true;
        let _guard = shared_skill_mutation_guard();
        if self.touched {
            validate_snapshot(&self.snapshot)?;
            self.snapshot.rollback()
        } else {
            self.snapshot.finish();
            Ok(())
        }
    }

    pub(crate) fn finish(mut self) {
        self.resolved = true;
        let _guard = shared_skill_mutation_guard();
        self.snapshot.finish();
    }
}

impl Drop for PluginSkillRemoval {
    fn drop(&mut self) {
        if !self.resolved {
            let _guard = shared_skill_mutation_guard();
            if self.touched {
                if let Err(error) =
                    validate_snapshot(&self.snapshot).and_then(|_| self.snapshot.rollback())
                {
                    tracing::error!(error=%error,"[plugin-remove] 中断移除的技能回退未完成，备份保留");
                }
            } else {
                self.snapshot.finish();
            }
        }
    }
}

fn validate_snapshot(snapshot: &MarketSkillRemovalSnapshot) -> Result<(), AcpError> {
    let root = shared_skills_dir();
    if snapshot.transaction_root.parent() != Some(root.as_path())
        || !snapshot
            .transaction_root
            .file_name()
            .and_then(|value| value.to_str())
            .and_then(|name| name.strip_prefix(".iyw-claw-market-uninstall-"))
            .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
    {
        return Err(AcpError::protocol(
            "Invalid plugin Skill recovery directory",
        ));
    }
    const MAX_RECOVERY_SKILLS: usize = 512;
    if snapshot.entries.len() > MAX_RECOVERY_SKILLS {
        return Err(AcpError::protocol("Too many plugin Skill recovery records"));
    }
    if snapshot.transaction_root.exists() && !is_real_skill_directory(&snapshot.transaction_root) {
        return Err(AcpError::protocol(
            "Plugin Skill recovery directory is not a real directory",
        ));
    }
    for entry in &snapshot.entries {
        let id = validate_skill_id(&entry.previous.skill_id)?;
        if entry.source != root.join(&id) || entry.backup != snapshot.transaction_root.join(&id) {
            return Err(AcpError::protocol("Plugin Skill recovery path mismatch"));
        }
        verify_entry(entry, snapshot.transaction_root.exists())?;
    }
    Ok(())
}

fn verify_entry(entry: &MarketSkillRemovalEntry, backup_present: bool) -> Result<(), AcpError> {
    let marker = entry
        .marker
        .as_ref()
        .ok_or_else(|| AcpError::protocol("Plugin Skill recovery identity is missing"))?;
    let expected = if backup_present {
        &entry.backup
    } else {
        &entry.source
    };
    crate::acp::skill_package::validate_skill_directory(expected, &marker.content_sha256)
        .map_err(|error| AcpError::protocol(error.to_string()))?;
    if path_entry_exists(&entry.source) {
        if !is_real_skill_directory(&entry.source) {
            return Err(AcpError::protocol(
                "Current Skill recovery path is not a real directory",
            ));
        }
        let current = read_market_skill_marker(&entry.source)
            .ok_or_else(|| AcpError::protocol("Current Skill identity changed during recovery"))?;
        if current.skill_id != marker.skill_id
            || current.installed_version != marker.installed_version
            || current.content_sha256 != marker.content_sha256
            || current
                .target_references
                .iter()
                .any(|(id, agents)| marker.target_references.get(id) != Some(agents))
        {
            return Err(AcpError::protocol(
                "Current Skill changed; recovery backup retained",
            ));
        }
        crate::acp::skill_package::validate_skill_directory(&entry.source, &marker.content_sha256)
            .map_err(|error| AcpError::protocol(error.to_string()))?;
    }
    Ok(())
}

fn remove_owned(
    snapshot: &MarketSkillRemovalSnapshot,
    identity: (i64, &str),
) -> Result<(), AcpError> {
    for entry in &snapshot.entries {
        let Some(marker) = read_market_skill_marker(&entry.source) else {
            continue;
        };
        if marker.skill_id == identity.0 && marker.plugin_slug.as_deref() == Some(identity.1) {
            remove_shared_skill_publications_locked(&marker.slug)?;
            remove_skill_entry(&entry.source).map_err(|error| {
                AcpError::protocol(format!(
                    "failed to remove plugin Skill '{}': {error}",
                    marker.slug
                ))
            })?;
        }
    }
    Ok(())
}
