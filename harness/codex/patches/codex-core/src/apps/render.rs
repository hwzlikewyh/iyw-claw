use crate::connectors::AppInfo;
use crate::context::AppsInstructions;
use crate::context::ContextualUserFragment;
use codex_protocol::protocol::APPS_INSTRUCTIONS_CLOSE_TAG;
use codex_protocol::protocol::APPS_INSTRUCTIONS_OPEN_TAG;

pub(crate) fn render_apps_section(connectors: &[AppInfo]) -> Option<String> {
    connectors
        .iter()
        .any(|connector| connector.is_accessible && connector.is_enabled)
        .then(|| AppsInstructions.render())
}
