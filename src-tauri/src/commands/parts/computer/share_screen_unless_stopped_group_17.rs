// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl ComputerService {
    /// Share the entire screen, or end its share — as
    /// [`share_unless_stopped`](Self::share_unless_stopped) shares a window,
    /// under the same lock and for the same reasons, and only where it is
    /// offered: macOS and Windows, with its switch on.
    pub(in crate::commands::computer) fn share_screen_unless_stopped(
        &self,
        level: GrantLevel,
        since: u64,
    ) -> Result<AppChange, AppCommandError> {
        let _gate = self.grant_gate.lock().unwrap_or_else(|p| p.into_inner());
        let policy = self.policy.lock().unwrap_or_else(|p| p.into_inner());
        if level != GrantLevel::None {
            if !cfg!(any(target_os = "macos", windows)) {
                return Err(AppCommandError::configuration_invalid(
                    "the entire screen is not offered on Linux; share windows or applications \
                     instead",
                ));
            }
            if !policy.enabled {
                return Err(AppCommandError::configuration_invalid(
                    "computer use is switched off",
                ));
            }
            if !policy.screen_enabled {
                return Err(AppCommandError::configuration_invalid(
                    "sharing the entire screen is switched off in Computer use settings",
                ));
            }
            if self.stopped_since(since) {
                return Err(AppCommandError::configuration_invalid(
                    "Stop was pressed while this was being shared; share it again",
                ));
            }
        }
        Ok(self
            .targets
            .share_screen(level, now_ms(), &self.me, &policy.blocklist))
    }

    /// Whether a share begun at `since` may still land: computer use on,
    /// and no Stop since.
    pub(in crate::commands::computer) fn sharing_open(&self, since: u64) -> bool {
        let _gate = self.grant_gate.lock().unwrap_or_else(|p| p.into_inner());
        self.policy
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .enabled
            && !self.stopped_since(since)
    }

    /// Remove cua-driver, as the person asked from Settings: switch computer
    /// use off — through the settings writer, so every panel hears of it —
    /// then kill the driver (mid-call if it is in one) and stop the helper
    /// now, rather than whenever the switch is followed, before the files go.
    /// No helper starts again meanwhile: the backend asks the switch itself
    /// before starting one. Then every cached release. Switching back on
    /// fetches the pinned release again.
    pub(in crate::commands::computer) async fn uninstall_driver(
        &self,
        conn: &sea_orm::DatabaseConnection,
    ) -> Result<DriverInfo, AppCommandError> {
        self.drivers
            .begin(DriverTask::Uninstalling)
            .map_err(AppCommandError::configuration_invalid)?;
        let result = async {
            if self.config.snapshot().await.enabled {
                crate::commands::computer_tools::set_computer_tools_enabled_core(
                    conn,
                    &self.config,
                    &self.settings_events,
                    false,
                )
                .await
                .map_err(|e| e.to_string())?;
            }
            self.backend.close_now().await;
            #[cfg(feature = "tauri-runtime")]
            if self.desktop.is_some() {
                crate::managed_environment::change_computer_driver(
                    false,
                    &self.settings_events,
                    |_| {},
                )
                .await?;
                return Ok::<(), String>(());
            }
            crate::computer::driver::forget_cached_driver()
                .await
                .map_err(|e| e.to_string())?;
            Ok::<(), String>(())
        }
        .await;
        self.drivers.finish(result.as_ref().err().cloned());
        result
            .map(|()| self.drivers.info())
            .map_err(AppCommandError::configuration_invalid)
    }

    pub(in crate::commands::computer) fn record(
        &self,
        target_id: &str,
        action: ComputerAction,
        outcome: ActivityOutcome,
    ) {
        self.events.activity(&ComputerActivityPayload {
            actor: crate::computer::tool_dispatch::CALL_ACTOR
                .try_with(Clone::clone)
                .ok(),
            target_id: target_id.to_string(),
            action,
            outcome,
            at: now_ms(),
            app: None,
        });
    }

    /// What was done to an application rather than to a window of it.
    pub(in crate::commands::computer) fn record_app(
        &self,
        app: &str,
        action: ComputerAction,
        outcome: ActivityOutcome,
    ) {
        self.events.activity(&ComputerActivityPayload {
            actor: crate::computer::tool_dispatch::CALL_ACTOR
                .try_with(Clone::clone)
                .ok(),
            target_id: String::new(),
            action,
            outcome,
            at: now_ms(),
            app: Some(app.to_string()),
        });
    }

    /// Step 1: the switch, re-read now.
    pub(in crate::commands::computer) async fn usable(
        &self,
    ) -> Result<ComputerToolsConfig, Refusal> {
        if crate::computer::tool_dispatch::CALL_STOP
            .try_with(|epoch| self.stopped_since(*epoch))
            .unwrap_or(false)
        {
            return Err(stopped());
        }
        let config = self.config.snapshot().await;
        if !config.enabled {
            return Err(Refusal::refused(
                ERROR_UNAVAILABLE,
                NO_DESKTOP_NOTE.to_string(),
            ));
        }
        Ok(config)
    }

    pub(in crate::commands::computer) fn backend_refusal(
        &self,
        target_id: Option<&str>,
        e: BackendError,
    ) -> Refusal {
        match e {
            BackendError::PermissionMissing(p) => Refusal::failed(
                ERROR_PERMISSION_MISSING,
                permission_missing_note(permission_name(p)),
            ),
            BackendError::NoSuchWindow => {
                if let Some(id) = target_id {
                    let ended: Vec<_> = self.targets.target_changed(id).into_iter().collect();
                    self.announce(&ended);
                    Refusal::failed(ERROR_GRANT_REQUIRED, grant_required_note(id))
                } else {
                    Refusal::failed(ERROR_READ_FAILED, "The window is gone.".to_string())
                }
            }
            BackendError::Unavailable(why) | BackendError::Rejected(why) => Refusal::failed(
                ERROR_UNAVAILABLE,
                format!(
                    "Computer use cannot run right now: {why}. It may be worth trying again later."
                ),
            ),
            BackendError::Failed(why) => Refusal::failed(
                ERROR_READ_FAILED,
                format!("The window could not be read: {why}. It may be worth trying again."),
            ),
            BackendError::Refused(kind, words) => refused_act(kind, words),
        }
    }

    /// A backend error on a read or a listing admitted at Stop count
    /// `stop`: one that met the person's Stop on its way — the driver it was
    /// using killed under it — is reported as the Stop.
    pub(in crate::commands::computer) fn backend_read_refusal(
        &self,
        target_id: Option<&str>,
        e: BackendError,
        stop: u64,
    ) -> Refusal {
        if self.stopped_since(stop) {
            return stopped();
        }
        self.backend_refusal(target_id, e)
    }

    /// A backend error on an action let through at Stop count `stop`. What
    /// differs from a read: an action that failed or lost its helper on the
    /// way may have happened anyway, and the words say so; and a helper that
    /// went away because the person pressed Stop is reported as the Stop.
    pub(in crate::commands::computer) fn backend_act_refusal(
        &self,
        target_id: &str,
        e: BackendError,
        stop: u64,
    ) -> Refusal {
        if self.stopped_since(stop) {
            return match e {
                // Refused before it went out, whatever for: not done.
                BackendError::Refused(kind, _) if kind != ActRefusal::Failed => stopped(),
                _ => Refusal::refused(
                    ERROR_STOPPED,
                    format!(
                        "The user pressed Stop while this action was on its way: it may or may \
                         not have happened. {STOPPED_NOTE}"
                    ),
                )
                .maybe_done(),
            };
        }
        match e {
            BackendError::Unavailable(why) => Refusal::failed(
                ERROR_UNAVAILABLE,
                format!(
                    "Computer use stopped working during the action ({why}); it may or may not \
                     have happened. Read the window again before going on."
                ),
            )
            .maybe_done(),
            BackendError::Failed(why) => Refusal::failed(
                ERROR_ACTION_FAILED,
                format!(
                    "The action did not complete ({why}); it may or may not have happened. Read \
                     the window again before going on."
                ),
            )
            .maybe_done(),
            other => self.backend_refusal(Some(target_id), other),
        }
    }
}
