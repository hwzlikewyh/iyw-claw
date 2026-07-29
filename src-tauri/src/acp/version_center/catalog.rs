use std::sync::Arc;

use sea_orm::DatabaseConnection;
use tokio::sync::RwLock;

use crate::acp::registry;
use crate::acp::version_center::capability::{self, CATALOG_SCHEMA_VERSION};
use crate::acp::version_center::client::{AgentPlatformClient, CatalogFetch};
use crate::acp::version_center::types::{CatalogPlatform, CatalogSnapshot, CatalogTool};
use crate::app_error::AppCommandError;
use crate::db::service::app_metadata_service;

const CACHE_KEY: &str = "agent_version_center.catalog.v1";

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogView {
    pub snapshot: CatalogSnapshot,
    pub stale: bool,
    pub etag: Option<String>,
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
    let platforms = registry::all_acp_agents()
        .into_iter()
        .enumerate()
        .map(|(sort_order, agent_type)| {
            let meta = registry::get_agent_meta(agent_type);
            CatalogPlatform {
                registry_id: registry::registry_id_for(agent_type).to_string(),
                display_name: meta.name.to_string(),
                description: meta.description.to_string(),
                status: "active".to_string(),
                sort_order: sort_order as i32,
                channel: "stable".to_string(),
                recommended_version: meta.registry_version().unwrap_or_default().to_string(),
                minimum_safe_version: String::new(),
                default_update_policy: "recommended".to_string(),
            }
        })
        .collect();
    let tools = [("git", "Git"), ("node", "Node.js/npm"), ("uv", "uv/uvx")]
        .into_iter()
        .enumerate()
        .map(|(sort_order, (tool_id, display_name))| CatalogTool {
            tool_id: tool_id.to_string(),
            display_name: display_name.to_string(),
            description: String::new(),
            status: "active".to_string(),
            sort_order: sort_order as i32,
            channel: "stable".to_string(),
            recommended_version: String::new(),
            minimum_safe_version: String::new(),
            default_update_policy: "recommended".to_string(),
        })
        .collect();
    CatalogSnapshot {
        schema_version: CATALOG_SCHEMA_VERSION,
        revision: 0,
        generated_at: String::new(),
        platforms,
        tools,
    }
}
