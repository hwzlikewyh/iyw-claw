use anyhow::{bail, Result};

use crate::model::{EnvironmentAction, EnvironmentPlan, EnvironmentSnapshot};

pub const REQUIRED: &[&str] = &["node", "git", "uv", "chromix", "agent-browser"];
const OPTIONAL: &[&str] = &[
    "officecli",
    "agent-reach",
    "open-computer-use",
    "environment-maintainer",
];

pub struct PlanExpectation<'a> {
    pub app_version: &'a str,
    pub target: &'a str,
    pub arch: &'a str,
}

pub fn validate_action(action: &EnvironmentAction) -> Result<()> {
    if !REQUIRED.contains(&action.component_id.as_str())
        && !OPTIONAL.contains(&action.component_id.as_str())
    {
        bail!("unknown environment component")
    }
    crate::paths::safe_segment(&action.component_id, "component")?;
    crate::paths::safe_segment(&action.version, "version")?;
    if !matches!(action.action.as_str(), "keep" | "install" | "update") {
        bail!("unsupported environment action")
    }
    if !supported_package(&action.artifact.package_kind) {
        bail!("unsupported environment package kind")
    }
    let artifact = &action.artifact;
    let valid_ids = artifact.version_id.parse::<u64>().unwrap_or_default() > 0
        && artifact.artifact_id.parse::<u64>().unwrap_or_default() > 0;
    let valid_hash = artifact.sha256.len() == 64
        && artifact.sha256 == artifact.sha256.to_ascii_lowercase()
        && artifact
            .sha256
            .bytes()
            .all(|value| value.is_ascii_hexdigit());
    if !valid_ids || artifact.size_bytes == 0 || !valid_hash {
        bail!("environment artifact integrity metadata is invalid")
    }
    Ok(())
}

pub fn validate_platform() -> Result<()> {
    if !matches!(
        (std::env::consts::OS, std::env::consts::ARCH),
        ("windows", "x86_64") | ("macos" | "linux", "x86_64" | "aarch64")
    ) {
        return Err(crate::failure::Failure::permanent(
            "UNSUPPORTED",
            format!(
                "当前环境暂不支持：{} / {}，不会下载其他平台的组件",
                std::env::consts::OS,
                std::env::consts::ARCH
            ),
        )
        .into());
    }
    Ok(())
}

pub fn validate_plan(plan: &EnvironmentPlan, expected: &PlanExpectation<'_>) -> Result<()> {
    let valid_identity = plan.plan_id.parse::<u64>().unwrap_or_default() > 0
        && plan.catalog_revision > 0
        && plan.binding_revision > 0;
    let valid_platform = plan.pc_version == expected.app_version
        && plan.target == expected.target
        && plan.arch == expected.arch;
    if !valid_identity || !valid_platform {
        bail!("Fusion environment plan does not match this installation")
    }
    let mut ids = std::collections::BTreeSet::new();
    for action in &plan.actions {
        validate_action(action)?;
        if !ids.insert(action.component_id.as_str()) {
            bail!("Fusion environment plan contains duplicate components")
        }
    }
    Ok(())
}

pub fn validate_prepared(state: &crate::model::PreparedState) -> Result<()> {
    let (target, arch, _) = crate::paths::platform();
    if state.target != target || state.arch != arch {
        bail!("prepared environment platform mismatch")
    }
    let mut ids = std::collections::BTreeSet::new();
    for entry in &state.components {
        let id = entry.component.component_id.as_str();
        if !ids.insert(id) || (!REQUIRED.contains(&id) && !OPTIONAL.contains(&id)) {
            bail!("duplicate or unknown prepared component")
        }
    }
    Ok(())
}

pub fn validate_snapshot(snapshot: &EnvironmentSnapshot) -> Result<()> {
    let (target, arch, _) = crate::paths::platform();
    if snapshot.schema_version != 1 || snapshot.target != target || snapshot.arch != arch {
        bail!("环境清单版本或系统架构不匹配，请运行环境修复")
    }
    if snapshot.generation.is_empty()
        || snapshot.pc_version.is_empty()
        || snapshot.catalog_revision == 0
        || snapshot.binding_revision == 0
    {
        bail!("环境清单缺少版本标识，请运行环境修复")
    }
    let mut ids = std::collections::BTreeSet::new();
    for component in &snapshot.components {
        let id = component.component_id.as_str();
        if !ids.insert(id) || (!REQUIRED.contains(&id) && !OPTIONAL.contains(&id)) {
            bail!("环境清单包含重复或未知组件，请运行环境修复")
        }
    }
    if let Some(selected) = &snapshot.selected_components {
        let selected_ids = selected.iter().map(String::as_str).collect();
        if ids != selected_ids || selected.len() != ids.len() {
            bail!("环境清单与安装时选定的组件不一致，请运行环境修复")
        }
    } else if !REQUIRED.iter().all(|id| ids.contains(id)) {
        bail!("环境清单缺少必需组件，请运行环境修复")
    }
    Ok(())
}

fn supported_package(kind: &str) -> bool {
    matches!(
        kind,
        "binary"
            | "zip"
            | "tar_gz"
            | "tar_xz"
            | "npm_runtime_bundle_zip"
            | "npm_runtime_bundle_tar_gz"
            | "uvx_runtime_bundle_zip"
            | "uvx_runtime_bundle_tar_gz"
    )
}
