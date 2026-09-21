use anyhow::{bail, Context, Result};
use chrono::Utc;
use fs2::FileExt;
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};

use crate::client::FusionClient;
use crate::component::PrepareContext;
use crate::download::emit;
use crate::inventory;
use crate::model::{
    EnvironmentAction, EnvironmentSnapshot, InstalledComponent, PreparedComponent, PreparedState,
    ResolveRequest,
};
use crate::paths::{platform, Layout};
use crate::{activation, component, transaction, validation};

pub fn prepare(app_version: &str) -> Result<String> {
    validation::validate_platform()?;
    let layout = Layout::resolve()?;
    layout.ensure()?;
    let _lock = environment_lock(&layout)?;
    let current = inventory::load_current(&layout).unwrap_or_else(|error| {
        eprintln!("环境清单不可读，将从已校验制品重建：{error:#}");
        None
    });
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
    validation::validate_plan(
        &plan,
        &validation::PlanExpectation {
            app_version,
            target: &target,
            arch: &arch,
        },
    )?;
    let transaction = uuid::Uuid::new_v4().simple().to_string();
    let context = PrepareContext {
        layout: &layout,
        client: &client,
        transaction: &transaction,
        platform: &platform,
        request: &request,
    };
    let state = prepare_actions(&context, plan, current.as_ref())?;
    let transaction_dir = layout.transaction_dir(&transaction)?;
    inventory::write_json(&transaction_dir.join("prepared.json"), &state)?;
    fs::write(layout.pending_transaction(), &transaction)?;
    emit("environment", "prepared", (0, 0));
    Ok(transaction)
}

fn prepare_actions(
    context: &PrepareContext<'_>,
    plan: crate::model::EnvironmentPlan,
    current: Option<&EnvironmentSnapshot>,
) -> Result<PreparedState> {
    let mut components = Vec::new();
    let mut seen = BTreeSet::new();
    for action in &plan.actions {
        if !seen.insert(action.component_id.clone()) {
            bail!("environment plan contains a duplicate component")
        }
        match component::prepare_action(context, action, current)
            .with_context(|| format!("组件 {} 安装失败", action.component_id))
        {
            Ok(component) => components.push(component),
            Err(error) if action.optional => match healthy_current(context.layout, current, action)
            {
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
                None => return Err(error),
            },
            Err(error) => return Err(error),
        }
    }
    Ok(PreparedState {
        transaction_id: context.transaction.to_string(),
        base_digest: inventory::snapshot_digest(context.layout)?,
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
    validation::validate_prepared(&state)?;
    let published = transaction::published_snapshot(&layout, &state)?;
    crate::revalidation::verify(&layout, &state, installation_id(&layout)?)?;
    let snapshot = transaction::snapshot(&state);
    crate::maintenance::install(&layout, &state)?;
    activation::commit(&layout, &state, published.as_ref().unwrap_or(&snapshot))?;
    transaction::finish(&layout, &state.transaction_id)?;
    let _ = fs::remove_file(layout.pending_transaction());
    emit("environment", "committed", (0, 0));
    Ok(())
}

pub fn diagnose() -> Result<u8> {
    let layout = Layout::resolve()?;
    let snapshot = match inventory::load_current(&layout) {
        Ok(value) => value,
        Err(error) => {
            println!(
                "{}",
                serde_json::json!({"status":"corrupt","message":format!("{error:#}")})
            );
            return Ok(10);
        }
    };
    let Some(snapshot) = snapshot else {
        println!("{}", serde_json::json!({"status":"missing"}));
        return Ok(10);
    };
    if let Err(error) = transaction::verify_finished(&layout, &snapshot) {
        println!(
            "{}",
            serde_json::json!({"status":"incomplete","message":format!("{error:#}")})
        );
        return Ok(10);
    }
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
