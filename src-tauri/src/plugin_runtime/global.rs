use std::sync::{Arc, OnceLock};

use super::supervisor::PluginRuntimeSupervisor;

static GLOBAL_SUPERVISOR: OnceLock<Arc<PluginRuntimeSupervisor>> = OnceLock::new();

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
