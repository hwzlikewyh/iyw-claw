// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl ComputerService {
    pub fn tools_enabled(&self) -> bool {
        self.config.is_served() && self.config.subscribe().borrow().enabled
    }

    pub(crate) fn operation_epoch(&self) -> u64 {
        self.stop_count()
    }

    pub(crate) fn settings_changes(&self) -> tokio::sync::watch::Receiver<ComputerToolsConfig> {
        self.config.subscribe()
    }

    pub async fn shutdown(&self) {
        self.stop().await;
        self.backend.close().await;
    }

    pub(crate) async fn resume_after_failed_update(&self) {
        if self.tools_enabled() {
            self.backend.open().await;
        }
    }

    pub async fn cancel_operation(&self) {
        let stop = self.stops.fetch_add(1, Ordering::AcqRel) + 1;
        self.backend.note_stop(stop);
        if let Err(error) = self.backend.halt(stop).await {
            tracing::warn!(%error, "[computer] operation cancellation was not confirmed");
        }
    }
    /// Build the service and start its duties: following the settings
    /// (switching off ends every grant and stops the helper; a longer
    /// blocklist or a shorter timeout ends what they now forbid), and ending
    /// grants whose time runs out.
    pub fn start(host: ComputerHost, config: ComputerToolsRuntimeConfig) -> Arc<Self> {
        let (events, settings_events) = match &host {
            #[cfg(feature = "tauri-runtime")]
            ComputerHost::Desktop(app) => (
                ComputerEvents::Desktop(app.clone()),
                EventEmitter::Tauri(app.clone()),
            ),
            ComputerHost::Server {
                broadcaster,
                emitter,
            } => (ComputerEvents::Web(broadcaster.clone()), emitter.clone()),
        };
        let status_events = events.clone();
        let drivers = Arc::new(DriverAdmin::new(events.clone()));
        let status_drivers = drivers.clone();
        let backend = Arc::new(
            LocalBackend::new(move |status: &BackendStatus| {
                status_events.backend_status(status);
                status_drivers.backend_moved(status);
            })
            .with_switch(config.clone()),
        );
        #[cfg(feature = "tauri-runtime")]
        let desktop = match host {
            ComputerHost::Desktop(app) => Some(DesktopUi {
                stop_key: StopKey::new(),
                indicator: Indicator::start(app.clone()),
                marker: Marker::start(app.clone()),
                app,
            }),
            ComputerHost::Server { .. } => None,
        };
        let (policy, strip_wanted) = {
            let settings = config.subscribe();
            let settings = settings.borrow();
            (SharingPolicy::of(&settings), settings.show_indicator)
        };
        config.mark_served();
        let service = Arc::new(Self {
            events,
            settings_events,
            #[cfg(feature = "tauri-runtime")]
            desktop,
            backend,
            targets: TargetTable::new(),
            config: config.clone(),
            me: SelfIdentity::current(),
            turn: tokio::sync::Mutex::new(()),
            stops: AtomicU64::new(0),
            grant_gate: std::sync::Mutex::new(()),
            policy: std::sync::Mutex::new(policy),
            strip_wanted: AtomicBool::new(strip_wanted),
            state_gate: std::sync::Mutex::new(()),
            drivers,
            clipboard: std::sync::Mutex::new(None),
        });

        // What a change takes away is taken before the write that made it
        // returns (see `ComputerToolsRuntimeConfig::on_change`).
        let hook = Arc::downgrade(&service);
        config.on_change(move |before, after| {
            if let Some(service) = hook.upgrade() {
                service.policy_changed(before, after);
            }
        });

        let watcher = Arc::downgrade(&service);
        let mut changes = config.subscribe();
        spawn_task(async move {
            // The settings as they stand, then every change to them. A watch
            // channel keeps only the latest value, so an off-and-on-again is
            // told apart by the switch-off count, not by `enabled`.
            let mut seen = changes.borrow_and_update().clone();
            if let Some(service) = watcher.upgrade() {
                service.follow(&seen, false).await;
            }
            while changes.changed().await.is_ok() {
                let next = changes.borrow_and_update().clone();
                let Some(service) = watcher.upgrade() else {
                    break;
                };
                service
                    .follow(&next, next.switched_off != seen.switched_off)
                    .await;
                seen = next;
            }
        });

        let sweeper = Arc::downgrade(&service);
        spawn_task(async move {
            let mut tick = tokio::time::interval(EXPIRY_SWEEP);
            loop {
                tick.tick().await;
                let Some(service) = sweeper.upgrade() else {
                    break;
                };
                service.sweep().await;
            }
        });
        service
    }

    /// The settings just changed, from `before` to `after`: end the grants
    /// the change takes away. Runs inside the write, once per change, so an
    /// entry added to the blocklist and taken off again straight after still
    /// ended the grants it named, and no read admitted after the write can
    /// use a grant the write ended.
    pub(in crate::commands::computer) fn policy_changed(
        &self,
        before: &ComputerToolsConfig,
        after: &ComputerToolsConfig,
    ) {
        let ended = {
            let _gate = self.grant_gate.lock().unwrap_or_else(|p| p.into_inner());
            *self.policy.lock().unwrap_or_else(|p| p.into_inner()) = SharingPolicy::of(after);
            if before.enabled && !after.enabled {
                self.targets.revoke_all(GrantChange::Disabled)
            } else {
                let mut ended =
                    self.targets
                        .sweep(now_ms(), after.grant_ttl, &self.me, &blocklist_of(after));
                // The entire screen is shared only while its switch is on.
                if !after.screen_enabled {
                    ended.absorb(self.targets.end_screen_share(GrantChange::Disabled));
                }
                ended
            }
        };
        self.announce_change(ended);
    }

    /// Bring the helper and the stop shortcut in line with `config`.
    /// `went_off`: the switch was off at some point since the last call, even
    /// if it is on again now — the helper (and the driver under it) stops,
    /// and is not started again while the switch is off.
    pub(in crate::commands::computer) async fn follow(
        self: &Arc<Self>,
        config: &ComputerToolsConfig,
        went_off: bool,
    ) {
        self.follow_stop_key(config);
        self.follow_strip(config.show_indicator);
        if went_off || !config.enabled {
            self.backend.close().await;
        }
        if config.enabled {
            self.backend.open().await;
        }
    }

    /// Hold the chosen stop shortcut with the OS while computer use is on —
    /// off, there is nothing for it to stop, and it would only take the keys
    /// from every other application.
    pub(in crate::commands::computer) fn follow_stop_key(
        self: &Arc<Self>,
        config: &ComputerToolsConfig,
    ) {
        #[cfg(feature = "tauri-runtime")]
        if let Some(desktop) = &self.desktop {
            let wanted = config
                .enabled
                .then_some(config.stop_shortcut.as_ref())
                .flatten();
            let service = Arc::downgrade(self);
            let on_press = move || {
                if let Some(service) = service.upgrade() {
                    spawn_task(async move { service.stop().await });
                }
            };
            if let Some(status) = desktop.stop_key.sync(&desktop.app, wanted, on_press) {
                self.events.stop_key(&status);
            }
        }
        #[cfg(not(feature = "tauri-runtime"))]
        let _ = config;
    }

    /// Put the strip up or down for the person's choice in Settings — the
    /// sharing it follows is unchanged.
    pub(in crate::commands::computer) fn follow_strip(&self, wanted: bool) {
        let _told = self.state_gate.lock().unwrap_or_else(|p| p.into_inner());
        self.strip_wanted.store(wanted, Ordering::Release);
        #[cfg(feature = "tauri-runtime")]
        if let Some(desktop) = &self.desktop {
            let shared = !self.targets.shared().is_empty()
                || !self.targets.shared_apps().is_empty()
                || self.targets.shared_screen().is_some();
            desktop.indicator.set(Strip::of(shared, wanted));
        }
    }

    pub fn stop_key_status(&self) -> StopKeyStatus {
        #[cfg(feature = "tauri-runtime")]
        if let Some(desktop) = &self.desktop {
            return desktop.stop_key.status();
        }
        StopKeyStatus::default()
    }
}
