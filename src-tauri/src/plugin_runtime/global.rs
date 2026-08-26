use std::sync::{Arc, OnceLock};

use super::router::PluginRouter;
use super::supervisor::PluginRuntimeSupervisor;

static GLOBAL_SUPERVISOR: OnceLock<Arc<PluginRuntimeSupervisor>> = OnceLock::new();
static GLOBAL_ROUTER: OnceLock<PluginRouter> = OnceLock::new();
static GLOBAL_DATABASE: OnceLock<sea_orm::DatabaseConnection> = OnceLock::new();

pub fn install_supervisor(
    supervisor: Arc<PluginRuntimeSupervisor>,
) -> Arc<PluginRuntimeSupervisor> {
    GLOBAL_SUPERVISOR.get_or_init(|| supervisor).clone()
}

pub async fn stop_plugin(plugin_slug: &str) {
    if let Some(supervisor) = GLOBAL_SUPERVISOR.get() {
        supervisor.stop_plugin(plugin_slug).await;
    }
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
