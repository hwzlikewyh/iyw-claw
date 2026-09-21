use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::{managed_path, verify_file, ManagedComponent};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedEnvironmentStatusReport {
    pub phase: String,
    pub components: Vec<ManagedComponentStatus>,
    pub offline: bool,
    pub writer_busy: bool,
    pub pending_activations: Vec<serde_json::Value>,
    pub manifest_generation: u64,
    pub digest: String,
    pub migrated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedComponentStatus {
    pub component_id: String,
    pub component_kind: String,
    pub version: String,
    pub installed: bool,
    pub active: bool,
    pub phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

pub fn init_status_report() -> ManagedEnvironmentStatusReport {
    let root = crate::paths::iyw_claw_user_dir();
    let raw = std::fs::read(root.join("inventory/environment-current.json")).unwrap_or_default();
    let parsed = super::snapshot::load(&root);
    let error = parsed.as_ref().err().cloned();
    let snapshot = parsed.ok();
    let mut components = snapshot
        .as_ref()
        .map(|value| {
            value
                .components
                .iter()
                .map(|component| component_status(&root, component))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    components.push(builtin_agent_status());
    if let Some(error) = error {
        components.push(ManagedComponentStatus {
            component_id: "environment".into(),
            component_kind: "runtime".into(),
            version: String::new(),
            installed: false,
            active: false,
            phase: "degraded".into(),
            last_error: Some(error),
        });
    }
    let ready = components.iter().all(|item| item.active);
    ManagedEnvironmentStatusReport {
        phase: if ready { "ready" } else { "degraded" }.to_string(),
        components,
        offline: false,
        writer_busy: false,
        pending_activations: Vec::new(),
        manifest_generation: snapshot.map_or(0, |value| value.catalog_revision),
        digest: format!("{:x}", Sha256::digest(&raw)),
        migrated: false,
    }
}

fn builtin_agent_status() -> ManagedComponentStatus {
    let error = crate::internal_xinghe_worker::resolve_library().err();
    let active = error.is_none();
    ManagedComponentStatus {
        component_id: "builtin-agent".into(),
        component_kind: "agent".into(),
        version: crate::internal_xinghe_worker::RUNTIME_VERSION.into(),
        installed: active,
        active,
        phase: if active { "ready" } else { "degraded" }.into(),
        last_error: error
            .map(|message| format!("内置 Agent 资源不完整，请重新运行应用安装包：{message}")),
    }
}

fn component_status(root: &Path, component: &ManagedComponent) -> ManagedComponentStatus {
    let active = !component.files.is_empty()
        && required_entrypoints(&component.component_id)
            .iter()
            .all(|name| component_entrypoint(root, component, name).is_some());
    ManagedComponentStatus {
        component_id: component.component_id.clone(),
        component_kind: component.component_kind.clone(),
        version: component.version.clone(),
        installed: active,
        active,
        phase: if active { "ready" } else { "degraded" }.to_string(),
        last_error: (!active).then(|| "Managed environment requires repair".to_string()),
    }
}

pub(super) fn required_entrypoints(component: &str) -> &'static [&'static str] {
    match component {
        "node" => &["node", "npm", "npx"],
        "git" => &["git"],
        "uv" => &["uv", "uvx"],
        "chromix" => &["chromix"],
        "agent-browser" => &["agent-browser"],
        "officecli" => &["officecli"],
        "agent-reach" => &["agent-reach"],
        "open-computer-use" => &["open-computer-use"],
        "memory-embedding-bge-small-zh-v1.5" => &["model"],
        _ => &[],
    }
}

fn component_entrypoint(root: &Path, component: &ManagedComponent, name: &str) -> Option<PathBuf> {
    let relative = component.entrypoints.get(name)?;
    let component_root = super::snapshot::component_path(root, &component.relative_path)?;
    let candidate = managed_path(&component_root, relative)?;
    let record = component.files.iter().find(|file| file.path == *relative)?;
    verify_file(&component_root, &candidate, record).then_some(candidate)
}
