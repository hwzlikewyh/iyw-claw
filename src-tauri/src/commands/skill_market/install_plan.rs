use std::collections::{BTreeMap, BTreeSet};

use crate::app_error::AppCommandError;
use crate::commands::acp::{MarketSkillDependencyMarker, MarketSkillMarker};
use crate::models::AgentType;

use super::types::{parse_id, SkillInstallPlan, SkillInstallPlanItem, SkillPackageType};

pub(super) const MAX_PACKAGE_BYTES: u64 = 30 * 1024 * 1024;
const MAX_INSTALL_PLAN_ITEMS: usize = 64;
const MAX_INSTALL_PLAN_BYTES: u64 = 256 * 1024 * 1024;

pub(super) fn validate_install_plan(
    plan: &SkillInstallPlan,
    requested_id: i64,
    requested_version: &str,
) -> Result<(), AppCommandError> {
    if parse_id(&plan.root_skill_id)? != requested_id
        || plan.root_version != requested_version
        || plan.items.is_empty()
        || plan.items.len() > MAX_INSTALL_PLAN_ITEMS
    {
        return Err(AppCommandError::configuration_invalid(
            "Skill install plan does not match the requested release",
        ));
    }
    validate_plan_slug(&plan.root_slug)?;
    let mut by_id = BTreeMap::new();
    let mut slugs = BTreeSet::new();
    let mut total_bytes = 0_u64;
    for item in &plan.items {
        validate_install_plan_item(item, &by_id)?;
        let skill_id = parse_id(&item.skill_id)?;
        if by_id
            .insert(skill_id, (item.slug.as_str(), item.version.as_str()))
            .is_some()
            || !slugs.insert(item.slug.as_str())
        {
            return Err(AppCommandError::configuration_invalid(
                "Skill install plan contains duplicate packages",
            ));
        }
        total_bytes = total_bytes
            .checked_add(item.download.package_size)
            .filter(|value| *value <= MAX_INSTALL_PLAN_BYTES)
            .ok_or_else(|| {
                AppCommandError::invalid_input("Skill install plan is too large to install safely")
            })?;
    }
    let root = plan.items.last().expect("non-empty install plan");
    if parse_id(&root.skill_id)? != requested_id
        || root.slug != plan.root_slug
        || root.version != requested_version
    {
        return Err(AppCommandError::configuration_invalid(
            "Skill install plan root package is invalid",
        ));
    }
    validate_plan_closure(&plan.items, requested_id)?;
    Ok(())
}

fn validate_plan_closure(
    items: &[SkillInstallPlanItem],
    root_skill_id: i64,
) -> Result<(), AppCommandError> {
    let by_id = items
        .iter()
        .map(|item| Ok((parse_id(&item.skill_id)?, item)))
        .collect::<Result<BTreeMap<_, _>, AppCommandError>>()?;
    let mut reachable = BTreeSet::from([root_skill_id]);
    let mut pending = vec![root_skill_id];
    while let Some(skill_id) = pending.pop() {
        let item = by_id.get(&skill_id).ok_or_else(|| {
            AppCommandError::configuration_invalid("Skill install plan root package is missing")
        })?;
        for dependency in &item.dependencies {
            let dependency_id = parse_id(&dependency.skill_id)?;
            if reachable.insert(dependency_id) {
                pending.push(dependency_id);
            }
        }
    }
    if reachable.len() != by_id.len() {
        return Err(AppCommandError::configuration_invalid(
            "Skill install plan contains packages unrelated to the requested release",
        ));
    }
    Ok(())
}

fn validate_install_plan_item(
    item: &SkillInstallPlanItem,
    previous: &BTreeMap<i64, (&str, &str)>,
) -> Result<(), AppCommandError> {
    let _ = parse_id(&item.skill_id)?;
    let slug = validate_plan_slug(&item.slug)?;
    if slug != item.slug || normalized_version(&item.version)? != item.version {
        return Err(AppCommandError::configuration_invalid(
            "Skill install plan contains an invalid package identity",
        ));
    }
    if item.download.version != item.version {
        return Err(AppCommandError::configuration_invalid(
            "Skill install plan download metadata is inconsistent",
        ));
    }
    ensure_download_artifact_ready(&item.download)?;
    if item.download.package_size > MAX_PACKAGE_BYTES {
        return Err(AppCommandError::configuration_invalid(
            "Skill install plan download metadata is inconsistent",
        ));
    }
    let expected_type = if item.dependencies.is_empty() {
        SkillPackageType::Skill
    } else {
        SkillPackageType::Expert
    };
    if item.package_type != expected_type
        || item.display_name.trim().is_empty()
        || !matches!(item.visibility.as_str(), "public" | "private")
        || !matches!(item.publisher_type.as_str(), "official" | "user")
    {
        return Err(AppCommandError::configuration_invalid(
            "Skill install plan package metadata is invalid",
        ));
    }
    validate_direct_dependencies(item, previous)
}

/// A release without a real artifact exposes placeholder metadata: the
/// per-file upload path writes the raw file size into `packageSize` and
/// `objectSha256` does not describe any real ZIP. Treat missing/empty
/// digest or a zero size as "artifact not ready" instead of a network
/// glitch so the client never repeats three identical downloads.
fn ensure_download_artifact_ready(
    download: &super::types::SkillDownloadInfo,
) -> Result<(), AppCommandError> {
    if download.package_size == 0
        || download.object_sha256.trim().is_empty()
        || download.content_sha256.trim().is_empty()
    {
        return Err(AppCommandError::artifact_not_ready(
            "The Skill artifact is not ready yet; this version cannot be installed",
        )
        .with_detail(format!(
            "artifact metadata incomplete: package_size={}, object_sha256={}, content_sha256={}",
            download.package_size,
            digest_presence(&download.object_sha256),
            digest_presence(&download.content_sha256),
        )));
    }
    Ok(())
}

fn digest_presence(value: &str) -> &'static str {
    if value.trim().is_empty() {
        "missing"
    } else {
        "present"
    }
}

fn validate_direct_dependencies(
    item: &SkillInstallPlanItem,
    previous: &BTreeMap<i64, (&str, &str)>,
) -> Result<(), AppCommandError> {
    let mut direct = BTreeSet::new();
    for dependency in &item.dependencies {
        let dependency_id = parse_id(&dependency.skill_id)?;
        if !direct.insert(dependency_id)
            || normalized_version(&dependency.version)? != dependency.version
            || previous.get(&dependency_id)
                != Some(&(dependency.slug.as_str(), dependency.version.as_str()))
        {
            return Err(AppCommandError::configuration_invalid(format!(
                "Skill install plan has an unresolved dependency for {}@{}",
                item.slug, item.version
            )));
        }
    }
    Ok(())
}

fn normalized_version(value: &str) -> Result<String, AppCommandError> {
    semver::Version::parse(value.trim())
        .map(|version| version.to_string())
        .map_err(|error| {
            AppCommandError::configuration_invalid("Skill install plan has an invalid version")
                .with_detail(error.to_string())
        })
}

fn validate_plan_slug(value: &str) -> Result<String, AppCommandError> {
    crate::commands::acp::validate_skill_id(value).map_err(|error| {
        AppCommandError::configuration_invalid("Skill install plan has an invalid slug")
            .with_detail(error.to_string())
    })
}

pub(super) fn market_marker(
    item: &SkillInstallPlanItem,
    object_sha256: String,
    root_skill_id: i64,
    agent_types: Vec<AgentType>,
) -> Result<MarketSkillMarker, AppCommandError> {
    let dependencies = item
        .dependencies
        .iter()
        .map(|dependency| {
            Ok(MarketSkillDependencyMarker {
                skill_id: parse_id(&dependency.skill_id)?,
                slug: dependency.slug.clone(),
                version: dependency.version.clone(),
            })
        })
        .collect::<Result<Vec<_>, AppCommandError>>()?;
    Ok(MarketSkillMarker {
        schema_version: 3,
        source: "iyw_skill_market".to_string(),
        skill_id: parse_id(&item.skill_id)?,
        slug: item.slug.clone(),
        installed_version: item.version.clone(),
        content_sha256: item.download.content_sha256.clone(),
        object_sha256,
        visibility: item.visibility.clone(),
        publisher_type: item.publisher_type.clone(),
        package_type: match item.package_type {
            SkillPackageType::Skill => "skill",
            SkillPackageType::Expert => "expert",
        }
        .to_string(),
        agent_types: Some(agent_types.clone()),
        target_references: BTreeMap::from([(root_skill_id, agent_types)]),
        dependencies,
        installed_at: chrono::Utc::now().to_rfc3339(),
    })
}
