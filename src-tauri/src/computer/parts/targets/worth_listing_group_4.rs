// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl TargetEntry {
    /// Whether this window belongs in a listing: on screen, minimized, hidden
    /// with its application, on another Space, or shared. What that leaves out is the invisible
    /// furniture every desktop is full of — an application's hidden helper
    /// windows, the Finder's off-screen desktop strips — which nobody means to
    /// share and a picker full of would hide the ones they do. A shared window
    /// stays listed whatever its visibility (its application may just be
    /// hidden), so that its sharing is never out of sight.
    pub fn worth_listing(&self) -> bool {
        self.on_screen
            || self.minimized == Some(true)
            || self.hidden == Some(true)
            || self.on_current_space == Some(false)
            || self.grant.is_some()
    }

    /// The window as an agent may see it.
    pub fn agent_summary(&self, me: &SelfIdentity, blocklist: &Blocklist) -> AgentWindowSummary {
        let level = level_of(self.grant.as_ref());
        AgentWindowSummary {
            target_id: self.target_id.clone(),
            app: AgentAppRef {
                key: self.app.key().unwrap_or_default().to_string(),
                name: self.app.name.clone(),
                pid: self.app.pid,
            },
            bounds: self.bounds,
            on_screen: self.on_screen,
            minimized: self.minimized,
            hidden: self.hidden,
            level,
            whole_app: self
                .grant
                .as_ref()
                .is_some_and(|g| g.scope == GrantScope::App),
            whole_screen: self
                .grant
                .as_ref()
                .is_some_and(|g| g.scope == GrantScope::Screen),
            title: visible_title(level, &self.title),
            note: grantable(&self.app, me, blocklist)
                .err()
                .map(|why| why.note().to_string()),
        }
    }
}
