use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;

use chrono::NaiveDateTime;

use super::{
    central_experts_dir, remove_skill_entry, scoped_skill_dirs, AgentSkillScope, AgentType,
};

const USER_BACKUP_SEPARATOR: &str = ".user-backup-";
const USER_BACKUP_TIMESTAMP_FORMAT: &str = "%Y%m%d-%H%M%S";
const LINK_BACKUP_PREFIX: &str = ".iyw-claw-link-backup-";

pub(crate) fn is_legacy_skill_backup_name(name: &str) -> bool {
    if let Some(suffix) = name.strip_prefix(LINK_BACKUP_PREFIX) {
        return uuid::Uuid::parse_str(suffix).is_ok();
    }
    let Some((skill_id, timestamp)) = name.rsplit_once(USER_BACKUP_SEPARATOR) else {
        return false;
    };
    !skill_id.is_empty()
        && NaiveDateTime::parse_from_str(timestamp, USER_BACKUP_TIMESTAMP_FORMAT).is_ok_and(
            |parsed| parsed.format(USER_BACKUP_TIMESTAMP_FORMAT).to_string() == timestamp,
        )
}

pub(super) fn cleanup_legacy_skill_backups(agents: &[AgentType]) -> Vec<String> {
    let mut roots = BTreeSet::from([
        central_experts_dir(),
        crate::system_skills::repository_dir(),
    ]);
    let mut errors = Vec::new();
    for agent in agents {
        match scoped_skill_dirs(*agent, AgentSkillScope::Global, None) {
            Ok(dirs) => roots.extend(dirs),
            Err(error) => errors.push(format!(
                "resolve legacy Skill backup roots for {agent}: {error}"
            )),
        }
    }
    for root in roots {
        if let Err(error) = cleanup_root(&root, &mut errors) {
            record_cleanup_error(&root, error, &mut errors);
        }
    }
    errors
}

fn cleanup_root(root: &Path, errors: &mut Vec<String>) -> io::Result<()> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        if !is_legacy_skill_backup_name(&entry.file_name().to_string_lossy()) {
            continue;
        }
        let path = entry.path();
        match remove_skill_entry(&path) {
            Ok(()) => tracing::info!(
                target: "system_skills",
                path = %path.display(),
                "removed legacy Skill backup from discovery directory"
            ),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => record_cleanup_error(&path, error, errors),
        }
    }
    Ok(())
}

fn record_cleanup_error(path: &Path, error: io::Error, errors: &mut Vec<String>) {
    tracing::warn!(
        target: "system_skills",
        path = %path.display(),
        error = %error,
        "failed to remove legacy Skill backup from discovery directory"
    );
    errors.push(format!(
        "remove legacy Skill backup {}: {error}",
        path.display()
    ));
}
