// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl TargetTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub(in crate::computer::targets) fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        // A panic while holding this lock cannot leave a half-written entry
        // (every mutation is a field store), so a poisoned lock is still a
        // consistent table.
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Take in a fresh listing: name every window in it, update what iyw-claw
    /// knows about the ones it had named, and let go of the rest.
    ///
    /// `scope_pid` is the filter the listing was made with. Only windows in
    /// that scope can be judged missing from it — a listing of one
    /// application's windows says nothing about anyone else's.
    ///
    /// Returns the entries for the listed windows, in listing order, and the
    /// grants that changed: ended because their window is gone, or begun
    /// because the window's application is shared as a whole.
    pub fn observe(
        &self,
        windows: &[RawWindow],
        scope_pid: Option<u32>,
    ) -> (Vec<TargetEntry>, Vec<ComputerGrantPayload>) {
        let mut inner = self.lock();
        let mut seen: Vec<String> = Vec::with_capacity(windows.len());
        for window in windows {
            let identity = WindowIdentity::of(window);
            let target_id = match inner.by_identity.get(&identity) {
                Some(id) => id.clone(),
                None => {
                    inner.next_id += 1;
                    let id = format!("w{}", inner.next_id);
                    inner.by_identity.insert(identity, id.clone());
                    inner.entries.insert(
                        id.clone(),
                        TargetEntry {
                            target_id: id.clone(),
                            identity,
                            app: window.app.clone(),
                            title: String::new(),
                            bounds: Rect::default(),
                            on_screen: false,
                            minimized: None,
                            hidden: None,
                            on_current_space: None,
                            grant: None,
                            epoch: 0,
                            reads: 0,
                            snapshot_mark: None,
                            capture_mark: None,
                            gone: false,
                        },
                    );
                    id
                }
            };
            if let Some(entry) = inner.entries.get_mut(&target_id) {
                entry.app = window.app.clone();
                // An empty title is the platform withholding it (no Screen
                // Recording yet), not the window losing its name: keep the
                // last one the person could have seen.
                if !window.title.is_empty() {
                    entry.title = window.title.clone();
                }
                entry.bounds = window.bounds;
                entry.on_screen = window.on_screen;
                entry.minimized = window.minimized;
                entry.hidden = window.hidden;
                entry.on_current_space = window.on_current_space;
            }
            seen.push(target_id);
        }

        let in_scope = |entry: &TargetEntry| scope_pid.is_none_or(|pid| entry.identity.pid == pid);
        let missing: Vec<String> = inner
            .entries
            .values()
            .filter(|e| !e.gone && in_scope(e) && !seen.contains(&e.target_id))
            .map(|e| e.target_id.clone())
            .collect();
        let mut ended = Vec::new();
        for id in missing {
            if let Some(payload) = Self::retire(&mut inner, &id, GrantChange::TargetChanged) {
                ended.push(payload);
            }
        }
        // A window of an application shared as a whole is shared with it as
        // soon as a listing finds it a window a person could mean; with the
        // entire screen shared, so is any window the rules allow.
        {
            let Inner {
                entries,
                apps,
                screen,
                ..
            } = &mut *inner;
            for id in &seen {
                let Some(entry) = entries.get_mut(id) else {
                    continue;
                };
                if entry.grant.is_some() || entry.gone || !entry.worth_listing() {
                    continue;
                }
                if let Some(share) = AppIdentity::of(entry).and_then(|app| apps.get(&app)) {
                    ended.extend(Self::grant_with(entry, &share.grant));
                } else if let Some(share) = screen.as_ref() {
                    if grantable(&entry.app, &share.me, &share.blocklist).is_ok() {
                        ended.extend(Self::grant_with(entry, &share.grant));
                    }
                }
            }
        }

        let entries = seen
            .iter()
            .filter_map(|id| inner.entries.get(id).cloned())
            .collect();
        (entries, ended)
    }

    /// A window is gone. A shared one keeps a grant-less entry (see
    /// [`TargetEntry::gone`]); an unshared one is forgotten.
    ///
    /// Idempotent, and it touches only what is this entry's own: a late
    /// "window gone" for an id that has already been retired — an operation
    /// that started before a listing retired it — leaves the entry, and the
    /// identity now named by a newer id, alone.
    pub(in crate::computer::targets) fn retire(
        inner: &mut Inner,
        target_id: &str,
        change: GrantChange,
    ) -> Option<ComputerGrantPayload> {
        let entry = inner.entries.get_mut(target_id)?;
        if entry.gone {
            return None;
        }
        let identity = entry.identity;
        if inner.by_identity.get(&identity).map(String::as_str) == Some(target_id) {
            inner.by_identity.remove(&identity);
        }
        let entry = inner.entries.get_mut(target_id)?;
        if entry.grant.is_some() {
            entry.grant = None;
            entry.gone = true;
            entry.epoch += 1;
            entry.snapshot_mark = None;
            entry.capture_mark = None;
            Some(ComputerGrantPayload {
                target_id: target_id.to_string(),
                change,
                level: GrantLevel::None,
            })
        } else {
            inner.entries.remove(target_id);
            None
        }
    }

    /// The window a target id names, if iyw-claw named one.
    pub fn get(&self, target_id: &str) -> Option<TargetEntry> {
        self.lock().entries.get(target_id).cloned()
    }

    /// Share a window at `level`, or stop sharing it at [`GrantLevel::None`].
    ///
    /// `Ok(None)` when nothing changed — the window was already at that level
    /// — so the caller neither emits an event nor restarts the idle clock. A
    /// window shared with its whole application changes only with it
    /// ([`ShareError::AppShared`]); every window, while the entire screen is
    /// shared, only with the screen ([`ShareError::ScreenShared`]).
    pub fn share(
        &self,
        target_id: &str,
        level: GrantLevel,
        now: i64,
        me: &SelfIdentity,
        blocklist: &Blocklist,
    ) -> Result<Option<ComputerGrantPayload>, ShareError> {
        let mut inner = self.lock();
        let Inner {
            entries,
            apps,
            screen,
            ..
        } = &mut *inner;
        if screen.is_some() {
            return Err(ShareError::ScreenShared);
        }
        let entry = entries.get_mut(target_id).ok_or(ShareError::NoSuchTarget)?;
        let with_app = entry
            .grant
            .as_ref()
            .is_some_and(|g| g.scope == GrantScope::App)
            || (!entry.gone && AppIdentity::of(entry).is_some_and(|app| apps.contains_key(&app)));
        if with_app {
            return Err(ShareError::AppShared);
        }
        if level == GrantLevel::None {
            return Ok(Self::revoke_entry(entry, GrantChange::Revoked));
        }
        if entry.gone {
            return Err(ShareError::Gone);
        }
        grantable(&entry.app, me, blocklist).map_err(ShareError::NotGrantable)?;
        if level_of(entry.grant.as_ref()) == level {
            return Ok(None);
        }
        match entry.grant.as_mut() {
            // A change of level on a live grant is the same grant: the reads
            // already made under it stay valid, and the clock keeps running
            // from the last one.
            Some(grant) => grant.level = level,
            None => {
                entry.grant = Some(ComputerGrant::new(level, now));
                entry.epoch += 1;
                entry.reads = 0;
                entry.snapshot_mark = None;
                entry.capture_mark = None;
            }
        }
        Ok(Some(ComputerGrantPayload {
            target_id: target_id.to_string(),
            change: GrantChange::Granted,
            level,
        }))
    }
}
