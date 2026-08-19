use std::collections::HashSet;

use serde::Serialize;

use crate::acp::version_center::CatalogSnapshot;
use crate::models::agent::AgentType;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustedAgentProjection {
    pub agent_type: AgentType,
    /// Fusion Agent Platform database id. Cross-service queries use this id.
    pub platform_id: String,
    /// Local identity/directory key only; never use it as a Fusion model key.
    pub registry_id: String,
    pub display_name: String,
    pub description: String,
    pub status: String,
    pub sort_order: i32,
}

pub fn project_catalog(snapshot: &CatalogSnapshot) -> Vec<TrustedAgentProjection> {
    let mut seen = HashSet::new();
    let mut projected = Vec::new();

    for platform in &snapshot.platforms {
        let Some(agent_type) = crate::acp::registry::from_registry_id(&platform.registry_id) else {
            continue;
        };
        if !valid_platform_id(&platform.id) || !valid_status(&platform.status) {
            continue;
        }
        if !seen.insert(agent_type) {
            continue;
        }

        let fallback = super::definition_for(&platform.registry_id)
            .map(|definition| definition.display_name.to_string())
            .unwrap_or_else(|| agent_type.as_wire().into_owned());
        projected.push(TrustedAgentProjection {
            agent_type,
            platform_id: platform.id.clone(),
            registry_id: platform.registry_id.clone(),
            display_name: nonempty(&platform.display_name)
                .map(str::to_string)
                .unwrap_or(fallback),
            description: platform.description.clone(),
            status: platform.status.clone(),
            sort_order: platform.sort_order,
        });
    }

    projected.sort_by(|left, right| {
        left.sort_order
            .cmp(&right.sort_order)
            .then_with(|| left.registry_id.cmp(&right.registry_id))
    });
    projected
}

fn valid_platform_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 19
        && !value.starts_with('0')
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value.parse::<i64>().is_ok_and(|id| id > 0)
}

fn valid_status(value: &str) -> bool {
    matches!(value, "active" | "hidden" | "disabled")
}

fn nonempty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}
