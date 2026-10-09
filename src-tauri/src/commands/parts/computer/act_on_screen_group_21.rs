// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl ComputerService {
    /// One action on the entire screen, for an agent: checked as one on a
    /// window is, minus the window — the screen shared for control and its
    /// grant in force, every point from its latest picture — and sent once,
    /// at the front as real input, only where the person allows the front.
    /// The helper refuses a point on what it painted over.
    pub(in crate::commands::computer) async fn act_on_screen(
        &self,
        request: &ComputerActRequest,
        requested: Option<ActDelivery>,
    ) -> Result<ActReport, Refusal> {
        let _turn = self.turn.lock().await;
        let stop = self.stop_count();
        let config = self.usable().await?;
        let ticket = match self
            .targets
            .begin_screen_act(now_ms(), config.grant_ttl, request)
        {
            Ok(ticket) => ticket,
            Err((why, ended)) => {
                self.announce_change(ended);
                return Err(screen_denied(why));
            }
        };
        if requested == Some(ActDelivery::Background) {
            return Err(Refusal::failed(
                ERROR_BACKGROUND_UNAVAILABLE,
                SCREEN_NOT_BACKGROUND_NOTE.to_string(),
            ));
        }
        if !config.allow_foreground {
            return Err(Refusal::refused(
                ERROR_FOREGROUND_NOT_ALLOWED,
                SCREEN_NEEDS_FRONT_NOTE.to_string(),
            ));
        }
        // The grant was looked at after `stop` was counted, so a Stop in
        // between either ended it above or shows here.
        if self.stopped_since(stop) {
            return Err(stopped());
        }
        let rules = self.screen_rules(&config);
        // Asked again with the helper in hand, just before it goes: the
        // sharing it was let through under still in force for control, the
        // switches still on, and nothing added to the never-share list the
        // helper is to judge the screen by.
        let epoch = ticket.epoch;
        let judged_by = rules.blocklist.clone();
        let still = move || {
            let _gate = self.grant_gate.lock().unwrap_or_else(|p| p.into_inner());
            let policy = self.policy.lock().unwrap_or_else(|p| p.into_inner());
            policy.enabled
                && policy.screen_enabled
                && policy
                    .blocklist
                    .entries()
                    .iter()
                    .all(|entry| judged_by.contains(entry))
                && self.targets.screen_controlled(epoch)
        };
        let raw = self
            .backend
            .act_screen(rules, ticket.action, ticket.geometry, stop, &still)
            .await
            .map_err(|e| self.backend_act_refusal(SCREEN_TARGET_ID, e, stop))?;
        Ok(ActReport {
            target_id: SCREEN_TARGET_ID.to_string(),
            effect: raw.effect,
            route: raw.route,
            delivery: ActDelivery::Foreground,
            presses: None,
            submitted: None,
            submit_note: None,
        })
    }

    /// Start an installed application for an agent — the one listed under
    /// `key`, or else `name` — in the background, where the person allows
    /// it. Never iyw-claw, nor an application on the blocklist. Its windows are
    /// not shared by it: the person shares them, as any other.
    pub async fn agent_launch_app(
        &self,
        name: Option<String>,
        key: Option<String>,
    ) -> ComputerLaunchOutcome {
        let _turn = self.turn.lock().await;
        let stop = self.stop_count();
        let config = match self.usable().await {
            Ok(config) => config,
            Err(r) => return ComputerLaunchOutcome::refused(r.slug, r.note),
        };
        if !config.launch_enabled {
            return ComputerLaunchOutcome::refused(ERROR_UNAVAILABLE, LAUNCH_OFF_NOTE);
        }
        let found = match self.backend.find_app(name, key).await {
            Ok(found) => found,
            Err(e) => {
                let r = self.backend_read_refusal(None, e, stop);
                return ComputerLaunchOutcome::refused(r.slug, r.note);
            }
        };
        // Judged by who the application is and by every word of the command
        // that starts it: a command carries arguments, or a wrapper.
        let blocklist = blocklist_of(&config);
        let command = found.launch_path.as_deref().unwrap_or_default();
        let blocked = if self.me.owns(&found.app) || self.me.owns_command(command) {
            Some(NotGrantable::OwnApp)
        } else if blocklist.matches(&found.app) || blocklist.matches_command(command) {
            Some(NotGrantable::Blocklisted)
        } else {
            None
        };
        let name = found.app.name.clone();
        let overview = found
            .app
            .bundle_id
            .as_deref()
            .is_some_and(|id| SHOWS_EVERY_WINDOW.contains(&id));
        if overview {
            self.record_app(&name, ComputerAction::Launch, ActivityOutcome::Refused);
            return ComputerLaunchOutcome::refused(
                ERROR_BLOCKED,
                format!(
                    "{name} is not started for an agent: it shows every window at once, the ones \
                     that are never shared included. Retrying will not change it."
                ),
            );
        }
        if let Some(why) = blocked {
            self.record_app(&name, ComputerAction::Launch, ActivityOutcome::Refused);
            return ComputerLaunchOutcome::refused(
                ERROR_BLOCKED,
                format!(
                    "{name} is not started for an agent: {} Retrying will not change it.",
                    why.note()
                ),
            );
        }
        if self.stopped_since(stop) {
            return ComputerLaunchOutcome::refused(ERROR_STOPPED, STOPPED_NOTE);
        }
        let key = found
            .app
            .key()
            .or(found.launch_path.as_deref())
            .unwrap_or_default()
            .to_string();
        match self.backend.launch_app(found, stop).await {
            Ok(raw) => {
                self.record_app(&raw.name, ComputerAction::Launch, ActivityOutcome::Done);
                ComputerLaunchOutcome {
                    app: Some(AgentAppRef {
                        key,
                        name: raw.name,
                        pid: raw.pid.unwrap_or(0),
                    }),
                    error: None,
                    note: Some(LAUNCHED_NOTE.to_string()),
                }
            }
            Err(e) => {
                self.record_app(&name, ComputerAction::Launch, ActivityOutcome::Failed);
                let r = if self.stopped_since(stop) {
                    stopped()
                } else {
                    self.backend_refusal(None, e)
                };
                ComputerLaunchOutcome::refused(
                    r.slug,
                    format!("{} It may or may not have started.", r.note),
                )
            }
        }
    }
}
