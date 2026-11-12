use super::dependencies::DependencyReport;
use super::registry::PluginDescriptor;
use super::types::PluginInvokeError;
use std::collections::BTreeMap;

pub(crate) fn inspect(
    plugin: &PluginDescriptor,
    plugins: &BTreeMap<String, PluginDescriptor>,
    context: (&str, &str),
) -> DependencyReport {
    let mut report = super::dependencies::inspect(plugin, plugins);
    for dependency in &mut report.dependencies {
        let Some(target) = plugins.values().find(|plugin| {
            plugin.market_skill_id.to_string() == dependency.id
                && plugin.manifest.schema_version >= 2
        }) else {
            continue;
        };
        if !dependency.ready {
            continue;
        }
        let configuration_missing = target
            .manifest
            .components
            .iter()
            .filter(|component| component.kind == "connector")
            .any(|component| {
                super::setup_store::environment(target, (context.0, &component.key)).is_err()
            });
        let reason = if configuration_missing {
            Some("dependency_configuration_required")
        } else if !super::policy::permission_granted(target, context.0) {
            Some("dependency_permission_required")
        } else if target
            .manifest
            .components
            .iter()
            .filter(|component| component.kind == "connector")
            .any(|component| {
                !super::policy::activation_enabled(target, (&component.key, context.0, context.1))
            })
        {
            Some("dependency_disabled")
        } else {
            None
        };
        if let Some(reason) = reason {
            dependency.ready = false;
            dependency.reason = Some(reason.into());
            report.issues.push(reason.into());
        }
    }
    report.issues.sort();
    report.issues.dedup();
    report
}

pub(crate) fn require(
    plugin: &PluginDescriptor,
    plugins: &BTreeMap<String, PluginDescriptor>,
    context: (&str, &str),
) -> Result<(), PluginInvokeError> {
    let report = inspect(plugin, plugins, context);
    if report.issues.is_empty() && (report.verified || plugin.manifest.schema_version < 2) {
        return Ok(());
    }
    Err(PluginInvokeError::before_effect(
        "plugin_dependency_unavailable",
        "Plugin dependencies are not authorized or enabled for this workspace",
    ))
}
