use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use serde::Serialize;

use super::registry::PluginDescriptor;
use crate::commands::acp::{MarketSkillDependencyMarker, MarketSkillMarker};

const MAX_DEPENDENCIES: usize = 64;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyStatus {
    pub id: String,
    pub slug: String,
    pub required_version: String,
    pub installed_version: Option<String>,
    pub ready: bool,
    pub reason: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyReport {
    pub verified: bool,
    pub issues: Vec<String>,
    pub dependencies: Vec<DependencyStatus>,
}

struct Context<'a> {
    plugins: &'a BTreeMap<String, PluginDescriptor>,
    skills: &'a HashMap<i64, String>,
    visiting: BTreeSet<i64>,
    completed: BTreeMap<i64, bool>,
    checked: usize,
    report: DependencyReport,
}

pub(crate) fn inspect(
    plugin: &PluginDescriptor,
    plugins: &BTreeMap<String, PluginDescriptor>,
) -> DependencyReport {
    let skills = crate::commands::acp::installed_market_skill_versions();
    inspect_with(plugin, (plugins, &skills))
}

fn inspect_with(
    plugin: &PluginDescriptor,
    input: (&BTreeMap<String, PluginDescriptor>, &HashMap<i64, String>),
) -> DependencyReport {
    let mut context = Context {
        plugins: input.0,
        skills: input.1,
        visiting: BTreeSet::new(),
        completed: BTreeMap::new(),
        checked: 0,
        report: DependencyReport {
            verified: true,
            issues: Vec::new(),
            dependencies: Vec::new(),
        },
    };
    visit(plugin, &mut context);
    context.report.issues.sort();
    context.report.issues.dedup();
    context.report
}

pub(crate) fn apply(plugins: &mut BTreeMap<String, PluginDescriptor>) {
    let snapshot = plugins.clone();
    let skills = crate::commands::acp::installed_market_skill_versions();
    for plugin in plugins.values_mut() {
        let report = inspect_with(plugin, (&snapshot, &skills));
        if plugin.manifest.schema_version >= 2 || report.verified {
            plugin.available &= report.verified && report.issues.is_empty();
        }
    }
}

fn visit(plugin: &PluginDescriptor, context: &mut Context<'_>) -> bool {
    if let Some(ready) = context.completed.get(&plugin.market_skill_id) {
        return *ready;
    }
    if context.checked >= MAX_DEPENDENCIES || !context.visiting.insert(plugin.market_skill_id) {
        context
            .report
            .issues
            .push("dependency_cycle_or_limit".into());
        return false;
    }
    context.checked += 1;
    let Some((marker, plugin_ids)) = snapshot(plugin) else {
        context.report.verified = false;
        if plugin.manifest.schema_version >= 2 {
            context
                .report
                .issues
                .push("dependency_snapshot_missing".into());
        }
        context.visiting.remove(&plugin.market_skill_id);
        return plugin.manifest.schema_version < 2;
    };
    let mut ready = true;
    for dependency in &marker.dependencies {
        ready &= check_dependency(
            dependency,
            plugin_ids.contains(&dependency.skill_id),
            context,
        );
    }
    context.visiting.remove(&plugin.market_skill_id);
    context.completed.insert(plugin.market_skill_id, ready);
    ready
}

fn snapshot(plugin: &PluginDescriptor) -> Option<(MarketSkillMarker, BTreeSet<i64>)> {
    if !Path::new(&plugin.install_root).is_dir() {
        return None;
    }
    let pointer = super::recovery::read_pointer(Path::new(&plugin.install_root))?;
    let marker = pointer.market?;
    if marker.schema_version != 3
        || marker.source != "iyw_skill_market"
        || marker.package_type != "plugin"
        || marker.skill_id != plugin.market_skill_id
        || marker.slug != plugin.slug
        || marker.installed_version != plugin.version
        || pointer.version != plugin.version
        || marker.content_sha256 != pointer.content_sha256
        || marker.object_sha256 != pointer.object_sha256
        || marker.dependencies.len() > MAX_DEPENDENCIES
    {
        return None;
    }
    let ids = marker
        .dependencies
        .iter()
        .map(|dependency| dependency.skill_id)
        .collect::<BTreeSet<_>>();
    if ids.len() != marker.dependencies.len()
        || pointer
            .plugin_dependencies
            .iter()
            .any(|id| !ids.contains(id))
    {
        return None;
    }
    Some((marker, pointer.plugin_dependencies.into_iter().collect()))
}

fn check_dependency(
    dependency: &MarketSkillDependencyMarker,
    is_plugin: bool,
    context: &mut Context<'_>,
) -> bool {
    if !is_plugin && crate::commands::office_tools::is_officecli_skill_id(&dependency.slug) {
        let ready=crate::commands::office_tools::managed_office_dependency_available(&dependency.slug);
        if !ready {context.report.issues.push("dependency_unavailable".into());}
        context.report.dependencies.push(DependencyStatus{id:dependency.skill_id.to_string(),slug:dependency.slug.clone(),required_version:dependency.version.clone(),installed_version:None,ready,reason:(!ready).then(||"managed_office_dependency_unavailable".into())});
        return ready;
    }
    let plugin = context
        .plugins
        .values()
        .find(|plugin| plugin.market_skill_id == dependency.skill_id)
        .cloned();
    let installed_version = if is_plugin {
        plugin.as_ref().map(|value| value.version.clone())
    } else {
        context.skills.get(&dependency.skill_id).cloned()
    };
    let mut ready = installed_version.as_deref() == Some(dependency.version.as_str());
    if is_plugin && ready {
        ready = plugin.as_ref().is_some_and(|plugin| {
            plugin.available && plugin.slug == dependency.slug && visit(plugin, context)
        });
    }
    if !ready {
        context.report.issues.push("dependency_unavailable".into());
    }
    context.report.dependencies.push(DependencyStatus {
        id: dependency.skill_id.to_string(),
        slug: dependency.slug.clone(),
        required_version: dependency.version.clone(),
        installed_version,
        ready,
        reason: (!ready).then(|| "dependency_unavailable".into()),
    });
    ready
}
