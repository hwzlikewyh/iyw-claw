use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};

use crate::archive::{self, UnpackRequest};
use crate::client::FusionClient;
use crate::download::{ensure_cached, CacheRequest};
use crate::inventory;
use crate::model::{EnvironmentAction, EnvironmentSnapshot, InstalledComponent, PreparedComponent};
use crate::paths::{slash_relative, Layout};
use crate::validation;

pub struct PrepareContext<'a> {
    pub layout: &'a Layout,
    pub client: &'a FusionClient,
    pub transaction: &'a str,
    pub platform: &'a str,
    pub request: &'a crate::model::ResolveRequest,
}

pub fn prepare_action(
    context: &PrepareContext<'_>,
    action: &EnvironmentAction,
    current: Option<&EnvironmentSnapshot>,
) -> Result<PreparedComponent> {
    validation::validate_action(action)?;
    if action.action == "keep" {
        return keep_component(context, action, current);
    }
    let staged = stage_component(context, action)?;
    build_component(context, action, staged)
}

fn keep_component(
    context: &PrepareContext<'_>,
    action: &EnvironmentAction,
    current: Option<&EnvironmentSnapshot>,
) -> Result<PreparedComponent> {
    let component = current
        .and_then(|snapshot| {
            snapshot
                .components
                .iter()
                .find(|item| item.component_id == action.component_id)
        })
        .context("Fusion returned keep without a healthy local component")?
        .clone();
    let verified = context.request.inventory.iter().any(|entry| {
        entry.component_key == action.component_id
            && entry.healthy
            && entry.active
            && entry.current_version == action.version
            && entry.sha256 == action.artifact.sha256
    });
    if !verified
        || component.version != action.version
        || component.version_id != action.artifact.version_id
        || component.artifact_id != action.artifact.artifact_id
        || component.package_kind != action.artifact.package_kind
        || component.artifact_sha256 != action.artifact.sha256
    {
        bail!("Fusion keep action does not match the installed component")
    }
    let root = crate::paths::from_slash(&context.layout.root, &component.relative_path)?;
    crate::download::emit(&action.component_id, "health_check", (0, 0));
    crate::health::verify(&root, &action.component_id, &component.entrypoints)?;
    Ok(PreparedComponent {
        component,
        staged: false,
    })
}

fn stage_component(context: &PrepareContext<'_>, action: &EnvironmentAction) -> Result<PathBuf> {
    const DISK_MARGIN: u64 = 512 * 1024 * 1024;
    const EXPANSION_ESTIMATE: u64 = 8;
    let required = action
        .artifact
        .size_bytes
        .saturating_mul(EXPANSION_ESTIMATE)
        .saturating_add(DISK_MARGIN);
    if fs2::available_space(&context.layout.root)? < required {
        bail!(
            "组件 {} 可用磁盘空间不足，需要至少 {} MiB",
            action.component_id,
            required / (1024 * 1024)
        )
    }
    let cache_root = context.layout.runtime.join("cache/downloads");
    let archive_path = ensure_cached(CacheRequest {
        client: context.client,
        environment: context.request,
        cache_root: &cache_root,
        action,
    })?;
    let staged = context
        .layout
        .transaction_dir(context.transaction)?
        .join("components")
        .join(crate::paths::safe_segment(
            &action.component_id,
            "component",
        )?);
    crate::paths::ensure_within(&context.layout.root, &staged)?;
    if staged.exists() {
        bail!("component staging directory already exists")
    }
    archive::unpack(UnpackRequest {
        archive: &archive_path,
        destination: &staged,
        kind: &action.artifact.package_kind,
        file_name: &action.artifact.file_name,
    })?;
    if action.artifact.package_kind == "binary" {
        let source = staged.join(&action.artifact.file_name);
        let destination = staged.join(format!(
            "{}{}",
            action.component_id,
            std::env::consts::EXE_SUFFIX
        ));
        if source != destination {
            fs::rename(source, destination)?;
        }
    }
    Ok(staged)
}

fn build_component(
    context: &PrepareContext<'_>,
    action: &EnvironmentAction,
    staged: PathBuf,
) -> Result<PreparedComponent> {
    let (files, entrypoints) =
        inventory::describe_component(context.layout, &action.component_id, &staged)?;
    crate::download::emit(&action.component_id, "health_check", (0, 0));
    crate::health::verify(&staged, &action.component_id, &entrypoints)?;
    let mut final_dir =
        context
            .layout
            .component_dir(&action.component_id, &action.version, context.platform)?;
    if action.component_id != crate::memory_model::COMPONENT {
        final_dir.push(context.transaction);
    }
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
            relative_path: slash_relative(&context.layout.root, &final_dir)?,
            entrypoints,
            files,
        },
        staged: true,
    })
}
