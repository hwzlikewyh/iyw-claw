use anyhow::{bail, Context, Result};
use chrono::Utc;
use fs2::FileExt;
use std::fs::{self, File, OpenOptions};

use crate::activation;
use crate::client::FusionClient;
use crate::download::emit;
use crate::inventory;
use crate::model::{EnvironmentAction, EnvironmentSnapshot, ResolveRequest};
use crate::paths::{platform, Layout};

#[path = "install_preparation.rs"]
mod preparation;

pub fn prepare(app_version: &str) -> Result<String> {
    prepare_with_verification(app_version, false)
}

pub fn prepare_repair(app_version: &str) -> Result<String> {
    prepare_with_verification(app_version, true)
}

fn prepare_with_verification(app_version: &str, full_check: bool) -> Result<String> {
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
        inventory: inventory::installation_inventory(&layout, current.as_ref(), full_check),
    };
    let client = FusionClient::new()?;
    let plan = client.resolve(&request)?;
    validate_plan(&plan, app_version, &target, &arch)?;
    let transaction = uuid::Uuid::new_v4().simple().to_string();
    let context = preparation::Context {
        layout: &layout,
        client: &client,
        transaction: &transaction,
        platform: &platform,
        current: current.as_ref(),
        inventory: &request.inventory,
    };
    let state = context.prepare(plan)?;
    let transaction_dir = layout.transaction_dir(&transaction)?;
    inventory::write_json(&transaction_dir.join("prepared.json"), &state)?;
    let pending = layout.pending_transaction();
    fs::write(&pending, &transaction).with_context(|| {
        format!(
            "write pending environment transaction: {}",
            pending.display()
        )
    })?;
    emit("environment", "prepared", 0, 0);
    Ok(transaction)
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
        .open(&path)
        .with_context(|| format!("open environment writer lock: {}", path.display()))?;
    file.try_lock_exclusive().with_context(|| {
        format!(
            "lock environment writer (another operation may be running): {}",
            path.display()
        )
    })?;
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
    fs::write(&path, &value)
        .with_context(|| format!("write installation identity: {}", path.display()))?;
    Ok(value)
}
