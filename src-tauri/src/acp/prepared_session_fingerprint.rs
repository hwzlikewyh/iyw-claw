use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sea_orm::EntityTrait;
use sha2::{Digest, Sha256};

use super::ConnectionManager;
use crate::acp::error::AcpError;
use crate::acp::prepared_session::PrepareSessionRequest;

impl ConnectionManager {
    pub(super) async fn prepared_environment_fingerprint(
        &self,
        request: &PrepareSessionRequest,
        environment: &mut BTreeMap<String, String>,
    ) -> Result<String, AcpError> {
        crate::acp::provider_overlay::apply_preferred_model_runtime_env(
            request.agent_type,
            environment,
            request
                .preferred_config_values
                .get("model")
                .map(String::as_str),
        );
        crate::acp::trusted_agents::restrict_configured_runtime_env(
            request.agent_type,
            environment,
        );
        let policy = crate::acp::runtime_host_policy::resolve(request.agent_type).await;
        let native = crate::commands::acp::fingerprint_config(request.agent_type, environment);
        let mcp = crate::commands::mcp::read_servers_for_agent_type(request.agent_type)
            .map_err(|error| AcpError::protocol(error.to_string()))?;
        let settings = self.prepared_settings_revision(request).await?;
        let files = instruction_revisions(request);
        let bytes = serde_json::to_vec(&(
            environment,
            native,
            mcp,
            settings,
            files,
            policy.revision,
            policy.capabilities.bits(),
        ))
        .map_err(|error| AcpError::protocol(error.to_string()))?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }

    async fn prepared_settings_revision(
        &self,
        request: &PrepareSessionRequest,
    ) -> Result<String, AcpError> {
        let db = self.preparation_db()?;
        let workspace = crate::commands::skill_inventory::workspace_key_for_agent(
            request.agent_type,
            request.working_dir.as_deref(),
        );
        let policies = crate::db::service::skill_activation_policy_service::list_for_workspace(
            &db.conn, &workspace,
        )
        .await
        .map_err(|error| AcpError::protocol(error.to_string()))?;
        let agent = serde_json::to_string(&request.agent_type)
            .map_err(|error| AcpError::protocol(error.to_string()))?;
        let mut values = BTreeMap::new();
        for policy in policies
            .into_iter()
            .filter(|policy| policy.agent_type == agent)
        {
            values.insert(
                format!("skill:{}", policy.id),
                format!("{}:{}", policy.requested_enabled, policy.updated_at),
            );
        }
        values.extend(self.prepared_metadata_revision().await?);
        serde_json::to_string(&values).map_err(|error| AcpError::protocol(error.to_string()))
    }

    async fn prepared_metadata_revision(&self) -> Result<BTreeMap<String, String>, AcpError> {
        let db = self.preparation_db()?;
        let mut values = BTreeMap::new();
        let metadata = crate::db::entities::app_metadata::Entity::find()
            .all(&db.conn)
            .await
            .map_err(|error| AcpError::protocol(error.to_string()))?;
        for value in metadata
            .into_iter()
            .filter(|value| value.deleted_at.is_none())
        {
            if [
                "managed_skills.",
                "user_memory.",
                "agent_storage.",
                "feedback.",
                "question.",
                "session_info.",
            ]
            .iter()
            .any(|prefix| value.key.starts_with(prefix))
            {
                values.insert(value.key, value.value);
            }
        }
        Ok(values)
    }
}

fn instruction_revisions(request: &PrepareSessionRequest) -> Vec<(PathBuf, Option<(u64, u128)>)> {
    let mut files = Vec::new();
    if let Some(cwd) = request.working_dir.as_deref() {
        for directory in Path::new(cwd).ancestors() {
            for name in [
                "AGENTS.md",
                "CLAUDE.md",
                ".codex/config.toml",
                ".claude/settings.json",
            ] {
                files.push(directory.join(name));
            }
        }
    }
    if let Some(paths) = crate::acp::agent_storage::AgentStoragePaths::active() {
        let profile = paths.profile(request.agent_type);
        files.extend([
            profile.root.join("AGENTS.md"),
            profile.root.join("CLAUDE.md"),
        ]);
    }
    files.extend(skill_instruction_files(request));
    files.sort();
    files.dedup();
    files
        .into_iter()
        .map(|path| {
            let revision = std::fs::metadata(&path).ok().and_then(|metadata| {
                let modified = metadata
                    .modified()
                    .ok()?
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()?;
                Some((metadata.len(), modified.as_nanos()))
            });
            (path, revision)
        })
        .collect()
}

fn skill_instruction_files(request: &PrepareSessionRequest) -> Vec<PathBuf> {
    use crate::acp::types::AgentSkillScope;
    let mut roots = vec![crate::commands::acp::shared_skills_dir()];
    for scope in [AgentSkillScope::Global, AgentSkillScope::Project] {
        if let Ok(paths) = crate::commands::acp::scoped_skill_dirs(
            request.agent_type,
            scope,
            request.working_dir.as_deref(),
        ) {
            roots.extend(paths);
        }
    }
    let mut files = roots.clone();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.push(path.join("SKILL.md"));
            } else if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
            {
                files.push(path);
            }
        }
    }
    files
}
