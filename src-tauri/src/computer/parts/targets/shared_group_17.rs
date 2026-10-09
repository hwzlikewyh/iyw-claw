// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl TargetTable {
    /// Every window with a grant in force, oldest grant first.
    pub fn shared(&self) -> Vec<SharedWindow> {
        let inner = self.lock();
        let mut out: Vec<SharedWindow> = inner
            .entries
            .values()
            .filter_map(|e| {
                let grant = e.grant.as_ref()?;
                Some(SharedWindow {
                    target_id: e.target_id.clone(),
                    app_name: e.app.name.clone(),
                    app_key: e.app.key().unwrap_or_default().to_string(),
                    title: e.title.clone(),
                    level: grant.level,
                    granted_at: grant.granted_at,
                    last_used_at: grant.last_used_at,
                    whole_app: grant.scope == GrantScope::App,
                    app_id: (grant.scope == GrantScope::App)
                        .then(|| AppIdentity::of(e))
                        .flatten()
                        .and_then(|app| inner.apps.get(&app))
                        .map(|share| share.app_id.clone()),
                    whole_screen: grant.scope == GrantScope::Screen,
                })
            })
            .collect();
        out.sort_by(|a, b| {
            a.granted_at
                .cmp(&b.granted_at)
                .then(a.target_id.cmp(&b.target_id))
        });
        out
    }

    /// Every application shared as a whole, oldest share first.
    pub fn shared_apps(&self) -> Vec<SharedApp> {
        let inner = self.lock();
        let mut out: Vec<SharedApp> = inner
            .apps
            .iter()
            .map(|(identity, share)| SharedApp {
                app_id: share.app_id.clone(),
                app_name: share.app.name.clone(),
                app_key: identity.key.clone(),
                level: share.grant.level,
                granted_at: share.grant.granted_at,
                last_used_at: share.grant.last_used_at,
                windows: inner
                    .entries
                    .values()
                    .filter(|e| {
                        e.grant.as_ref().is_some_and(|g| g.scope == GrantScope::App)
                            && AppIdentity::of(e).as_ref() == Some(identity)
                    })
                    .count() as u32,
            })
            .collect();
        out.sort_by(|a, b| {
            a.granted_at
                .cmp(&b.granted_at)
                .then(a.app_id.cmp(&b.app_id))
        });
        out
    }

    /// The entire screen, when it is shared.
    pub fn shared_screen(&self) -> Option<SharedScreen> {
        let inner = self.lock();
        let share = inner.screen.as_ref()?;
        Some(SharedScreen {
            level: share.grant.level,
            granted_at: share.grant.granted_at,
            last_used_at: share.grant.last_used_at,
            windows: inner
                .entries
                .values()
                .filter(|e| {
                    e.grant
                        .as_ref()
                        .is_some_and(|g| g.scope == GrantScope::Screen)
                })
                .count() as u32,
        })
    }

    /// Check a read of the entire screen may start: it is shared, and its
    /// grant has not lapsed. A lapsed one is ended here, what that ended
    /// returned alongside the refusal. Counts as use of the grant.
    pub fn begin_screen_read(
        &self,
        now: i64,
        ttl: Option<Duration>,
    ) -> Result<ScreenReadTicket, (ReadRefusal, AppChange)> {
        let mut inner = self.lock();
        let Inner {
            entries, screen, ..
        } = &mut *inner;
        let Some(share) = screen.as_ref() else {
            return Err((ReadRefusal::GrantRequired, AppChange::default()));
        };
        if share.grant.lapsed(now, ttl) {
            let ended = Self::end_screen(entries, screen, GrantChange::Expired);
            return Err((ReadRefusal::GrantRequired, ended));
        }
        let Some(share) = screen.as_mut() else {
            return Err((ReadRefusal::GrantRequired, AppChange::default()));
        };
        if !share.grant.level.allows(GrantLevel::Read) {
            return Err((ReadRefusal::GrantRequired, AppChange::default()));
        }
        share.grant.last_used_at = now;
        Ok(ScreenReadTicket { epoch: share.epoch })
    }

    /// Check a read of the entire screen that has finished may be handed
    /// over: the same sharing of it is still in force. Returns the
    /// generation that names the read; `mark` becomes the screen's latest
    /// picture under it — what later points on the screen are read in.
    pub fn finish_screen_read(
        &self,
        ticket: &ScreenReadTicket,
        mark: ReadMark,
    ) -> Result<String, ReadRefusal> {
        let mut inner = self.lock();
        let Some(share) = inner
            .screen
            .as_mut()
            .filter(|s| s.epoch == ticket.epoch && s.grant.level.allows(GrantLevel::Read))
        else {
            return Err(ReadRefusal::GrantRequired);
        };
        share.reads += 1;
        let generation = generation(share.epoch, share.reads);
        if let ReadMark::Capture {
            width,
            height,
            native_width,
            native_height,
            full_size,
            window_bounds,
        } = mark
        {
            share.capture_mark = Some(CaptureMark {
                generation: generation.clone(),
                width,
                height,
                native_width,
                native_height,
                full_size,
                window_bounds,
            });
        }
        Ok(generation)
    }

    /// Check an action on the entire screen may go ahead, and resolve it for
    /// the helper: the screen is shared for control, its grant has not
    /// lapsed, and the action is a click, a drag or a scroll at points of its
    /// latest picture as the agent was given it. Counts as use of the grant.
    pub fn begin_screen_act(
        &self,
        now: i64,
        ttl: Option<Duration>,
        request: &ComputerActRequest,
    ) -> Result<ScreenActTicket, (ActDenied, AppChange)> {
        let mut inner = self.lock();
        let Inner {
            entries, screen, ..
        } = &mut *inner;
        let Some(share) = screen.as_ref() else {
            return Err((ActDenied::GrantRequired, AppChange::default()));
        };
        if share.grant.lapsed(now, ttl) {
            let ended = Self::end_screen(entries, screen, GrantChange::Expired);
            return Err((ActDenied::GrantRequired, ended));
        }
        let Some(share) = screen.as_mut() else {
            return Err((ActDenied::GrantRequired, AppChange::default()));
        };
        if !share.grant.level.allows(GrantLevel::Read) {
            return Err((ActDenied::GrantRequired, AppChange::default()));
        }
        if !share.grant.level.allows(GrantLevel::Control) {
            return Err((ActDenied::ControlRequired, AppChange::default()));
        }
        let mark = share.capture_mark.as_ref();
        let action = resolve_on_screen(mark, request).map_err(|why| (why, AppChange::default()))?;
        // A point resolved, so the picture it was read in is there.
        let geometry = mark
            .map(|m| ScreenGeometry {
                scale: f64::from(m.native_width) / m.window_bounds.width,
                width: m.window_bounds.width,
                height: m.window_bounds.height,
            })
            .filter(|g| g.scale.is_finite() && g.scale > 0.0)
            .ok_or((ActDenied::NoPointing, AppChange::default()))?;
        share.grant.last_used_at = now;
        Ok(ScreenActTicket {
            epoch: share.epoch,
            action,
            geometry,
        })
    }

    /// Whether the sharing of the entire screen an action was let through
    /// under (`epoch`) is still in force, for control.
    pub fn screen_controlled(&self, epoch: u64) -> bool {
        self.lock()
            .screen
            .as_ref()
            .is_some_and(|s| s.epoch == epoch && s.grant.level.allows(GrantLevel::Control))
    }

    /// What `app` — as a listing of applications names it — is shared for as
    /// a whole; [`GrantLevel::None`] when it is not.
    pub fn app_level(&self, app: &RawApp) -> GrantLevel {
        self.lock()
            .apps
            .iter()
            .find(|(identity, _)| identity.names(app))
            .map_or(GrantLevel::None, |(_, share)| share.grant.level)
    }

    /// The share of the application `target_id` is a window of, if it is
    /// shared as a whole.
    pub fn app_share_of(&self, target_id: &str) -> Option<AppShare> {
        let inner = self.lock();
        let entry = inner.entries.get(target_id)?;
        let identity = AppIdentity::of(entry)?;
        inner.apps.get(&identity).cloned()
    }
}
