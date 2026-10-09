// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl ComputerService {
    /// The stamp of what an agent last put on the clipboard itself, while
    /// that may still be pasted or read back: written by an agent, or copied
    /// out of a window still shared for reading under the sharing it was
    /// copied under. Whether the clipboard still holds it the helper checks.
    pub(in crate::commands::computer) fn owned_clipboard(&self) -> Option<u64> {
        let owned = self
            .clipboard
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()?;
        if let Some((target_id, epoch)) = &owned.source {
            let entry = self.targets.get(target_id)?;
            let readable = entry
                .grant
                .as_ref()
                .is_some_and(|g| g.level.allows(GrantLevel::Read));
            if entry.epoch != *epoch || !readable {
                return None;
            }
        }
        Some(owned.stamp)
    }

    /// Hold `owned` as what an agent put on the clipboard — unless a Stop
    /// has come since `stop`: decided under the lock a Stop clears it under,
    /// so a reply that lands after a Stop never brings back what it cleared.
    pub(in crate::commands::computer) fn own_clipboard(
        &self,
        owned: OwnedClipboard,
        stop: u64,
    ) -> bool {
        let _gate = self.grant_gate.lock().unwrap_or_else(|p| p.into_inner());
        if self.stopped_since(stop) {
            return false;
        }
        *self.clipboard.lock().unwrap_or_else(|p| p.into_inner()) = Some(owned);
        true
    }

    /// What is shared now: every window, the applications shared as a
    /// whole, and the entire screen when it is.
    pub(in crate::commands::computer) fn shared_state(&self) -> SharedState {
        SharedState {
            shared: self.targets.shared(),
            apps: self.targets.shared_apps(),
            screen: self.targets.shared_screen(),
        }
    }

    /// The rules the helper judges every window on the screen by, as
    /// `config` makes them.
    pub(in crate::commands::computer) fn screen_rules(
        &self,
        config: &ComputerToolsConfig,
    ) -> ScreenRules {
        ScreenRules {
            me: self.me.clone(),
            blocklist: blocklist_of(config).entries().to_vec(),
        }
    }

    /// End the grants the settings as they are now no longer allow: lapsed,
    /// or on an application that has joined the blocklist.
    pub(in crate::commands::computer) async fn sweep(&self) {
        let config = self.config.snapshot().await;
        let ended =
            self.targets
                .sweep(now_ms(), config.grant_ttl, &self.me, &blocklist_of(&config));
        self.announce_change(ended);
        // An application that quit takes its share with it, though its
        // windows — gone with it — cannot say so.
        let quit = self
            .targets
            .prune_apps(|pid, started_at| process_start(pid) == Some(started_at));
        self.announce_change(quit);
    }

    /// Tell the panel about grant changes: each transition, then the state.
    pub(in crate::commands::computer) fn announce(&self, changes: &[ComputerGrantPayload]) {
        if changes.is_empty() {
            return;
        }
        for change in changes {
            self.events.grant(change);
        }
        self.emit_state();
    }

    /// [`announce`](Self::announce), for a change that may have moved an
    /// application's share too — which is state even with no window in it.
    pub(in crate::commands::computer) fn announce_change(&self, change: AppChange) {
        if change.app_changed && change.windows.is_empty() {
            self.emit_state();
        } else {
            self.announce(&change.windows);
        }
    }

    /// Tell the panels, and bring the strip and the marker in line: the
    /// strip is up while anything is shared (unless the person turned it
    /// off), the marker ready while anything is shared for control.
    pub(in crate::commands::computer) fn emit_state(&self) {
        let _told = self.state_gate.lock().unwrap_or_else(|p| p.into_inner());
        let shared = self.targets.shared();
        let apps = self.targets.shared_apps();
        let screen = self.targets.shared_screen();
        self.events.state(&shared, &apps, screen.as_ref());
        #[cfg(feature = "tauri-runtime")]
        if let Some(desktop) = &self.desktop {
            desktop.indicator.set(Strip::of(
                !shared.is_empty() || !apps.is_empty() || screen.is_some(),
                self.strip_wanted.load(Ordering::Acquire),
            ));
            desktop.marker.arm(
                shared.iter().any(|w| w.level == GrantLevel::Control)
                    || apps.iter().any(|a| a.level == GrantLevel::Control)
                    || screen.is_some_and(|s| s.level == GrantLevel::Control),
            );
        }
    }

    /// The person pressed Stop: every grant ends, whatever is under way is
    /// cut off, and the helper kills the driver — mid-action if it is in
    /// one. The count moves with the revocation, before anything is waited
    /// on, so nothing let through before this Stop goes out after it; and
    /// nothing is held after it: sharing a window again is the next step.
    pub async fn stop(&self) {
        let (ended, stop) = {
            let _gate = self.grant_gate.lock().unwrap_or_else(|p| p.into_inner());
            let stop = self.stops.fetch_add(1, Ordering::AcqRel) + 1;
            // The backend holds actions to it from this moment, not from
            // when its `Halt` goes out below.
            self.backend.note_stop(stop);
            // Nothing an agent copied before a Stop is pasted after it.
            *self.clipboard.lock().unwrap_or_else(|p| p.into_inner()) = None;
            (self.targets.revoke_all(GrantChange::Stopped), stop)
        };
        for change in &ended.windows {
            self.events.grant(change);
        }
        self.emit_state();
        if let Err(e) = self.backend.halt(stop).await {
            tracing::warn!("[computer] the helper did not confirm the stop: {e}");
        }
    }

    /// How many times the person has pressed Stop, for whatever begins now
    /// to be held to.
    pub(in crate::commands::computer) fn stop_count(&self) -> u64 {
        self.stops.load(Ordering::Acquire)
    }

    /// Whether a Stop has come since `stop` was counted.
    pub(in crate::commands::computer) fn stopped_since(&self, stop: u64) -> bool {
        self.stop_count() != stop
    }

    /// Share a window, unless computer use is off or the person has pressed
    /// Stop since the share began (`since`) — decided under the same lock a
    /// Stop and a settings change take to revoke, so no share lands between
    /// either and its revocation.
    pub(in crate::commands::computer) fn share_unless_stopped(
        &self,
        target_id: &str,
        level: GrantLevel,
        since: u64,
    ) -> Result<Option<ComputerGrantPayload>, AppCommandError> {
        let _gate = self.grant_gate.lock().unwrap_or_else(|p| p.into_inner());
        let policy = self.policy.lock().unwrap_or_else(|p| p.into_inner());
        if level != GrantLevel::None {
            if !policy.enabled {
                return Err(AppCommandError::configuration_invalid(
                    "computer use is switched off",
                ));
            }
            if self.stopped_since(since) {
                return Err(AppCommandError::configuration_invalid(
                    "Stop was pressed while this was being shared; share it again",
                ));
            }
        }
        match self
            .targets
            .share(target_id, level, now_ms(), &self.me, &policy.blocklist)
        {
            Ok(change) => Ok(change),
            Err(ShareError::NoSuchTarget) | Err(ShareError::Gone) => Err(
                AppCommandError::configuration_invalid("that window is gone; open the list again"),
            ),
            Err(ShareError::NotGrantable(why)) => {
                Err(AppCommandError::configuration_invalid(why.note()))
            }
            Err(ShareError::AppShared) => Err(AppCommandError::configuration_invalid(
                "that window is shared with its whole application; change the application's \
                 sharing instead",
            )),
            Err(ShareError::ScreenShared) => Err(screen_shared_error()),
        }
    }

    /// Share an application as a whole, or end its share — as
    /// [`share_unless_stopped`](Self::share_unless_stopped) shares a window,
    /// under the same lock and for the same reasons.
    pub(in crate::commands::computer) fn share_app_unless_stopped(
        &self,
        target: AppTarget<'_>,
        level: GrantLevel,
        since: u64,
    ) -> Result<AppChange, AppCommandError> {
        let _gate = self.grant_gate.lock().unwrap_or_else(|p| p.into_inner());
        let policy = self.policy.lock().unwrap_or_else(|p| p.into_inner());
        if level != GrantLevel::None {
            if !policy.enabled {
                return Err(AppCommandError::configuration_invalid(
                    "computer use is switched off",
                ));
            }
            if self.stopped_since(since) {
                return Err(AppCommandError::configuration_invalid(
                    "Stop was pressed while this was being shared; share it again",
                ));
            }
        }
        self.targets
            .share_app(target, level, now_ms(), &self.me, &policy.blocklist)
            .map_err(|e| match e {
                ShareError::NoSuchTarget | ShareError::Gone | ShareError::AppShared => {
                    AppCommandError::configuration_invalid(
                        "that application is gone; open the list again",
                    )
                }
                ShareError::NotGrantable(why) => AppCommandError::configuration_invalid(why.note()),
                ShareError::ScreenShared => screen_shared_error(),
            })
    }
}
