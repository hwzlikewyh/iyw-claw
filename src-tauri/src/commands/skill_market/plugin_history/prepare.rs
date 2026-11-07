use super::{
    build::{self, Context},
    local,
    types::{InspectInput, Prepared, VersionReview},
};
use crate::app_error::AppCommandError;
use crate::db::service::plugin_installation_service;
use sea_orm::DatabaseConnection;

pub async fn inspect(
    conn: &DatabaseConnection,
    input: InspectInput,
) -> Result<VersionReview, AppCommandError> {
    let _storage = crate::acp::agent_storage_work::begin_agent_storage_read().await;
    let _operation = crate::plugin_runtime::registry::lock_mutation().await;
    Ok(prepare(conn, &input).await?.view)
}

pub(super) async fn prepare(
    conn: &DatabaseConnection,
    input: &InspectInput,
) -> Result<Prepared, AppCommandError> {
    let (mut prepared, candidate, receipt) = initial(conn, input).await?;
    let plan = match crate::commands::plugin_recovery::request_release_plan(conn, &candidate).await
    {
        Ok(plan) => plan,
        Err(error) => {
            prepared.view.issues.push(error.message);
            return Ok(prepared);
        }
    };
    let records = plugin_installation_service::list_records(conn)
        .await
        .map_err(AppCommandError::db)?;
    let agents = serde_json::from_str(&prepared.current.installation.agent_types_json)
        .map_err(|_| super::invalid("当前智能体安装记录损坏"))?;
    let context = Context {
        plan,
        root: receipt,
        records,
        agents,
        scope: prepared.scope.clone(),
        root_id: prepared.current.installation.market_skill_id,
    };
    finish(conn, &mut prepared, &context).await?;
    Ok(prepared)
}

async fn finish(
    conn: &DatabaseConnection,
    prepared: &mut Prepared,
    context: &Context,
) -> Result<(), AppCommandError> {
    prepared.view.listed = context.plan.items.last().map(|item| item.listed);
    let built = build::build(context).await?;
    prepared.view.scope_changed |= built.scope_changed;
    prepared.view.issues.extend(built.issues);
    prepared.view.dependencies = built.dependencies;
    prepared.plugins = built.plugins;
    prepared.skills = built.skills;
    prepared.agents = context.agents.clone();
    let versions = context
        .plan
        .items
        .iter()
        .map(|item| {
            Ok((
                item.skill_id
                    .parse::<i64>()
                    .map_err(|_| super::invalid("依赖标识无效"))?,
                (item.slug.clone(), item.version.clone()),
            ))
        })
        .collect::<Result<super::super::plugin_constraints::PlannedVersions, AppCommandError>>()?;
    let conflicts = super::super::plugin_constraints::report(conn, &versions).await?;
    prepared.view.issues.extend(conflicts.issues);
    prepared.view.related = conflicts
        .related
        .into_iter()
        .map(|(id, slug)| {
            let is_plugin = context.records.iter().any(|record| {
                record.installation.market_skill_id.to_string() == id
            });
            super::types::RelatedCapability {
                package_type: if is_plugin { "plugin" } else { "skill" }.into(),
                id,
                slug,
            }
        })
        .collect();
    check_agents(conn, prepared).await;
    review_changes(prepared, context)?;
    Ok(())
}

async fn initial(
    conn: &DatabaseConnection,
    input: &InspectInput,
) -> Result<
    (
        Prepared,
        crate::db::service::plugin_installation_service::PluginInstallationRecord,
        super::super::plugin_receipt::PluginVersionReceipt,
    ),
    AppCommandError,
> {
    let current = super::current(conn, &input.market_skill_id).await?;
    if current.installation.version == input.version {
        return Err(super::invalid("当前已是此版本，无需切换"));
    }
    let scope = crate::commands::mcp_cli_config::account_scope(conn).await?;
    let receipt = load_receipt(&current.installation.slug, &input.version).await?;
    if receipt.market.skill_id != current.installation.market_skill_id {
        return Err(super::invalid("历史版本不属于当前插件"));
    }
    let candidate = local::candidate(&current, &receipt)?;
    let view = VersionReview {
        slug: current.installation.slug.clone(),
        current_version: current.installation.version.clone(),
        version: input.version.clone(),
        state: "blocked".into(),
        review_token: None,
        scope_changed: receipt.account_scope != scope,
        permissions_changed: false,
        preparation_changed: false,
        listed: None,
        issues: Vec::new(),
        dependencies: Vec::new(),
        related: Vec::new(),
    };
    let prepared = Prepared {
        current,
        scope,
        view,
        agents: Vec::new(),
        plugins: Vec::new(),
        skills: Vec::new(),
    };
    Ok((prepared, candidate, receipt))
}

async fn load_receipt(
    slug: &str,
    version: &str,
) -> Result<super::super::plugin_receipt::PluginVersionReceipt, AppCommandError> {
    let slug = slug.to_string();
    let version = version.to_string();
    tokio::task::spawn_blocking(move || {
        let receipt = local::receipt(&slug, &version)?;
        super::super::plugin_client_version::require_current(&receipt.manifest.min_client_version)?;
        let package = local::package(&receipt)?;
        super::super::plugin_manifest::validate_plugin_package(
            &package,
            &receipt.manifest,
            &slug,
            &version,
        )?;
        Ok::<_, AppCommandError>(receipt)
    })
    .await
    .map_err(|_| super::invalid("历史版本验证未完成"))?
}

fn review_changes(prepared: &mut Prepared, context: &Context) -> Result<(), AppCommandError> {
    prepared.view.scope_changed |= context
        .records
        .iter()
        .filter(|record| {
            context
                .plan
                .items
                .iter()
                .any(|item| item.skill_id == record.installation.market_skill_id.to_string())
        })
        .any(|record| {
            local::pointer(record)
                .ok()
                .flatten()
                .map(|pointer| pointer.account_scope)
                != Some(context.scope.clone())
        });
    prepared.view.permissions_changed =
        prepared.view.scope_changed || changed(prepared, context, true);
    prepared.view.preparation_changed = changed(prepared, context, false);
    if prepared.view.issues.is_empty() {
        prepared.view.state = "ready".into();
        prepared.view.review_token = Some(super::fingerprint::token(context, prepared)?);
    }
    Ok(())
}

fn changed(prepared: &Prepared, context: &Context, permissions: bool) -> bool {
    prepared.plugins.iter().any(|plugin| {
        context
            .records
            .iter()
            .find(|record| record.installation.market_skill_id == plugin.market_skill_id)
            .and_then(|record| {
                serde_json::from_str::<super::super::SkillPluginManifest>(
                    &record.installation.manifest_json,
                )
                .ok()
            })
            .map_or(true, |manifest| {
                if permissions {
                    manifest.permissions != plugin.plugin.manifest.permissions
                } else {
                    manifest.components != plugin.plugin.manifest.components
                }
            })
    })
}

async fn check_agents(conn: &DatabaseConnection, prepared: &mut Prepared) {
    if !prepared.skills.is_empty()
        || prepared.plugins.iter().any(|plugin| {
            plugin
                .plugin
                .manifest
                .components
                .iter()
                .any(|component| component.kind == "skill")
        })
    {
        if let Err(error) =
            super::super::install::validate_install_targets(conn, &prepared.agents).await
        {
            prepared.view.issues.push(error.message);
        }
    }
}
