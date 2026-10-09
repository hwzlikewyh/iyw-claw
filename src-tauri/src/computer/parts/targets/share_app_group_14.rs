// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl TargetTable {
    /// Share an application as a whole at `level` — the one `target` names —
    /// or end its share at [`GrantLevel::None`]. Every window of it iyw-claw has
    /// named and a person could mean takes the application's grant at its
    /// level, one shared on its own before included; the ones it opens later
    /// take it as listings find them. Ending it ends every window's share of
    /// it. Nothing changes while the entire screen is shared
    /// ([`ShareError::ScreenShared`]).
    pub fn share_app(
        &self,
        target: AppTarget<'_>,
        level: GrantLevel,
        now: i64,
        me: &SelfIdentity,
        blocklist: &Blocklist,
    ) -> Result<AppChange, ShareError> {
        let mut inner = self.lock();
        let Inner {
            entries,
            apps,
            next_app_id,
            screen,
            ..
        } = &mut *inner;
        if screen.is_some() {
            return Err(ShareError::ScreenShared);
        }
        let (identity, app) = match target {
            AppTarget::Window(target_id) => {
                let entry = entries.get(target_id).ok_or(ShareError::NoSuchTarget)?;
                if entry.gone {
                    return Err(ShareError::Gone);
                }
                let identity = AppIdentity::of(entry)
                    .ok_or(ShareError::NotGrantable(NotGrantable::Unidentified))?;
                (identity, entry.app.clone())
            }
            AppTarget::Share(app_id) => match apps.iter().find(|(_, s)| s.app_id == app_id) {
                Some((identity, share)) => (identity.clone(), share.app.clone()),
                None if level == GrantLevel::None => return Ok(AppChange::default()),
                None => return Err(ShareError::NoSuchTarget),
            },
        };
        if level == GrantLevel::None {
            return Ok(Self::end_app(
                entries,
                apps,
                &identity,
                GrantChange::Revoked,
            ));
        }
        grantable(&app, me, blocklist).map_err(ShareError::NotGrantable)?;
        let app_changed = match apps.get_mut(&identity) {
            Some(share) if share.grant.level == level => false,
            // The same share at another level: its clock keeps running.
            Some(share) => {
                share.grant.level = level;
                true
            }
            None => {
                *next_app_id += 1;
                apps.insert(
                    identity.clone(),
                    AppShare {
                        app_id: format!("a{next_app_id}"),
                        grant: ComputerGrant::of_app(level, now),
                        app,
                    },
                );
                true
            }
        };
        let Some(share) = apps.get(&identity) else {
            return Ok(AppChange::default());
        };
        let windows = entries
            .values_mut()
            .filter(|e| !e.gone && (e.grant.is_some() || e.worth_listing()))
            .filter(|e| AppIdentity::of(e).as_ref() == Some(&identity))
            .filter_map(|e| Self::grant_with(e, &share.grant))
            .collect();
        Ok(AppChange {
            windows,
            app_changed,
        })
    }

    /// Give `entry` its share of a grant on more than itself — its whole
    /// application's, or the entire screen's — at that grant's level, in its
    /// scope and on its clock. A window already shared keeps the reads made
    /// under its grant, as a change of level does.
    pub(in crate::computer::targets) fn grant_with(
        entry: &mut TargetEntry,
        shared: &ComputerGrant,
    ) -> Option<ComputerGrantPayload> {
        let (level, scope) = (shared.level, shared.scope);
        match entry.grant.as_mut() {
            Some(grant) if grant.scope == scope && grant.level == level => return None,
            Some(grant) => {
                grant.level = level;
                grant.scope = scope;
            }
            None => {
                entry.grant = Some(shared.clone());
                entry.epoch += 1;
                entry.reads = 0;
                entry.snapshot_mark = None;
                entry.capture_mark = None;
            }
        }
        Some(ComputerGrantPayload {
            target_id: entry.target_id.clone(),
            change: GrantChange::Granted,
            level,
        })
    }

    /// End the share of application `identity`, and every window's share of
    /// it.
    pub(in crate::computer::targets) fn end_app(
        entries: &mut HashMap<String, TargetEntry>,
        apps: &mut HashMap<AppIdentity, AppShare>,
        identity: &AppIdentity,
        change: GrantChange,
    ) -> AppChange {
        let app_changed = apps.remove(identity).is_some();
        let windows = entries
            .values_mut()
            .filter(|e| AppIdentity::of(e).as_ref() == Some(identity))
            .filter_map(|e| Self::revoke_entry(e, change))
            .collect();
        AppChange {
            windows,
            app_changed,
        }
    }

    /// Share the entire screen at `level`, or end its share at
    /// [`GrantLevel::None`]. Every window iyw-claw has named that a person could
    /// mean and the rules allow takes the screen's grant at its level — one
    /// shared on its own, or with its application, included: the
    /// applications shared as a whole are shared with the screen from then
    /// on, and their own shares end. The windows that come up later take it
    /// as listings find them. Ending it ends every window's share of it.
    pub fn share_screen(
        &self,
        level: GrantLevel,
        now: i64,
        me: &SelfIdentity,
        blocklist: &Blocklist,
    ) -> AppChange {
        let mut inner = self.lock();
        let Inner {
            entries,
            apps,
            screen,
            screen_epochs,
            ..
        } = &mut *inner;
        if level == GrantLevel::None {
            return Self::end_screen(entries, screen, GrantChange::Revoked);
        }
        let mut out = AppChange::default();
        match screen.as_mut() {
            Some(share) => {
                // The same sharing at another level: its clock keeps running.
                if share.grant.level != level {
                    share.grant.level = level;
                    out.app_changed = true;
                }
                share.me = me.clone();
                share.blocklist = blocklist.clone();
            }
            None => {
                *screen_epochs += 1;
                *screen = Some(ScreenShare {
                    grant: ComputerGrant::of_screen(level, now),
                    epoch: *screen_epochs,
                    reads: 0,
                    capture_mark: None,
                    me: me.clone(),
                    blocklist: blocklist.clone(),
                });
                out.app_changed = true;
            }
        }
        if !apps.is_empty() {
            apps.clear();
            out.app_changed = true;
        }
        let Some(share) = screen.as_ref() else {
            return out;
        };
        for entry in entries.values_mut() {
            let shareable = !entry.gone
                && (entry.grant.is_some() || entry.worth_listing())
                && grantable(&entry.app, me, blocklist).is_ok();
            if shareable {
                out.windows.extend(Self::grant_with(entry, &share.grant));
            }
        }
        out
    }

    /// End the share of the entire screen, for `change` — the switch for it
    /// turned off — and every window's share of it.
    pub fn end_screen_share(&self, change: GrantChange) -> AppChange {
        let mut inner = self.lock();
        let Inner {
            entries, screen, ..
        } = &mut *inner;
        Self::end_screen(entries, screen, change)
    }

    /// End the share of the entire screen, and every window's share of it.
    pub(in crate::computer::targets) fn end_screen(
        entries: &mut HashMap<String, TargetEntry>,
        screen: &mut Option<ScreenShare>,
        change: GrantChange,
    ) -> AppChange {
        let app_changed = screen.take().is_some();
        let windows = entries
            .values_mut()
            .filter(|e| {
                e.grant
                    .as_ref()
                    .is_some_and(|g| g.scope == GrantScope::Screen)
            })
            .filter_map(|e| Self::revoke_entry(e, change))
            .collect();
        AppChange {
            windows,
            app_changed,
        }
    }
}
