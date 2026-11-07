pub mod apply;
mod local;
mod metadata;
mod skill;
mod state;
mod types;

pub(crate) use local::verify as verify_installed_package;
pub(crate) use metadata::request as request_release_plan;
pub(crate) use types::{RecoveryPlan as ReleaseRecoveryPlan, RecoveryItem as ReleaseRecoveryItem};

use crate::app_error::AppCommandError;
use crate::db::service::plugin_installation_service::{self, PluginInstallationRecord};
use crate::plugin_runtime::{recovery::CurrentPointer, registry};
use sea_orm::DatabaseConnection;
use sha2::{Digest, Sha256};
use types::{DependencyView, Inspection, RecoveryItem, RecoveryTarget};
pub use types::{InspectInput, RecoveryInput, RecoveryReview};

pub async fn inspect(
    conn: &DatabaseConnection,
    input: InspectInput,
) -> Result<RecoveryReview, AppCommandError> {
    let _storage = crate::acp::agent_storage_work::begin_agent_storage_read().await;
    let _operation = registry::lock_mutation().await;
    Ok(prepare(conn, &input.market_skill_id).await?.view)
}

async fn prepare(conn: &DatabaseConnection, id: &str) -> Result<Inspection, AppCommandError> {
    let (record, account_scope, mut view) = initial(conn, id).await?;
    let plan = match metadata::request(conn, &record).await {
        Ok(plan) => Some(plan),
        Err(error) => {
            tracing::warn!(plugin=%record.installation.slug,code=?error.code,"[plugin-recovery] 恢复元数据不可用");
            view.issues.push(error.message);
            None
        }
    };
    let mut targets = Vec::new();
    if let Some(plan) = &plan {
        view.listed = plan.items.last().map(|item| item.listed);
        collect_targets(
            conn,
            (&record, plan),
            (&account_scope, &mut targets, &mut view),
        )
        .await?;
        view.credentials_retained = !view.scope_changed;
        if view.issues.is_empty() {
            view.state = "ready".into();
            view.review_token = Some(review_token(&record, (&account_scope, plan), &targets));
        }
    }
    Ok(Inspection {
        record,
        account_scope,
        plan,
        view,
        targets,
    })
}

async fn initial(
    conn: &DatabaseConnection,
    id: &str,
) -> Result<(PluginInstallationRecord, String, RecoveryReview), AppCommandError> {
    crate::commands::mcp::require_private_agent_storage_for_write()?;
    let id = id
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| invalid("插件市场标识无效"))?;
    let record = plugin_installation_service::find_by_market_skill_id(conn, id)
        .await
        .map_err(AppCommandError::from)?
        .ok_or_else(|| AppCommandError::not_found("插件已卸载，无法恢复本地安装记录"))?;
    let path = local::verify(&record).await?;
    if record.installation.schema_version != 2 || record.installation.trust_state != "trusted" {
        return Err(invalid(
            "此恢复流程需要已验证的v2插件安装，请使用原版本安装入口",
        ));
    }
    let previous = local::read_pointer(&path)?;
    let account_scope = crate::commands::mcp_cli_config::account_scope(conn).await?;
    let scope_changed = pointer_scope(previous.as_deref()) != Some(account_scope.clone());
    let view = RecoveryReview {
        slug: record.installation.slug.clone(),
        version: record.installation.version.clone(),
        state: "blocked".into(),
        scope_changed,
        reset_permissions: scope_changed,
        credentials_retained: !scope_changed,
        listed: None,
        review_token: None,
        issues: Vec::new(),
        dependencies: Vec::new(),
    };
    Ok((record, account_scope, view))
}

async fn collect_targets(
    conn: &DatabaseConnection,
    input: (&PluginInstallationRecord, &types::RecoveryPlan),
    output: (&str, &mut Vec<RecoveryTarget>, &mut RecoveryReview),
) -> Result<(), AppCommandError> {
    let (root, plan) = input;
    let (scope, targets, view) = output;
    for item in &plan.items {
        if item.package_type != "plugin" {
            let dependency = skill::inspect(item).await?;
            if !dependency.ready {
                view.issues.push(format!(
                    "依赖 {} 需要完整且有效的本地版本 {}，请先修复或重新安装",
                    item.slug, item.version
                ));
            }
            view.dependencies.push(dependency);
            continue;
        }
        collect_plugin(conn, (root, item), (scope, targets, view)).await?;
    }
    Ok(())
}

async fn collect_plugin(
    conn: &DatabaseConnection,
    input: (&PluginInstallationRecord, &RecoveryItem),
    output: (&str, &mut Vec<RecoveryTarget>, &mut RecoveryReview),
) -> Result<(), AppCommandError> {
    let (root, item) = input;
    let (scope, targets, view) = output;
    let id = item
        .skill_id
        .parse::<i64>()
        .map_err(|_| invalid("插件依赖标识无效"))?;
    let record = plugin_installation_service::find_by_market_skill_id(conn, id)
        .await
        .map_err(AppCommandError::from)?;
    let installed = record
        .as_ref()
        .map(|record| record.installation.version.clone());
    let target = match record {
        Some(record) if record.installation.version == item.version => {
            target(record, item, scope).await
        }
        _ => Err(invalid(&format!(
            "依赖插件 {} 需要先安装版本 {}",
            item.slug, item.version
        ))),
    };
    let ready = target.is_ok();
    match target {
        Ok(target) => {
            view.reset_permissions |= target.scope_changed;
            view.scope_changed |= target.scope_changed;
            targets.push(target)
        }
        Err(error) => view.issues.push(error.message),
    }
    if id != root.installation.market_skill_id {
        view.dependencies.push(DependencyView {
            package_type: item.package_type.clone(),
            id: item.skill_id.clone(),
            slug: item.slug.clone(),
            required_version: item.version.clone(),
            installed_version: installed,
            ready,
        });
    }
    Ok(())
}

async fn target(
    record: PluginInstallationRecord,
    item: &RecoveryItem,
    scope: &str,
) -> Result<RecoveryTarget, AppCommandError> {
    let manifest: crate::commands::skill_market::SkillPluginManifest =
        serde_json::from_str(&record.installation.manifest_json)
            .map_err(|_| invalid("依赖插件清单损坏"))?;
    if record.installation.content_sha256 != item.content_sha256
        || item.plugin.as_ref() != Some(&manifest)
        || record.installation.slug != item.slug
        || record.installation.trust_state != "trusted"
    {
        return Err(invalid("依赖插件与原版本发布记录不符"));
    }
    let path = local::verify(&record).await?;
    let previous = local::read_pointer(&path)?;
    let scope_changed = pointer_scope(previous.as_deref()).as_deref() != Some(scope);
    Ok(RecoveryTarget {
        record,
        previous_pointer: previous,
        pointer_path: path,
        metadata: item.clone(),
        scope_changed,
    })
}

fn pointer_scope(bytes: Option<&[u8]>) -> Option<String> {
    bytes
        .and_then(|bytes| serde_json::from_slice::<CurrentPointer>(bytes).ok())
        .map(|pointer| pointer.account_scope)
}

fn review_token(
    record: &PluginInstallationRecord,
    input: (&str, &types::RecoveryPlan),
    targets: &[RecoveryTarget],
) -> String {
    let states = targets
        .iter()
        .map(|target| {
            serde_json::json!([
                target.record.installation.id,
                target.record.installation.updated_at,
                target.record.installation.status,
                target.record.installation.reconcile_state,
                target.previous_pointer,
                activation_stamp(&target.record),
                grant_stamp(&target.record),
            ])
        })
        .collect::<Vec<_>>();
    let bytes = serde_json::json!([
        record.installation.market_skill_id.to_string(),
        input.0,
        input.1,
        states
    ])
    .to_string();
    format!("{:x}", Sha256::digest(bytes.as_bytes()))
}

fn activation_stamp(record: &PluginInstallationRecord) -> serde_json::Value {
    let mut values = record
        .activations
        .iter()
        .map(|value| {
            (
                &value.component_key,
                &value.scope,
                &value.workspace_key,
                &value.agent_type,
                value.requested_enabled,
                &value.routing_mode,
                &value.policy_source,
            )
        })
        .collect::<Vec<_>>();
    values.sort();
    serde_json::json!(values)
}

fn grant_stamp(record: &PluginInstallationRecord) -> serde_json::Value {
    let mut values = record
        .permission_grants
        .iter()
        .map(|value| {
            (
                &value.scope,
                &value.workspace_key,
                &value.permissions_digest,
                &value.grant_state,
                &value.granted_permissions_json,
                value.granted_at,
            )
        })
        .collect::<Vec<_>>();
    values.sort();
    serde_json::json!(values)
}

pub(super) fn invalid(message: &str) -> AppCommandError {
    AppCommandError::configuration_invalid(message)
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn plugin_recovery_inspect(
    input: InspectInput,
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<RecoveryReview, AppCommandError> {
    inspect(&db.conn, input).await
}
