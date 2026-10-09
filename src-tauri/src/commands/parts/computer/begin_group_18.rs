// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl ComputerService {
    /// Steps 1–3: everything that has to hold before the helper is asked.
    pub(in crate::commands::computer) async fn begin(
        &self,
        target_id: &str,
    ) -> Result<Admitted, Refusal> {
        // Counted before the grant is looked at: a Stop after this revokes
        // the grant, or is caught by `finish`.
        let stop = self.stop_count();
        let config = self.usable().await?;
        let blocklist = blocklist_of(&config);
        let ticket = match self.targets.begin_read(
            target_id,
            now_ms(),
            config.grant_ttl,
            &self.me,
            &blocklist,
        ) {
            Ok(ticket) => ticket,
            Err((why, ended)) => {
                self.announce(&ended.into_iter().collect::<Vec<_>>());
                return Err(match why {
                    ReadRefusal::NoSuchTarget => {
                        Refusal::refused(ERROR_NO_SUCH_TARGET, no_such_target_note(target_id))
                    }
                    ReadRefusal::GrantRequired => {
                        Refusal::refused(ERROR_GRANT_REQUIRED, grant_required_note(target_id))
                    }
                    ReadRefusal::NotGrantable(why) => {
                        Refusal::refused(ERROR_BLOCKED, blocked_note(target_id, why.note()))
                    }
                });
            }
        };
        self.check_identity(&ticket.target_id, &ticket.identity)?;
        Ok(Admitted {
            ticket,
            switched_off: config.switched_off,
            stop,
        })
    }

    /// Step 3. A pid that no longer answers with the start time it had when
    /// the window was shared is a different process — the window's owner, or
    /// the process drawing inside a frame (`WindowIdentity::content`). A
    /// window without a start time cannot have been shared at all
    /// (`NotGrantable::Unidentified`). Returns the start time the grant is
    /// held against.
    pub(in crate::commands::computer) fn check_identity(
        &self,
        target_id: &str,
        identity: &WindowIdentity,
    ) -> Result<u64, Refusal> {
        let Some(started_at) = identity.started_at else {
            return Err(Refusal::refused(
                ERROR_BLOCKED,
                blocked_note(target_id, NotGrantable::Unidentified.note()),
            ));
        };
        let content_changed = identity
            .content
            .is_some_and(|run| process_start(run.pid) != Some(run.started_at));
        if process_start(identity.pid) != Some(started_at) || content_changed {
            let ended: Vec<_> = self.targets.target_changed(target_id).into_iter().collect();
            self.announce(&ended);
            return Err(Refusal::failed(
                ERROR_GRANT_REQUIRED,
                grant_required_note(target_id),
            ));
        }
        Ok(started_at)
    }

    /// Step 4's second half: steps 1–3 again, against what the read began
    /// under. `mark` is what the read leaves for later actions.
    pub(in crate::commands::computer) async fn finish(
        &self,
        admitted: &Admitted,
        mark: Option<ReadMark>,
    ) -> Result<String, Refusal> {
        let ticket = &admitted.ticket;
        let refused =
            || Refusal::refused(ERROR_GRANT_REQUIRED, grant_required_note(&ticket.target_id));
        if self.stopped_since(admitted.stop) {
            return Err(stopped());
        }
        let config = self.usable().await?;
        if config.switched_off != admitted.switched_off {
            return Err(refused());
        }
        // Whatever process holds the pid now is the one the helper just read:
        // if it is not the one the window was shared from, neither is what
        // was read.
        self.check_identity(&ticket.target_id, &ticket.identity)?;
        let blocklist = blocklist_of(&config);
        match self.targets.finish_read(ticket, &self.me, &blocklist, mark) {
            Ok(generation) => Ok(generation),
            Err((why, ended)) => {
                self.announce(&ended.into_iter().collect::<Vec<_>>());
                Err(match why {
                    ReadRefusal::NotGrantable(why) => {
                        Refusal::refused(ERROR_BLOCKED, blocked_note(&ticket.target_id, why.note()))
                    }
                    ReadRefusal::NoSuchTarget | ReadRefusal::GrantRequired => refused(),
                })
            }
        }
    }

    /// The window's title as the agent may see it now.
    pub(in crate::commands::computer) fn title_for(
        &self,
        target_id: &str,
        raw: Option<String>,
    ) -> Option<String> {
        let entry = self.targets.get(target_id)?;
        let level = entry.grant.as_ref().map_or(GrantLevel::None, |g| g.level);
        let title = raw.filter(|t| !t.is_empty()).unwrap_or(entry.title);
        visible_title(level, &title)
    }

    pub async fn agent_list_apps(&self) -> ComputerAppsOutcome {
        let _turn = self.turn.lock().await;
        let stop = self.stop_count();
        let config = match self.usable().await {
            Ok(config) => config,
            Err(r) => return ComputerAppsOutcome::refused(r.slug, r.note),
        };
        let blocklist = blocklist_of(&config);
        let listed = self.backend.list_apps().await;
        // A Stop that came while the helper was listing cuts this off too.
        if self.stopped_since(stop) {
            return ComputerAppsOutcome::refused(ERROR_STOPPED, STOPPED_NOTE);
        }
        match listed {
            Ok(apps) => ComputerAppsOutcome {
                apps: apps
                    .into_iter()
                    .map(|app| AgentAppSummary {
                        note: grantable(&app, &self.me, &blocklist)
                            .err()
                            .map(|why| why.note().to_string()),
                        app: AgentAppRef {
                            key: app.key().unwrap_or_default().to_string(),
                            name: app.name.clone(),
                            pid: app.pid,
                        },
                        active: app.active,
                        level: self.targets.app_level(&app),
                    })
                    .collect(),
                error: None,
                note: None,
            },
            Err(e) => {
                let r = self.backend_read_refusal(None, e, stop);
                ComputerAppsOutcome::refused(r.slug, r.note)
            }
        }
    }

    pub async fn agent_list_windows(&self, pid: Option<u32>) -> ComputerWindowsOutcome {
        let _turn = self.turn.lock().await;
        let stop = self.stop_count();
        let config = match self.usable().await {
            Ok(config) => config,
            Err(r) => return ComputerWindowsOutcome::refused(r.slug, r.note),
        };
        let blocklist = blocklist_of(&config);
        let listed = self.backend.list_windows(pid).await;
        // Grants that have already ended by the rules as they are now must
        // not show — neither as a level nor as a title.
        self.sweep().await;
        // A Stop that came while the helper was listing, or since, cuts this
        // off too; nothing below waits on anything.
        if self.stopped_since(stop) {
            return ComputerWindowsOutcome::refused(ERROR_STOPPED, STOPPED_NOTE);
        }
        match listed {
            Ok(windows) => {
                let (entries, ended) = self.targets.observe(&windows, pid);
                self.announce(&ended);
                ComputerWindowsOutcome {
                    windows: entries
                        .iter()
                        .filter(|e| e.worth_listing())
                        .map(|e| e.agent_summary(&self.me, &blocklist))
                        .collect(),
                    screen: self.targets.shared_screen().map(|screen| AgentScreen {
                        target_id: SCREEN_TARGET_ID.to_string(),
                        level: screen.level,
                    }),
                    input: Some(InputPolicy::of(&config)),
                    error: None,
                    note: None,
                }
            }
            Err(e) => {
                let r = self.backend_read_refusal(None, e, stop);
                ComputerWindowsOutcome::refused(r.slug, r.note)
            }
        }
    }
}
