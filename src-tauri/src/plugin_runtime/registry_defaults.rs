use crate::app_error::AppCommandError;
use crate::commands::skill_market::SkillPluginManifest;
use crate::db::entities::{plugin_activation_policy, plugin_permission_grant};
use crate::db::service::plugin_installation_service::PluginInstallationRecord;

// 已校验安装默认授予清单权限。旧待授权记录按同一规则读取，显式停用继续保留。
pub(super) fn apply(
    record: &mut PluginInstallationRecord,
    manifest: &SkillPluginManifest,
) -> Result<(), AppCommandError> {
    let permissions = serde_json::to_string(&manifest.permissions.clone().unwrap_or_default())
        .map_err(|error| AppCommandError::configuration_invalid(error.to_string()))?;
    grant_permissions(record, permissions);
    enable_defaults(record, manifest);
    Ok(())
}

fn grant_permissions(record: &mut PluginInstallationRecord, permissions: String) {
    let installation = &record.installation;
    for grant in &mut record.permission_grants {
        if grant.grant_state == "revoked" {
            continue;
        }
        grant.grant_state = "granted".into();
        grant.permissions_digest = installation.permissions_digest.clone();
        grant.granted_permissions_json = permissions.clone();
    }
    if record
        .permission_grants
        .iter()
        .any(|grant| grant.scope == "global")
    {
        return;
    }
    record
        .permission_grants
        .push(plugin_permission_grant::Model {
            id: 0,
            plugin_slug: installation.slug.clone(),
            scope: "global".into(),
            workspace_key: String::new(),
            permissions_digest: installation.permissions_digest.clone(),
            granted_permissions_json: permissions,
            grant_state: "granted".into(),
            granted_at: Some(installation.updated_at),
            updated_at: installation.updated_at,
        });
}

fn enable_defaults(record: &mut PluginInstallationRecord, manifest: &SkillPluginManifest) {
    record.activations.retain(|entry| {
        !matches!(
            entry.policy_source.as_str(),
            "install_default" | "recovery_pending"
        )
    });
    for connector in manifest
        .components
        .iter()
        .filter(|entry| entry.kind == "connector")
    {
        if record.activations.iter().any(|entry| {
            entry.component_key == connector.key
                && entry.scope == "global"
                && entry.agent_type.is_empty()
        }) {
            continue;
        }
        let routing_mode = connector
            .config
            .as_ref()
            .and_then(|config| config["routing"]["mode"].as_str())
            .unwrap_or("host_gateway");
        record.activations.push(plugin_activation_policy::Model {
            id: 0,
            plugin_slug: record.installation.slug.clone(),
            component_key: connector.key.clone(),
            scope: "global".into(),
            workspace_key: String::new(),
            agent_type: String::new(),
            requested_enabled: true,
            routing_mode: routing_mode.into(),
            policy_source: "install_default".into(),
            updated_at: record.installation.updated_at,
        });
    }
}
