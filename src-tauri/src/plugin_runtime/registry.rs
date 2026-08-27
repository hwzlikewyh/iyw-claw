use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock, RwLock};

use sea_orm::DatabaseConnection;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::app_error::AppCommandError;
use crate::commands::skill_market::SkillPluginManifest;
use crate::db::service::plugin_installation_service::{self, PluginInstallationRecord};

static GLOBAL_REGISTRY: OnceLock<PluginRegistry> = OnceLock::new();

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRegistrySnapshot {
    pub generation: u64,
    pub digest: String,
    pub plugins: BTreeMap<String, PluginDescriptor>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDescriptor {
    pub market_skill_id: i64,
    pub slug: String,
    pub version: String,
    pub install_root: String,
    pub publisher_id: String,
    pub trust_state: String,
    pub artifact_signature_key_id: String,
    pub permissions_digest: String,
    pub reconcile_state: String,
    pub available: bool,
    pub manifest: SkillPluginManifest,
    pub activations: Vec<ActivationDescriptor>,
    pub permission_grants: Vec<PermissionGrantDescriptor>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivationDescriptor {
    pub component_key: String,
    pub scope: String,
    pub workspace_key: String,
    pub agent_type: String,
    pub requested_enabled: bool,
    pub routing_mode: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionGrantDescriptor {
    pub scope: String,
    pub workspace_key: String,
    pub permissions_digest: String,
    pub granted_permissions_json: String,
    pub grant_state: String,
}

#[derive(Clone)]
pub struct PluginRegistry {
    inner: Arc<RwLock<Arc<PluginRegistrySnapshot>>>,
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Arc::new(PluginRegistrySnapshot::default()))),
        }
    }
}

impl PluginRegistry {
    pub async fn load(conn: &DatabaseConnection) -> Result<Self, AppCommandError> {
        super::recovery::log_recovery_artifacts();
        let registry = Self::default();
        registry.reconcile(conn).await?;
        Ok(registry)
    }

    pub fn snapshot(&self) -> Arc<PluginRegistrySnapshot> {
        self.inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub async fn reconcile(&self, conn: &DatabaseConnection) -> Result<bool, AppCommandError> {
        let records = plugin_installation_service::list_records(conn)
            .await
            .map_err(AppCommandError::db)?;
        let mut plugins = BTreeMap::new();
        for record in records {
            let slug = record.installation.slug.clone();
            match descriptor(record) {
                Ok((key, value)) => {
                    plugins.insert(key, value);
                }
                Err(error) => {
                    tracing::error!(slug, error = %error, "[plugin-registry] invalid install excluded");
                }
            }
        }
        let digest = snapshot_digest(&plugins)?;
        let current = self.snapshot();
        if current.digest == digest {
            return Ok(false);
        }
        let next = PluginRegistrySnapshot {
            generation: current.generation.saturating_add(1),
            digest,
            plugins,
        };
        *self
            .inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Arc::new(next);
        Ok(true)
    }

    pub fn suspend(&self, plugin_slug: &str) -> bool {
        let current = self.snapshot();
        if !current.plugins.contains_key(plugin_slug) {
            return false;
        }
        let mut plugins = current.plugins.clone();
        plugins.remove(plugin_slug);
        let Ok(digest) = snapshot_digest(&plugins) else {
            return false;
        };
        let next = PluginRegistrySnapshot {
            generation: current.generation.saturating_add(1),
            digest,
            plugins,
        };
        *self
            .inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Arc::new(next);
        true
    }
}

pub fn install_global(registry: PluginRegistry) -> PluginRegistry {
    GLOBAL_REGISTRY.get_or_init(|| registry).clone()
}

pub fn global_snapshot() -> Option<Arc<PluginRegistrySnapshot>> {
    GLOBAL_REGISTRY.get().map(PluginRegistry::snapshot)
}

pub async fn reconcile_global(conn: &DatabaseConnection) -> Result<bool, AppCommandError> {
    let Some(registry) = GLOBAL_REGISTRY.get() else {
        return Ok(false);
    };
    registry.reconcile(conn).await
}

pub fn suspend_global(plugin_slug: &str) -> bool {
    GLOBAL_REGISTRY
        .get()
        .is_some_and(|registry| registry.suspend(plugin_slug))
}

pub fn market_skill_state_global(market_skill_id: i64) -> Option<(bool, bool)> {
    GLOBAL_REGISTRY.get().map(|registry| {
        let descriptor = registry
            .snapshot()
            .plugins
            .values()
            .find(|plugin| plugin.market_skill_id == market_skill_id)
            .cloned();
        descriptor.map_or((false, false), |plugin| (true, plugin.available))
    })
}

fn descriptor(
    record: PluginInstallationRecord,
) -> Result<(String, PluginDescriptor), AppCommandError> {
    let manifest: SkillPluginManifest = serde_json::from_str(&record.installation.manifest_json)
        .map_err(|error| {
            AppCommandError::configuration_invalid("Installed plugin manifest is invalid")
                .with_detail(error.to_string())
        })?;
    let available = super::recovery::valid_current_pointer(&record)
        && record.installation.status == "installed"
        && record.installation.reconcile_state == "ready"
        && (manifest.schema_version < 2 || record.installation.trust_state == "trusted");
    let slug = record.installation.slug.clone();
    let activations = activation_descriptors(record.activations);
    let permission_grants = permission_descriptors(record.permission_grants);
    Ok((
        slug.clone(),
        PluginDescriptor {
            market_skill_id: record.installation.market_skill_id,
            slug,
            version: record.installation.version,
            install_root: record.installation.install_root,
            publisher_id: record.installation.publisher_id,
            trust_state: record.installation.trust_state,
            artifact_signature_key_id: record.installation.artifact_signature_key_id,
            permissions_digest: record.installation.permissions_digest,
            reconcile_state: record.installation.reconcile_state,
            available,
            manifest,
            activations,
            permission_grants,
        },
    ))
}

fn activation_descriptors(
    values: Vec<crate::db::entities::plugin_activation_policy::Model>,
) -> Vec<ActivationDescriptor> {
    let mut result = values
        .into_iter()
        .map(|value| ActivationDescriptor {
            component_key: value.component_key,
            scope: value.scope,
            workspace_key: value.workspace_key,
            agent_type: value.agent_type,
            requested_enabled: value.requested_enabled,
            routing_mode: value.routing_mode,
        })
        .collect::<Vec<_>>();
    result.sort_by(|left, right| {
        (
            &left.component_key,
            &left.scope,
            &left.workspace_key,
            &left.agent_type,
        )
            .cmp(&(
                &right.component_key,
                &right.scope,
                &right.workspace_key,
                &right.agent_type,
            ))
    });
    result
}

fn permission_descriptors(
    values: Vec<crate::db::entities::plugin_permission_grant::Model>,
) -> Vec<PermissionGrantDescriptor> {
    let mut result = values
        .into_iter()
        .map(|value| PermissionGrantDescriptor {
            scope: value.scope,
            workspace_key: value.workspace_key,
            permissions_digest: value.permissions_digest,
            granted_permissions_json: value.granted_permissions_json,
            grant_state: value.grant_state,
        })
        .collect::<Vec<_>>();
    result.sort_by(|left, right| {
        (&left.scope, &left.workspace_key).cmp(&(&right.scope, &right.workspace_key))
    });
    result
}

fn snapshot_digest(
    plugins: &BTreeMap<String, PluginDescriptor>,
) -> Result<String, AppCommandError> {
    let bytes = serde_json::to_vec(plugins).map_err(|error| {
        AppCommandError::configuration_invalid("Plugin registry cannot be serialized")
            .with_detail(error.to_string())
    })?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}
