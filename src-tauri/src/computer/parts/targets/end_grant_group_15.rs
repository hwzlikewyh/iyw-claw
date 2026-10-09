// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl TargetTable {
    /// End the grant `target_id` holds: its own, or — for a window shared
    /// with its whole application — the application's, with every window's
    /// share of it. A window shared with the entire screen loses its share of
    /// it alone, unless the screen's own time is up (`Expired`), which ends
    /// the screen's share.
    pub(in crate::computer::targets) fn end_grant(
        entries: &mut HashMap<String, TargetEntry>,
        apps: &mut HashMap<AppIdentity, AppShare>,
        screen: &mut Option<ScreenShare>,
        target_id: &str,
        change: GrantChange,
    ) -> Vec<ComputerGrantPayload> {
        let Some(entry) = entries.get_mut(target_id) else {
            return Vec::new();
        };
        match entry.grant.as_ref().map(|g| g.scope) {
            Some(GrantScope::App) => match AppIdentity::of(entry) {
                Some(app) => Self::end_app(entries, apps, &app, change).windows,
                None => Self::revoke_entry(entry, change).into_iter().collect(),
            },
            Some(GrantScope::Screen) if change == GrantChange::Expired => {
                Self::end_screen(entries, screen, change).windows
            }
            _ => Self::revoke_entry(entry, change).into_iter().collect(),
        }
    }

    pub(in crate::computer::targets) fn revoke_entry(
        entry: &mut TargetEntry,
        change: GrantChange,
    ) -> Option<ComputerGrantPayload> {
        entry.grant.take()?;
        entry.epoch += 1;
        entry.snapshot_mark = None;
        entry.capture_mark = None;
        Some(ComputerGrantPayload {
            target_id: entry.target_id.clone(),
            change,
            level: GrantLevel::None,
        })
    }

    /// End every grant, for one reason — the applications shared as a whole
    /// and the entire screen with them. Used when the user switches computer
    /// use off, which is a statement about every window at once, and for
    /// Stop.
    pub fn revoke_all(&self, change: GrantChange) -> AppChange {
        let mut inner = self.lock();
        let app_changed = !inner.apps.is_empty() || inner.screen.is_some();
        inner.apps.clear();
        inner.screen = None;
        let windows = inner
            .entries
            .values_mut()
            .filter_map(|entry| Self::revoke_entry(entry, change))
            .collect();
        AppChange {
            windows,
            app_changed,
        }
    }

    /// The window is not the one that was shared any more (it closed, or its
    /// process is gone). Ends its grant, if it had one.
    pub fn target_changed(&self, target_id: &str) -> Option<ComputerGrantPayload> {
        let mut inner = self.lock();
        Self::retire(&mut inner, target_id, GrantChange::TargetChanged)
    }

    /// End every grant that no longer holds by the rules as they are now: gone
    /// unused for `ttl`, or on a window that can no longer be shared (its
    /// application joined the blocklist). Run before anything is listed, when
    /// the settings change and on a timer, so what an agent sees of a window
    /// never reflects a grant that has already ended. An application shared
    /// as a whole goes on its own clock, which its windows share; so does the
    /// entire screen, which follows the rules as they are now from here on.
    pub fn sweep(
        &self,
        now: i64,
        ttl: Option<Duration>,
        me: &SelfIdentity,
        blocklist: &Blocklist,
    ) -> AppChange {
        let mut inner = self.lock();
        let Inner {
            entries,
            apps,
            screen,
            ..
        } = &mut *inner;
        let ending: Vec<(AppIdentity, GrantChange)> = apps
            .iter()
            .filter_map(|(identity, share)| {
                if grantable(&share.app, me, blocklist).is_err() {
                    Some((identity.clone(), GrantChange::Revoked))
                } else if share.grant.lapsed(now, ttl) {
                    Some((identity.clone(), GrantChange::Expired))
                } else {
                    None
                }
            })
            .collect();
        let mut out = AppChange::default();
        for (identity, change) in ending {
            out.absorb(Self::end_app(entries, apps, &identity, change));
        }
        if screen.as_ref().is_some_and(|s| s.grant.lapsed(now, ttl)) {
            out.absorb(Self::end_screen(entries, screen, GrantChange::Expired));
        } else if let Some(share) = screen.as_mut() {
            share.me = me.clone();
            share.blocklist = blocklist.clone();
        }
        for entry in entries.values_mut() {
            let Some(grant) = entry.grant.as_ref() else {
                continue;
            };
            let change = if grantable(&entry.app, me, blocklist).is_err() {
                Some(GrantChange::Revoked)
            } else if grant.scope == GrantScope::App {
                // A share left of an application whose own has ended.
                let shared = AppIdentity::of(entry).is_some_and(|app| apps.contains_key(&app));
                (!shared).then_some(GrantChange::Revoked)
            } else if grant.scope == GrantScope::Screen {
                // A share left of a screen whose own has ended.
                screen.is_none().then_some(GrantChange::Revoked)
            } else if grant.lapsed(now, ttl) {
                Some(GrantChange::Expired)
            } else {
                None
            };
            if let Some(change) = change {
                out.windows.extend(Self::revoke_entry(entry, change));
            }
        }
        out
    }

    /// End the shares of applications that no longer run — `alive` says
    /// whether a process run still does. Their windows have gone with them;
    /// this is the application's own share, which would otherwise wait out
    /// its clock.
    pub fn prune_apps(&self, alive: impl Fn(u32, u64) -> bool) -> AppChange {
        let mut inner = self.lock();
        let Inner { entries, apps, .. } = &mut *inner;
        let quit: Vec<AppIdentity> = apps
            .keys()
            .filter(|app| {
                !alive(app.pid, app.started_at)
                    || app
                        .content
                        .is_some_and(|run| !alive(run.pid, run.started_at))
            })
            .cloned()
            .collect();
        let mut out = AppChange::default();
        for identity in quit {
            out.absorb(Self::end_app(
                entries,
                apps,
                &identity,
                GrantChange::TargetChanged,
            ));
        }
        out
    }

    /// Whether `entry`'s grant has lapsed: on its own clock, or — shared
    /// with its whole application, or the entire screen — on the
    /// application's or the screen's (a share whose own has ended has ended
    /// with it).
    pub(in crate::computer::targets) fn lapsed(
        entry: &TargetEntry,
        apps: &HashMap<AppIdentity, AppShare>,
        screen: &Option<ScreenShare>,
        now: i64,
        ttl: Option<Duration>,
    ) -> bool {
        match entry.grant.as_ref() {
            None => false,
            Some(grant) if grant.scope == GrantScope::App => AppIdentity::of(entry)
                .and_then(|app| apps.get(&app))
                .is_none_or(|share| share.grant.lapsed(now, ttl)),
            Some(grant) if grant.scope == GrantScope::Screen => screen
                .as_ref()
                .is_none_or(|share| share.grant.lapsed(now, ttl)),
            Some(grant) => grant.lapsed(now, ttl),
        }
    }

    /// A read or an action used `entry`'s grant now: its clock, and its
    /// application's or the screen's when it is shared with it, start again.
    pub(in crate::computer::targets) fn used(
        entry: &mut TargetEntry,
        apps: &mut HashMap<AppIdentity, AppShare>,
        screen: &mut Option<ScreenShare>,
        now: i64,
    ) {
        let Some(grant) = entry.grant.as_mut() else {
            return;
        };
        grant.last_used_at = now;
        match grant.scope {
            GrantScope::App => {
                if let Some(share) = AppIdentity::of(entry).and_then(|app| apps.get_mut(&app)) {
                    share.grant.last_used_at = now;
                }
            }
            GrantScope::Screen => {
                if let Some(share) = screen.as_mut() {
                    share.grant.last_used_at = now;
                }
            }
            GrantScope::Window => {}
        }
    }
}
