use super::registry::PluginDescriptor;

pub(crate) fn revision(plugin: &PluginDescriptor, workspace: &str) -> String {
    use sha2::{Digest, Sha256};
    let grants = plugin
        .permission_grants
        .iter()
        .filter(|grant| grant.scope == "global" || grant.workspace_key == workspace)
        .collect::<Vec<_>>();
    let activations = plugin
        .activations
        .iter()
        .filter(|policy| policy.scope == "global" || policy.workspace_key == workspace)
        .collect::<Vec<_>>();
    let configurations=plugin.manifest.components.iter().filter(|component|component.kind=="connector")
        .map(|component|(&component.key,super::setup_store::revision(plugin,(workspace,&component.key)))).collect::<Vec<_>>();
    let bytes = serde_json::json!([plugin.permissions_digest, grants, activations, configurations]).to_string();
    format!("{:x}", Sha256::digest(bytes.as_bytes()))
}

pub(crate) fn current(spec: &super::types::RuntimeLaunchSpec) -> bool {
    super::registry::global_snapshot()
        .and_then(|snapshot| snapshot.plugins.get(&spec.key.plugin_slug).cloned())
        .is_some_and(|plugin| {
            plugin.available
                && plugin.version == spec.key.plugin_version
                && permission_granted(&plugin, &spec.key.workspace_key)
                && scope_revision(&plugin, &spec.key.workspace_key) == spec.permission_revision
        })
}

pub(super) async fn wait_revoked(spec: &super::types::RuntimeLaunchSpec) {
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(250));
    loop {
        interval.tick().await;
        if !authority_current(spec).await {
            return;
        }
    }
}

pub(crate) async fn authority_current(spec: &super::types::RuntimeLaunchSpec) -> bool {
    if !current(spec) {
        return false;
    }
    let Some(conn) = super::global::database() else {
        return false;
    };
    let Ok(scope) = crate::commands::mcp_cli_config::account_scope(&conn).await else {
        return false;
    };
    super::recovery::read_pointer(&spec.install_root)
        .is_some_and(|pointer| pointer.account_scope == scope)
}

pub(crate) fn scope_revision(plugin: &PluginDescriptor, workspace: &str) -> String {
    use sha2::{Digest, Sha256};
    let snapshot = super::registry::global_snapshot();
    let dependencies = snapshot
        .as_ref()
        .map(|snapshot| super::dependencies::inspect(plugin, &snapshot.plugins));
    let mut revisions = vec![revision(plugin, workspace)];
    if let (Some(snapshot), Some(report)) = (snapshot, dependencies) {
        revisions.push(
            serde_json::to_string(&report).unwrap_or_else(|_| "dependency_state_invalid".into()),
        );
        for dependency in report.dependencies {
            if let Some(target) = snapshot
                .plugins
                .values()
                .find(|target| target.market_skill_id.to_string() == dependency.id)
            {
                revisions.push(format!(
                    "{}:{}:{}",
                    dependency.id,
                    target.version,
                    revision(target, workspace)
                ));
            }
        }
    }
    revisions.sort();
    revisions.dedup();
    format!(
        "{:x}",
        Sha256::digest(serde_json::json!(revisions).to_string().as_bytes())
    )
}

pub(crate) fn permission_granted(plugin: &PluginDescriptor, workspace: &str) -> bool {
    plugin
        .permission_grants
        .iter()
        .filter(|grant| {
            grant.scope == "global"
                || (grant.scope == "workspace" && grant.workspace_key == workspace)
        })
        .max_by_key(|grant| grant.scope == "workspace")
        .is_some_and(|grant| {
            grant.permissions_digest == plugin.permissions_digest && grant.grant_state == "granted"
        })
}

pub(crate) fn activation_enabled(plugin: &PluginDescriptor, context: (&str, &str, &str)) -> bool {
    let (connector, workspace, agent) = context;
    plugin
        .activations
        .iter()
        .filter(|policy| policy.component_key == connector && policy.routing_mode == "host_gateway")
        .filter(|policy| policy.agent_type.is_empty() || policy.agent_type == agent)
        .filter(|policy| {
            policy.scope == "global"
                || (policy.scope == "workspace" && policy.workspace_key == workspace)
        })
        .max_by_key(|policy| (policy.scope == "workspace", !policy.agent_type.is_empty()))
        .is_some_and(|policy| policy.requested_enabled)
}
