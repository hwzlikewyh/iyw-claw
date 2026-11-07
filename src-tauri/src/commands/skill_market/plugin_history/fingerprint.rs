use super::{build::Context, local, types::Prepared};
use crate::app_error::AppCommandError;
use sha2::{Digest, Sha256};

pub(super) fn token(context: &Context, prepared: &Prepared) -> Result<String, AppCommandError> {
    let mut records = context.records.iter().collect::<Vec<_>>();
    records.sort_by_key(|record| record.installation.id);
    let stamps = records.into_iter().map(record_stamp).collect::<Vec<_>>();
    let packages = prepared
        .plugins
        .iter()
        .map(|plugin| {
            serde_json::json!([
                plugin.marker,
                plugin.plugin.manifest,
                plugin.publisher_id,
                plugin.signature_key_id
            ])
        })
        .collect::<Vec<_>>();
    let skill_markers = prepared
        .skills
        .iter()
        .map(|skill| &skill.marker)
        .collect::<Vec<_>>();
    let bytes = serde_json::to_vec(&serde_json::json!([
        prepared.scope,
        context.plan,
        context.root,
        stamps,
        packages,
        skill_markers,
        prepared.agents
    ]))
    .map_err(|_| super::invalid("版本检查快照无法编码"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn record_stamp(
    record: &crate::db::service::plugin_installation_service::PluginInstallationRecord,
) -> serde_json::Value {
    let value = &record.installation;
    let mut activations = record
        .activations
        .iter()
        .map(|value| format!("{value:?}"))
        .collect::<Vec<_>>();
    activations.sort();
    let mut grants = record
        .permission_grants
        .iter()
        .map(|value| format!("{value:?}"))
        .collect::<Vec<_>>();
    grants.sort();
    let pointer = local::root(&value.slug)
        .and_then(|root| {
            super::super::plugin_storage_io::read_pointer_bytes(&root.join("current.json"))
        })
        .ok()
        .flatten();
    serde_json::json!([
        value.id,
        value.market_skill_id.to_string(),
        value.version,
        value.content_sha256,
        value.updated_at,
        value.status,
        value.reconcile_state,
        activations,
        grants,
        pointer
    ])
}
