use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Context as _, Result};
use chrono::Utc;

use crate::client::FusionClient;
use crate::download::{emit, ensure_cached};
use crate::model::{
    EnvironmentAction, EnvironmentPlan, EnvironmentSnapshot, InstalledComponent, InventoryEntry,
    PreparedComponent, PreparedState,
};
use crate::paths::{slash_relative, Layout};
use crate::{archive, inventory};

pub(super) struct Context<'a> {
    pub layout: &'a Layout,
    pub client: &'a FusionClient,
    pub transaction: &'a str,
    pub platform: &'a str,
    pub current: Option<&'a EnvironmentSnapshot>,
    pub inventory: &'a [InventoryEntry],
}

impl Context<'_> {
    pub fn prepare(&self, plan: EnvironmentPlan) -> Result<PreparedState> {
        let mut components = Vec::new();
        let mut seen = BTreeSet::new();
        for action in &plan.actions {
            if !seen.insert(&action.component_id) {
                bail!("environment plan contains a duplicate component")
            }
            match self
                .prepare_action(action)
                .with_context(|| format!("prepare environment component: {}", action.component_id))
            {
                Ok(component) => {
                    emit(&action.component_id, "component-prepared", 0, 0);
                    components.push(component);
                }
                Err(error) if action.optional => {
                    eprintln!(
                        "optional component {} preparation failed: {error:#}",
                        action.component_id
                    );
                    if let Some(component) = self.available_current(action) {
                        emit(&action.component_id, "reused", 0, 0);
                        components.push(PreparedComponent {
                            component,
                            staged: false,
                        });
                    }
                    emit(&action.component_id, "optional-skipped", 0, 0);
                }
                Err(error) => return Err(error),
            }
        }
        Ok(PreparedState {
            transaction_id: self.transaction.to_string(),
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

    fn prepare_action(&self, action: &EnvironmentAction) -> Result<PreparedComponent> {
        super::validate_action(action)?;
        if let Some(mut component) = self.available_current(action).filter(|component| {
            component.version == action.version
                && component.component_kind == action.component_kind
                && component.version_id == action.artifact.version_id
                && component.artifact_id == action.artifact.artifact_id
                && component.package_kind == action.artifact.package_kind
                && component.artifact_sha256 == action.artifact.sha256
        }) {
            component.optional = action.optional;
            emit(&action.component_id, "reused", 0, 0);
            return Ok(PreparedComponent {
                component,
                staged: false,
            });
        }
        if action.action == "keep" {
            bail!("Fusion keep action does not match an available installed component")
        }
        let staged = self.stage(action)?;
        self.describe(action, staged)
    }

    fn available_current(&self, action: &EnvironmentAction) -> Option<InstalledComponent> {
        self.current?
            .components
            .iter()
            .find(|component| {
                component.component_id == action.component_id
                    && self.inventory.iter().any(|entry| {
                        entry.component_key == component.component_id
                            && entry.active
                            && entry.healthy
                            && entry.current_version == component.version
                            && entry.sha256 == component.artifact_sha256
                    })
            })
            .cloned()
    }

    fn stage(&self, action: &EnvironmentAction) -> Result<PathBuf> {
        let cache = self.layout.runtime.join("cache/downloads");
        let archive = ensure_cached(
            self.client.http(),
            &cache,
            &action.artifact,
            &action.component_id,
        )?;
        let staged = self
            .layout
            .transaction_dir(self.transaction)?
            .join("components")
            .join(crate::paths::safe_segment(
                &action.component_id,
                "component",
            )?);
        if staged.exists() {
            crate::retry::file(
                "remove previous component staging directory",
                &staged,
                || fs::remove_dir_all(&staged),
            )?;
        }
        emit(&action.component_id, "extracting", 0, 0);
        archive::unpack(
            &archive,
            &staged,
            &action.artifact.package_kind,
            &action.artifact.file_name,
        )?;
        Ok(staged)
    }

    fn describe(&self, action: &EnvironmentAction, staged: PathBuf) -> Result<PreparedComponent> {
        let (files, entrypoints) =
            inventory::describe_component(self.layout, &action.component_id, &staged)?;
        let final_dir =
            self.layout
                .component_dir(&action.component_id, &action.version, self.platform)?;
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
                relative_path: slash_relative(&self.layout.root, &final_dir)?,
                entrypoints,
                files,
            },
            staged: true,
        })
    }
}
