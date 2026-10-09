// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl TargetTable {
    /// Check a read may start: the window is one iyw-claw named, it is shared,
    /// and the grant has not lapsed. A lapsed grant is ended here and its
    /// payloads returned alongside the refusal, because the caller is the one
    /// holding an emitter.
    pub fn begin_read(
        &self,
        target_id: &str,
        now: i64,
        ttl: Option<Duration>,
        me: &SelfIdentity,
        blocklist: &Blocklist,
    ) -> Result<ReadTicket, (ReadRefusal, Vec<ComputerGrantPayload>)> {
        let mut inner = self.lock();
        let Inner {
            entries,
            apps,
            screen,
            ..
        } = &mut *inner;
        let Some(entry) = entries.get(target_id) else {
            return Err((ReadRefusal::NoSuchTarget, Vec::new()));
        };
        // Checked even for a window that holds a grant: the blocklist can grow
        // while a window is shared, and the list is what the user said last.
        if let Err(why) = grantable(&entry.app, me, blocklist) {
            let ended = Self::end_grant(entries, apps, screen, target_id, GrantChange::Revoked);
            return Err((ReadRefusal::NotGrantable(why), ended));
        }
        let Some(grant) = entry.grant.as_ref() else {
            return Err((ReadRefusal::GrantRequired, Vec::new()));
        };
        let (level, scope) = (grant.level, grant.scope);
        if Self::lapsed(entry, apps, screen, now, ttl) {
            let ended = Self::end_grant(entries, apps, screen, target_id, GrantChange::Expired);
            return Err((ReadRefusal::GrantRequired, ended));
        }
        if !level.allows(GrantLevel::Read) {
            return Err((ReadRefusal::GrantRequired, Vec::new()));
        }
        let Some(entry) = entries.get_mut(target_id) else {
            return Err((ReadRefusal::NoSuchTarget, Vec::new()));
        };
        Self::used(entry, apps, screen, now);
        Ok(ReadTicket {
            target_id: entry.target_id.clone(),
            identity: entry.identity,
            epoch: entry.epoch,
            app: entry.app.clone(),
            bounds: entry.bounds,
            scope,
        })
    }

    /// Check a read that has finished may be handed over: the same grant is
    /// still in force on the same window, and the window may still be shared
    /// by the rules as they are now. The person may have taken the grant back,
    /// or put the application on the blocklist, while the capture was in
    /// flight, and what the capture holds is exactly what they took back. A
    /// grant the blocklist now forbids is ended here, its payloads returned
    /// for the caller to announce.
    ///
    /// Returns the generation that names this read. `mark`, when the read
    /// leaves one, becomes the window's latest snapshot or screenshot under
    /// that generation — what later actions resolve refs and points against.
    pub fn finish_read(
        &self,
        ticket: &ReadTicket,
        me: &SelfIdentity,
        blocklist: &Blocklist,
        mark: Option<ReadMark>,
    ) -> Result<String, (ReadRefusal, Vec<ComputerGrantPayload>)> {
        let mut inner = self.lock();
        let Inner {
            entries,
            apps,
            screen,
            ..
        } = &mut *inner;
        let Some(entry) = entries.get(&ticket.target_id) else {
            return Err((ReadRefusal::GrantRequired, Vec::new()));
        };
        let still = entry.identity == ticket.identity
            && entry.epoch == ticket.epoch
            && level_of(entry.grant.as_ref()).allows(GrantLevel::Read);
        if !still {
            return Err((ReadRefusal::GrantRequired, Vec::new()));
        }
        if let Err(why) = grantable(&entry.app, me, blocklist) {
            let ended = Self::end_grant(
                entries,
                apps,
                screen,
                &ticket.target_id,
                GrantChange::Revoked,
            );
            return Err((ReadRefusal::NotGrantable(why), ended));
        }
        let Some(entry) = entries.get_mut(&ticket.target_id) else {
            return Err((ReadRefusal::GrantRequired, Vec::new()));
        };
        entry.reads += 1;
        let generation = generation(entry.epoch, entry.reads);
        match mark {
            Some(ReadMark::Snapshot {
                snapshot_id,
                shown,
                cut,
                secret,
            }) => {
                entry.snapshot_mark = Some(SnapshotMark {
                    generation: generation.clone(),
                    snapshot_id,
                    shown,
                    cut,
                    secret,
                })
            }
            Some(ReadMark::Capture {
                width,
                height,
                native_width,
                native_height,
                full_size,
                window_bounds,
            }) => {
                entry.capture_mark = Some(CaptureMark {
                    generation: generation.clone(),
                    width,
                    height,
                    native_width,
                    native_height,
                    full_size,
                    window_bounds,
                })
            }
            None => {}
        }
        Ok(generation)
    }

    /// Check an action may go ahead, and resolve it for the helper.
    ///
    /// In this order, each answered before the next is asked: the window is
    /// one iyw-claw named; it may still be shared at all; it is shared; the grant
    /// has not lapsed; it is shared for control — and only then anything about
    /// the action itself: keys and menus the grant does not reach (a window's,
    /// or its whole application's), then every ref against the window's
    /// latest snapshot and every point against its latest screenshot, as the
    /// agent was given them. A refusal therefore never says more about a
    /// window than the agent was allowed to know.
    ///
    /// Counts as use of the grant, like a read.
    // One argument per thing the decision reads, as `begin_read` takes them,
    // and whether a paste may go (`resolve`).
    #[allow(clippy::too_many_arguments)]
    pub fn begin_act(
        &self,
        target_id: &str,
        now: i64,
        ttl: Option<Duration>,
        me: &SelfIdentity,
        blocklist: &Blocklist,
        request: &ComputerActRequest,
        paste_ok: bool,
    ) -> Result<ActTicket, (ActDenied, Vec<ComputerGrantPayload>)> {
        let mut inner = self.lock();
        let Inner {
            entries,
            apps,
            screen,
            ..
        } = &mut *inner;
        let Some(entry) = entries.get(target_id) else {
            return Err((ActDenied::NoSuchTarget, Vec::new()));
        };
        if let Err(why) = grantable(&entry.app, me, blocklist) {
            let ended = Self::end_grant(entries, apps, screen, target_id, GrantChange::Revoked);
            return Err((ActDenied::NotGrantable(why), ended));
        }
        let Some(grant) = entry.grant.as_ref() else {
            return Err((ActDenied::GrantRequired, Vec::new()));
        };
        let level = grant.level;
        if Self::lapsed(entry, apps, screen, now, ttl) {
            let ended = Self::end_grant(entries, apps, screen, target_id, GrantChange::Expired);
            return Err((ActDenied::GrantRequired, ended));
        }
        if !level.allows(GrantLevel::Read) {
            return Err((ActDenied::GrantRequired, Vec::new()));
        }
        if !level.allows(GrantLevel::Control) {
            return Err((ActDenied::ControlRequired, Vec::new()));
        }
        let action = resolve(entry, request, paste_ok).map_err(|why| (why, Vec::new()))?;
        let Some(entry) = entries.get_mut(target_id) else {
            return Err((ActDenied::NoSuchTarget, Vec::new()));
        };
        Self::used(entry, apps, screen, now);
        Ok(ActTicket {
            target_id: entry.target_id.clone(),
            identity: entry.identity,
            epoch: entry.epoch,
            app: entry.app.clone(),
            aim: Aim::of(entry, &action),
            action,
        })
    }
}
