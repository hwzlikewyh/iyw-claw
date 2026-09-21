use anyhow::{bail, Context, Result};
use chrono::Utc;
use fs2::FileExt;
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};

use crate::client::FusionClient;
use crate::download::{emit, ensure_cached};
use crate::inventory;
use crate::model::{
    EnvironmentAction, EnvironmentSnapshot, InstalledComponent, PreparedComponent, PreparedState,
    ResolveRequest,
};
use crate::paths::{platform, slash_relative, Layout};
use crate::{activation, archive};

pub fn prepare(app_version: &str) -> Result<String> {
    let layout = Layout::resolve()?;
    layout.ensure()?;
    let _lock = environment_lock(&layout)?;
    let current = inventory::load_current(&layout)?;
    let (target, arch, platform) = platform();
    let request = ResolveRequest {
        schema_version: 1,
        installation_id: installation_id(&layout)?,
        client_version: app_version.to_string(),
        pc_version: app_version.to_string(),
        channel: "stable",
        runtime: "desktop",
        target: target.clone(),
        arch: arch.clone(),
        inventory: inventory::healthy_inventory(&layout, current.as_ref()),
    };
    let client = FusionClient::new()?;
    let plan = client.resolve(&request)?;
    validate_plan(&plan, app_version, &target, &arch)?;
    let transaction = uuid::Uuid::new_v4().simple().to_string();
    let state = prepare_actions(
        &layout,
        &client,
        &transaction,
        &platform,
        plan,
        current.as_ref(),
    )?;
    let transaction_dir = layout.transaction_dir(&transaction)?;
    inventory::write_json(&transaction_dir.join("prepared.json"), &state)?;
    fs::write(layout.pending_transaction(), &transaction)?;
    emit("environment", "prepared", 0, 0);
    Ok(transaction)
}

fn prepare_actions(
    layout: &Layout,
    client: &FusionClient,
    transaction: &str,
    platform: &str,
    plan: crate::model::EnvironmentPlan,
    current: Option<&EnvironmentSnapshot>,
) -> Result<PreparedState> {
    let mut components = Vec::new();
    let mut seen = BTreeSet::new();
    for action in &plan.actions {
        if !seen.insert(action.component_id.clone()) {
            bail!("environment plan contains a duplicate component")
        }
        match prepare_action(layout, client, transaction, platform, action, current) {
            Ok(component) => components.push(component),
            Err(error) if action.optional => match healthy_current(layout, current, action) {
                Some(component) => {
                    eprintln!(
                        "optional component {} kept after update failure: {error:#}",
                        action.component_id
                    );
                    components.push(PreparedComponent {
                        component,
                        staged: false,
                    });
                }
                None => eprintln!(
                    "optional component {} skipped: {error:#}",
                    action.component_id
                ),
            },
            Err(error) => return Err(error),
        }
    }
    Ok(PreparedState {
        transaction_id: transaction.to_string(),
        plan_id: plan.plan_id,
        pc_version: plan.pc_version,
        catalog_revision: plan.catalog_revision,
        binding_revision: plan.binding_revision,
        target: plan.target,
        arch: plan.arch,
        created_at: Utc::now().to_rfc3339(),
        components,
    })
}

fn prepare_action(
    layout: &Layout,
    client: &FusionClient,
    transaction: &str,
    platform: &str,
    action: &EnvironmentAction,
    current: Option<&EnvironmentSnapshot>,
) -> Result<PreparedComponent> {
    validate_action(action)?;
    if action.action == "keep" {
        let component = current
            .and_then(|snapshot| {
                snapshot
                    .components
                    .iter()
                    .find(|item| item.component_id == action.component_id)
            })
            .context("Fusion returned keep without a healthy local component")?
            .clone();
        inventory::verify_component(layout, &component)?;
        if component.version != action.version
            || component.artifact_sha256 != action.artifact.sha256
        {
            bail!("Fusion keep action does not match the installed component")
        }
        return Ok(PreparedComponent {
            component,
            staged: false,
        });
    }
    let cache = layout.runtime.join("cache/downloads");
    let archive = ensure_cached(
        client.http(),
        &cache,
        &action.artifact,
        &action.component_id,
    )?;
    let staged = layout
        .transaction_dir(transaction)?
        .join("components")
        .join(crate::paths::safe_segment(
            &action.component_id,
            "component",
        )?);
    if staged.exists() {
        fs::remove_dir_all(&staged)?;
    }
    archive::unpack(
        &archive,
        &staged,
        &action.artifact.package_kind,
        &action.artifact.file_name,
    )?;
    let (files, entrypoints) =
        inventory::describe_component(layout, &action.component_id, &staged)?;
    let final_dir = layout.component_dir(&action.component_id, &action.version, platform)?;
    Ok(PreparedComponent {
        component: InstalledComponent {
            component_id: action.component_id.clone(),
            component_kind: action.component_kind.clone(),
            optional: action.optional,
            version: action.version.clone(),
            version_id: action.artifact.version_id.clone(),
            artifact_id: action.artifact.artifact_id.clone(),
            package_kind: action.artifact.package_kind.clone(),
            artifact_sha256: action.artifact.sha256.clone(),
            relative_path: slash_relative(&layout.root, &final_dir)?,
            entrypoints,
            files,
        },
        staged: true,
    })
}

fn healthy_current(
    layout: &Layout,
    current: Option<&EnvironmentSnapshot>,
    action: &EnvironmentAction,
) -> Option<InstalledComponent> {
    current?
        .components
        .iter()
        .find(|item| item.component_id == action.component_id)
        .filter(|item| inventory::verify_component(layout, item).is_ok())
        .cloned()
}

fn validate_action(action: &EnvironmentAction) -> Result<()> {
    crate::paths::safe_segment(&action.component_id, "component")?;
    crate::paths::safe_segment(&action.version, "version")?;
    if !matches!(action.action.as_str(), "keep" | "install" | "update") {
        bail!("unsupported environment action")
    }
    if !matches!(
        action.artifact.package_kind.as_str(),
        "binary"
            | "zip"
            | "tar_gz"
            | "tar_xz"
            | "npm_runtime_bundle_zip"
            | "npm_runtime_bundle_tar_gz"
            | "uvx_runtime_bundle_zip"
            | "uvx_runtime_bundle_tar_gz"
    ) {
        bail!("unsupported environment package kind")
    }
    if action
        .artifact
        .version_id
        .parse::<u64>()
        .unwrap_or_default()
        == 0
        || action
            .artifact
            .artifact_id
            .parse::<u64>()
            .unwrap_or_default()
            == 0
        || action.artifact.size_bytes == 0
        || action.artifact.sha256.len() != 64
        || action.artifact.sha256 != action.artifact.sha256.to_ascii_lowercase()
        || !action
            .artifact
            .sha256
            .bytes()
            .all(|value| value.is_ascii_hexdigit())
    {
        bail!("environment artifact integrity metadata is invalid")
    }
    Ok(())
}

fn validate_plan(
    plan: &crate::model::EnvironmentPlan,
    app_version: &str,
    target: &str,
    arch: &str,
) -> Result<()> {
    if plan.plan_id.parse::<u64>().unwrap_or_default() == 0
        || plan.catalog_revision == 0
        || plan.binding_revision == 0
        || plan.pc_version != app_version
        || plan.target != target
        || plan.arch != arch
        || plan.actions.is_empty()
    {
        bail!("Fusion environment plan does not match this installation")
    }
    Ok(())
}

pub fn commit(transaction: Option<&str>) -> Result<()> {
    let layout = Layout::resolve()?;
    layout.ensure()?;
    let _lock = environment_lock(&layout)?;
    let transaction = transaction.map(str::to_string).unwrap_or_else(|| {
        fs::read_to_string(layout.pending_transaction())
            .unwrap_or_default()
            .trim()
            .to_string()
    });
    if transaction.is_empty() {
        bail!("no prepared environment transaction is available")
    }
    crate::paths::safe_segment(&transaction, "transaction")?;
    let state = inventory::read_prepared(&layout, &transaction)?;
    if state.transaction_id != transaction {
        bail!("prepared environment transaction identity does not match")
    }
    let snapshot = EnvironmentSnapshot {
        schema_version: 1,
        generation: uuid::Uuid::new_v4().simple().to_string(),
        pc_version: state.pc_version.clone(),
        catalog_revision: state.catalog_revision,
        binding_revision: state.binding_revision,
        target: state.target.clone(),
        arch: state.arch.clone(),
        created_at: Utc::now().to_rfc3339(),
        components: state
            .components
            .iter()
            .map(|value| value.component.clone())
            .collect(),
    };
    activation::commit(&layout, &state, &snapshot)?;
    let _ = fs::remove_file(layout.pending_transaction());
    emit("environment", "committed", 0, 0);
    Ok(())
}

pub fn diagnose() -> Result<u8> {
    let layout = Layout::resolve()?;
    let Some(snapshot) = inventory::load_current(&layout)? else {
        println!("{}", serde_json::json!({"status":"missing"}));
        return Ok(10);
    };
    let failures = snapshot
        .components
        .iter()
        .filter(|component| inventory::verify_component(&layout, component).is_err())
        .map(|component| component.component_id.clone())
        .collect::<Vec<_>>();
    let status = if failures.is_empty() {
        "healthy"
    } else {
        "corrupt"
    };
    println!(
        "{}",
        serde_json::json!({"status":status,"components":failures})
    );
    Ok(if failures.is_empty() { 0 } else { 10 })
}

fn environment_lock(layout: &Layout) -> Result<File> {
    let path = layout.runtime.join(".environment-writer.lock");
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(path)?;
    file.try_lock_exclusive()
        .context("another environment operation is running")?;
    Ok(file)
}

fn installation_id(layout: &Layout) -> Result<String> {
    let path = layout.inventory.join("installation-id");
    if let Ok(value) = fs::read_to_string(&path) {
        let value = value.trim();
        if value.len() >= 8 && value.len() <= 128 {
            return Ok(value.to_string());
        }
    }
    let value = uuid::Uuid::new_v4().to_string();
    fs::write(path, &value)?;
    Ok(value)
}
