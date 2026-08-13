use std::sync::Arc;

use sea_orm::DatabaseConnection;
use tokio::sync::RwLock;

use crate::acp::registry;
use crate::acp::version_center::capability::{self, CATALOG_SCHEMA_VERSION};
use crate::acp::version_center::client::{AgentPlatformClient, CatalogFetch};
use crate::acp::version_center::types::CatalogSnapshot;
use crate::app_error::AppCommandError;
use crate::db::service::app_metadata_service;
use crate::models::agent::AgentType;

const CACHE_KEY: &str = "agent_version_center.catalog.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformAccess {
    Active,
    Hidden,
    Disabled,
    Missing,
}

#[derive(Debug, Clone)]
pub struct PlatformProjection {
    pub access: PlatformAccess,
    pub recommended_version: Option<String>,
}

impl PlatformProjection {
    pub fn visible(self, installed: bool) -> bool {
        self.access == PlatformAccess::Active
            || (self.access == PlatformAccess::Hidden && installed)
    }

    pub fn install_allowed(self, installed: bool) -> bool {
        self.visible(installed)
    }

    pub fn launch_allowed(self) -> bool {
        matches!(self.access, PlatformAccess::Active | PlatformAccess::Hidden)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogView {
    pub snapshot: CatalogSnapshot,
    pub stale: bool,
    pub etag: Option<String>,
}

pub async fn platform_projection(
    conn: &DatabaseConnection,
    agent_type: AgentType,
) -> PlatformProjection {
    let raw = match app_metadata_service::get_value(conn, CACHE_KEY).await {
        Ok(Some(raw)) => raw,
        Ok(None) => return fallback_projection(agent_type),
        Err(error) => {
            tracing::warn!(%error, "[agent-version-center] catalog cache read failed");
            return fallback_projection(agent_type);
        }
    };
    let Ok(snapshot) = serde_json::from_str::<CatalogSnapshot>(&raw) else {
        return fallback_projection(agent_type);
    };
    if capability::validate_catalog(&snapshot).is_err() {
        return fallback_projection(agent_type);
    }
    let registry_id = registry::registry_id_for(agent_type);
    let Some(platform) = snapshot
        .platforms
        .iter()
        .find(|item| item.registry_id == registry_id)
    else {
        return PlatformProjection {
            access: PlatformAccess::Missing,
            recommended_version: None,
        };
    };
    PlatformProjection {
        access: match platform.status.as_str() {
            "active" => PlatformAccess::Active,
            "hidden" => PlatformAccess::Hidden,
            "disabled" => PlatformAccess::Disabled,
            _ => PlatformAccess::Missing,
        },
        recommended_version: nonempty(&platform.recommended_version),
    }
}

fn fallback_projection(agent_type: crate::models::agent::AgentType) -> PlatformProjection {
    tracing::warn!(
        agent_type = ?agent_type,
        "[agent-version-center] no trusted catalog is available; denying Agent access"
    );
    PlatformProjection {
        access: PlatformAccess::Missing,
        recommended_version: None,
    }
}

pub async fn authorize_agent_launch(
    conn: &DatabaseConnection,
    agent_type: AgentType,
) -> Result<(), AppCommandError> {
    let setting = crate::db::service::agent_setting_service::get_by_agent_type(conn, agent_type)
        .await
        .map_err(AppCommandError::from)?
        .ok_or_else(|| AppCommandError::configuration_invalid("Agent setting is unavailable"))?;
    let version = setting
        .installed_version
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppCommandError::configuration_invalid("Agent is not installed"))?;
    let offer = AgentPlatformClient::resolve_agent(
        conn,
        crate::acp::version_center::types::ResolveAgentRequest {
            registry_id: registry::registry_id_for(agent_type),
            current_version: version,
            requested_version: Some(version),
            pinned_version: setting.pinned_version.as_deref(),
            client_version: env!("CARGO_PKG_VERSION"),
            runtime: capability::RUNTIME,
            target: capability::current_target(),
            arch: capability::current_arch(),
            channel: &setting.update_channel,
            reason: "manual",
        },
    )
    .await?;
    (offer.version == version)
        .then_some(())
        .ok_or_else(|| AppCommandError::configuration_invalid("Agent launch version was rejected"))
}

fn nonempty(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.trim().to_string())
}

#[derive(Debug, Clone)]
struct CatalogState {
    snapshot: CatalogSnapshot,
    stale: bool,
    etag: Option<String>,
}

#[derive(Clone)]
pub struct CatalogStore {
    state: Arc<RwLock<CatalogState>>,
}

impl CatalogStore {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(CatalogState {
                snapshot: built_in_snapshot(),
                stale: true,
                etag: None,
            })),
        }
    }

    pub async fn load(conn: &DatabaseConnection) -> Self {
        let store = Self::new();
        let cached = app_metadata_service::get_value(conn, CACHE_KEY).await;
        if let Ok(Some(raw)) = cached {
            store.restore_cached(&raw).await;
        }
        store
    }

    pub async fn view(&self) -> CatalogView {
        let state = self.state.read().await;
        CatalogView {
            snapshot: state.snapshot.clone(),
            stale: state.stale,
            etag: state.etag.clone(),
        }
    }

    pub async fn refresh(&self, conn: &DatabaseConnection) -> Result<CatalogView, AppCommandError> {
        let etag = self.state.read().await.etag.clone();
        let fetched = AgentPlatformClient::fetch_catalog(conn, etag.as_deref()).await;
        match fetched {
            Ok(CatalogFetch::NotModified) => self.mark_fresh().await,
            Ok(CatalogFetch::Updated { snapshot, etag }) => {
                self.accept_remote(conn, snapshot, etag).await
            }
            Err(error) => {
                self.mark_stale().await;
                Err(error)
            }
        }
    }

    async fn restore_cached(&self, raw: &str) {
        let Ok(snapshot) = serde_json::from_str::<CatalogSnapshot>(raw) else {
            tracing::warn!("[agent-version-center] ignored invalid cached catalog");
            return;
        };
        if let Err(error) = capability::validate_catalog(&snapshot) {
            tracing::warn!(error = %error, "[agent-version-center] ignored unsafe cached catalog");
            return;
        }
        let mut state = self.state.write().await;
        state.snapshot = merge_known_entries(snapshot);
        state.stale = true;
    }

    async fn accept_remote(
        &self,
        conn: &DatabaseConnection,
        snapshot: CatalogSnapshot,
        etag: Option<String>,
    ) -> Result<CatalogView, AppCommandError> {
        capability::validate_catalog(&snapshot).map_err(|error| {
            AppCommandError::configuration_invalid("Agent catalog was rejected").with_detail(error)
        })?;
        let snapshot = merge_known_entries(snapshot);
        let serialized = serde_json::to_string(&snapshot).map_err(|error| {
            AppCommandError::configuration_invalid("Failed to cache Agent catalog")
                .with_detail(error.to_string())
        })?;
        let mut state = self.state.write().await;
        if snapshot.revision < state.snapshot.revision {
            return Err(AppCommandError::configuration_invalid(
                "Agent catalog revision moved backwards",
            ));
        }
        app_metadata_service::upsert_value(conn, CACHE_KEY, &serialized)
            .await
            .map_err(AppCommandError::from)?;
        state.snapshot = snapshot;
        state.stale = false;
        state.etag = etag;
        Ok(CatalogView {
            snapshot: state.snapshot.clone(),
            stale: false,
            etag: state.etag.clone(),
        })
    }

    async fn mark_fresh(&self) -> Result<CatalogView, AppCommandError> {
        let mut state = self.state.write().await;
        state.stale = false;
        Ok(CatalogView {
            snapshot: state.snapshot.clone(),
            stale: false,
            etag: state.etag.clone(),
        })
    }

    async fn mark_stale(&self) {
        self.state.write().await.stale = true;
    }
}

fn merge_known_entries(mut remote: CatalogSnapshot) -> CatalogSnapshot {
    remote
        .platforms
        .retain(|item| registry::from_registry_id(&item.registry_id).is_some());
    remote
        .tools
        .retain(|item| capability::known_tool(&item.tool_id));
    remote
}

fn built_in_snapshot() -> CatalogSnapshot {
    CatalogSnapshot {
        schema_version: CATALOG_SCHEMA_VERSION,
        revision: 0,
        generated_at: String::new(),
        platforms: Vec::new(),
        tools: Vec::new(),
    }
}
