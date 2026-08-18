use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::acp::error::AcpError;
use crate::acp::skill_tree_hash::hash_skill_path;
use crate::acp::types::{AgentSkillItem, AgentSkillLayout, AgentSkillScope};
use crate::commands::acp::{
    build_skill_item, disabled_skills_dir, read_market_skill_marker, scoped_skill_dirs,
    set_shared_skill_read_only, set_skill_read_only, shared_skills_dir, skill_storage_spec,
    SkillStorageKind,
};
use crate::models::agent::AgentType;

use super::types::{SkillInventoryOwnership, SkillObservation, SkillObservedLocation};

#[derive(Default)]
struct RootSpec {
    scope: Option<AgentSkillScope>,
    agent_kinds: BTreeMap<AgentType, SkillStorageKind>,
    scan_directory_source: bool,
}

struct RawLocation {
    item: AgentSkillItem,
    root: PathBuf,
    canonical_path: PathBuf,
    agent_types: Vec<AgentType>,
    enabled: bool,
    projection_source: Option<String>,
}

pub(super) fn scan_observations(
    workspace_path: Option<&str>,
) -> Result<Vec<SkillObservation>, AcpError> {
    let roots = collect_roots(workspace_path);
    let mut raw = Vec::new();
    for (root, spec) in roots {
        scan_root(&root, &spec, true, &mut raw)?;
        scan_root(&disabled_skills_dir(&root), &spec, false, &mut raw)?;
    }
    Ok(merge_aliases(raw))
}

fn collect_roots(workspace_path: Option<&str>) -> BTreeMap<PathBuf, RootSpec> {
    let mut roots = BTreeMap::new();
    for agent_type in crate::commands::managed_skills::supported_skill_agent_types() {
        let Some(spec) = skill_storage_spec(agent_type) else {
            continue;
        };
        let kind = spec.kind;
        for root in spec.global_dirs {
            add_root(&mut roots, root, AgentSkillScope::Global, kind, agent_type);
        }
        add_project_roots(&mut roots, workspace_path, kind, agent_type);
    }
    let shared = roots.entry(shared_skills_dir()).or_default();
    shared.scope = Some(AgentSkillScope::Global);
    shared.scan_directory_source = true;
    roots
}

fn add_project_roots(
    roots: &mut BTreeMap<PathBuf, RootSpec>,
    workspace_path: Option<&str>,
    kind: SkillStorageKind,
    agent_type: AgentType,
) {
    let Some(workspace) = workspace_path.filter(|value| !value.trim().is_empty()) else {
        return;
    };
    let Ok(project_dirs) = scoped_skill_dirs(agent_type, AgentSkillScope::Project, Some(workspace))
    else {
        return;
    };
    for project_dir in project_dirs {
        add_root(
            roots,
            project_dir,
            AgentSkillScope::Project,
            kind,
            agent_type,
        );
    }
}

fn add_root(
    roots: &mut BTreeMap<PathBuf, RootSpec>,
    root: PathBuf,
    scope: AgentSkillScope,
    kind: SkillStorageKind,
    agent_type: AgentType,
) {
    let entry = roots.entry(root).or_default();
    entry.scope = Some(scope);
    entry.agent_kinds.insert(agent_type, kind);
}

fn scan_root(
    scan_dir: &Path,
    spec: &RootSpec,
    enabled: bool,
    output: &mut Vec<RawLocation>,
) -> Result<(), AcpError> {
    if !scan_dir.exists() {
        return Ok(());
    }
    let entries = fs::read_dir(scan_dir)
        .map_err(|error| AcpError::protocol(format!("failed to scan Skill root: {error}")))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let scope = spec.scope.unwrap_or(AgentSkillScope::Global);
        let mut matched = false;
        for (agent_type, kind) in &spec.agent_kinds {
            let Some((id, layout)) = identify_skill(&path, *kind) else {
                continue;
            };
            matched = true;
            let mut item = build_skill_item(id, scope, layout, path.clone(), enabled);
            set_skill_read_only(*agent_type, &mut item);
            output.push(RawLocation {
                item,
                root: scan_dir.to_path_buf(),
                canonical_path: fs::canonicalize(&path).unwrap_or_else(|_| path.clone()),
                agent_types: vec![*agent_type],
                enabled,
                projection_source: managed_copy_source(&path),
            });
        }
        if !matched && spec.scan_directory_source {
            push_source_location(scan_dir, path, scope, enabled, output);
        }
    }
    Ok(())
}

fn push_source_location(
    scan_dir: &Path,
    path: PathBuf,
    scope: AgentSkillScope,
    enabled: bool,
    output: &mut Vec<RawLocation>,
) {
    let Some((id, layout)) = identify_skill(&path, SkillStorageKind::SkillDirectoryOnly) else {
        return;
    };
    let mut item = build_skill_item(id, scope, layout, path.clone(), enabled);
    set_shared_skill_read_only(&mut item);
    output.push(RawLocation {
        item,
        root: scan_dir.to_path_buf(),
        canonical_path: fs::canonicalize(&path).unwrap_or(path.clone()),
        agent_types: Vec::new(),
        enabled,
        projection_source: managed_copy_source(&path),
    });
}

fn identify_skill(path: &Path, kind: SkillStorageKind) -> Option<(String, AgentSkillLayout)> {
    if path.is_dir() && path.join("SKILL.md").is_file() {
        return path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|id| (id.to_string(), AgentSkillLayout::SkillDirectory));
    }
    if matches!(kind, SkillStorageKind::SkillDirectoryOrMarkdownFile)
        && path.is_file()
        && path.extension()?.eq_ignore_ascii_case("md")
    {
        return path
            .file_stem()
            .and_then(|name| name.to_str())
            .map(|id| (id.to_string(), AgentSkillLayout::MarkdownFile));
    }
    None
}

fn managed_copy_source(path: &Path) -> Option<String> {
    let raw = fs::read_to_string(path.join(".iyw-claw-managed-copy.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    value.get("source_path")?.as_str().map(ToOwned::to_owned)
}

fn merge_aliases(raw: Vec<RawLocation>) -> Vec<SkillObservation> {
    let central = canonical_key(&shared_skills_dir());
    let mut by_path: BTreeMap<String, SkillObservation> = BTreeMap::new();
    for location in raw {
        let path_key = canonical_key(&location.canonical_path);
        let scope_key = match location.item.scope {
            AgentSkillScope::Global => "global",
            AgentSkillScope::Project => "project",
        };
        let key = format!("{scope_key}:{path_key}");
        let market = read_market_skill_marker(&location.canonical_path);
        let ownership = classify_ownership(&location.item, &path_key, &central, market.as_ref());
        let observed = observed_location(&location);
        if let Some(existing) = by_path.get_mut(&key) {
            merge_location(&mut existing.locations, observed);
            existing.read_only |= location.item.read_only;
            continue;
        }
        let hash = hash_skill_path(location.item.layout, &location.canonical_path);
        let market_content_matches = market.as_ref().and_then(|marker| {
            hash.as_ref()
                .ok()
                .map(|value| value.eq_ignore_ascii_case(&marker.content_sha256))
        });
        by_path.insert(
            key,
            SkillObservation {
                skill_id: location.item.id,
                name: location.item.name,
                description: location.item.description,
                scope: location.item.scope,
                layout: location.item.layout,
                canonical_path: display_path(&location.canonical_path),
                content_tree_hash: hash.as_ref().ok().cloned(),
                hash_error: hash.err(),
                ownership,
                read_only: location.item.read_only,
                market_skill_id: location.item.market_skill_id,
                installed_version: location.item.installed_version,
                market_content_sha256: location.item.market_content_sha256,
                market_content_matches,
                plugin_slug: market.as_ref().and_then(|value| value.plugin_slug.clone()),
                plugin_component_key: market
                    .as_ref()
                    .and_then(|value| value.plugin_component_key.clone()),
                dependencies: market
                    .map(|value| {
                        value
                            .dependencies
                            .into_iter()
                            .map(|dependency| dependency.slug)
                            .collect()
                    })
                    .unwrap_or_default(),
                locations: vec![observed],
            },
        );
    }
    by_path.into_values().collect()
}

fn observed_location(location: &RawLocation) -> SkillObservedLocation {
    SkillObservedLocation {
        root: display_path(&location.root),
        path: location.item.path.clone(),
        agent_types: location.agent_types.clone(),
        enabled: location.enabled,
        projection_source: location.projection_source.clone(),
    }
}

fn merge_location(locations: &mut Vec<SkillObservedLocation>, incoming: SkillObservedLocation) {
    if let Some(existing) = locations
        .iter_mut()
        .find(|value| value.path == incoming.path)
    {
        existing.agent_types.extend(incoming.agent_types);
        existing.agent_types.sort();
        existing.agent_types.dedup();
        existing.enabled |= incoming.enabled;
    } else {
        locations.push(incoming);
    }
}

fn classify_ownership(
    item: &AgentSkillItem,
    path_key: &str,
    central: &str,
    market: Option<&crate::commands::acp::MarketSkillMarker>,
) -> SkillInventoryOwnership {
    if market.is_some_and(|value| value.plugin_slug.is_some()) {
        SkillInventoryOwnership::Plugin
    } else if item.market_managed {
        SkillInventoryOwnership::Market
    } else if path_key == central
        || path_key
            .strip_prefix(central)
            .is_some_and(|suffix| suffix.starts_with('/'))
    {
        SkillInventoryOwnership::IywManaged
    } else if item.read_only {
        SkillInventoryOwnership::AgentBuiltin
    } else {
        SkillInventoryOwnership::Manual
    }
}

fn canonical_key(path: &Path) -> String {
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let value = display_path(&canonical);
    if cfg!(windows) {
        value.to_ascii_lowercase()
    } else {
        value
    }
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/")
}
