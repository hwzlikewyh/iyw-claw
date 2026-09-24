use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sea_orm::{ColumnTrait, Condition, EntityTrait, QueryFilter};
use sha2::{Digest, Sha256};

use super::ConnectionManager;
use crate::acp::error::AcpError;
use crate::acp::prepared_session::PrepareSessionRequest;

const METADATA_PREFIXES: [&str; 6] = [
    "managed_skills.",
    "user_memory.",
    "agent_storage.",
    "feedback.",
    "question.",
    "session_info.",
];

impl ConnectionManager {
    pub(super) async fn prepared_environment_fingerprint(
        &self,
        request: &PrepareSessionRequest,
        environment: &mut BTreeMap<String, String>,
    ) -> Result<String, AcpError> {
        project_fingerprint_environment(request, environment);
        let file_request = request.clone();
        let file_environment = environment.clone();
        let file_snapshot = tokio::task::spawn_blocking(move || {
            let native = crate::commands::acp::fingerprint_config(
                file_request.agent_type,
                &file_environment,
            );
            let mcp = crate::commands::mcp::read_servers_for_agent_type(file_request.agent_type)
                .map_err(|error| AcpError::protocol(error.to_string()))?;
            Ok::<_, AcpError>((native, mcp, instruction_revisions(&file_request)))
        });
        let (policy, settings, files) = tokio::join!(
            crate::acp::runtime_host_policy::resolve(request.agent_type),
            self.prepared_settings_revision(request),
            file_snapshot,
        );
        let settings = settings?;
        let (native, mcp, files) = files.map_err(|error| {
            AcpError::protocol(format!("Prepared file snapshot failed: {error}"))
        })??;
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
        use crate::db::entities::app_metadata::{Column, Entity};

        let db = self.preparation_db()?;
        let mut values = BTreeMap::new();
        let relevant = METADATA_PREFIXES
            .iter()
            .fold(Condition::any(), |condition, prefix| {
                condition.add(Column::Key.starts_with(*prefix))
            });
        let metadata = Entity::find()
            .filter(Column::DeletedAt.is_null())
            .filter(relevant)
            .all(&db.conn)
            .await
            .map_err(|error| AcpError::protocol(error.to_string()))?;
        for value in metadata {
            // SQL LIKE 的大小写及下划线语义不同，保留原来的精确前缀判断。
            if METADATA_PREFIXES
                .iter()
                .any(|prefix| value.key.starts_with(prefix))
            {
                values.insert(value.key, value.value);
            }
        }
        Ok(values)
    }
}

fn project_fingerprint_environment(
    request: &PrepareSessionRequest,
    environment: &mut BTreeMap<String, String>,
) {
    crate::acp::provider_overlay::apply_preferred_model_runtime_env(
        request.agent_type,
        environment,
        request
            .preferred_config_values
            .get("model")
            .map(String::as_str),
    );
    crate::acp::trusted_agents::restrict_configured_runtime_env(request.agent_type, environment);
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
