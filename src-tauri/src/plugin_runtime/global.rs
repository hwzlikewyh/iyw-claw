use std::sync::{Arc, OnceLock};

use super::router::PluginRouter;
use super::supervisor::PluginRuntimeSupervisor;
use super::{app_host::PluginAppRegistry, app_launch_broker::PluginAppLaunchBroker};

static GLOBAL_SUPERVISOR: OnceLock<Arc<PluginRuntimeSupervisor>> = OnceLock::new();
static GLOBAL_ROUTER: OnceLock<PluginRouter> = OnceLock::new();
static GLOBAL_DATABASE: OnceLock<sea_orm::DatabaseConnection> = OnceLock::new();
static GLOBAL_APPS: OnceLock<PluginAppRegistry> = OnceLock::new();
static GLOBAL_APP_BROKER: OnceLock<PluginAppLaunchBroker> = OnceLock::new();

pub fn install_supervisor(
    supervisor: Arc<PluginRuntimeSupervisor>,
) -> Arc<PluginRuntimeSupervisor> {
    GLOBAL_SUPERVISOR.get_or_init(|| supervisor).clone()
}

pub async fn stop_plugin(plugin_slug: &str) {
    stop_plugin_version(plugin_slug, None).await;
}

pub async fn stop_plugin_version(plugin_slug: &str, plugin_version: Option<&str>) {
    let revoked_leases = GLOBAL_APPS.get().map_or(0, |apps| {
        apps.teardown_plugin_version(plugin_slug, plugin_version)
    });
    let cancelled_tickets = GLOBAL_APP_BROKER.get().map_or(0, |broker| {
        broker.cancel_plugin_version(plugin_slug, plugin_version)
    });
    let inactive_instances = if let Some(database) = GLOBAL_DATABASE.get() {
        crate::db::service::plugin_app_instance_service::mark_plugin_inactive_version(
            database,
            plugin_slug,
            plugin_version,
        )
        .await
        .unwrap_or_else(|error| {
            tracing::error!(plugin = plugin_slug, error = %error,
                "[plugin-runtime] failed to mark plugin app instances inactive");
            0
        })
    } else {
        0
    };
    if let Some(supervisor) = GLOBAL_SUPERVISOR.get() {
        supervisor
            .stop_plugin_version(plugin_slug, plugin_version)
            .await;
    }
    tracing::info!(
        plugin = plugin_slug,
        revoked_leases,
        cancelled_tickets,
        inactive_instances,
        version = plugin_version.unwrap_or("all"),
        "[plugin-runtime] plugin runtime and app state stopped"
    );
}

pub fn install_router(router: PluginRouter) -> PluginRouter {
    GLOBAL_ROUTER.get_or_init(|| router).clone()
}

pub fn router() -> Option<PluginRouter> {
    GLOBAL_ROUTER.get().cloned()
}

pub fn install_database(database: sea_orm::DatabaseConnection) {
    let _ = GLOBAL_DATABASE.set(database);
}

pub fn database() -> Option<sea_orm::DatabaseConnection> {
    GLOBAL_DATABASE.get().cloned()
}

pub fn install_apps(apps: PluginAppRegistry) -> PluginAppRegistry {
    GLOBAL_APPS.get_or_init(|| apps).clone()
}

pub fn apps() -> Option<PluginAppRegistry> {
    GLOBAL_APPS.get().cloned()
}

pub fn app_launch_broker() -> PluginAppLaunchBroker {
    GLOBAL_APP_BROKER.get_or_init(Default::default).clone()
}
