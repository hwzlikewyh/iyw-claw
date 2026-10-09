// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl ComputerService {
    pub(in crate::commands::computer) async fn verify_inner(
        &self,
        target_id: &str,
        request: VerifyRequest,
    ) -> Result<VerifyOutcome, Refusal> {
        if target_id == SCREEN_TARGET_ID {
            return Err(Refusal::failed(
                ERROR_ACTION_FAILED,
                SCREEN_POINTER_ONLY_NOTE.to_string(),
            ));
        }
        let _turn = self.turn.lock().await;
        let admitted = self.begin(target_id).await?;
        let ticket = &admitted.ticket;
        let raw = self
            .backend
            .verify(ticket.identity.pid, ticket.identity.window_id, request)
            .await
            .map_err(|e| self.backend_read_refusal(Some(target_id), e, admitted.stop))?;
        self.finish(&admitted, None).await?;
        Ok(VerifyOutcome {
            target_id: target_id.to_string(),
            status: raw.status,
            stable: raw.stable,
            samples: raw.samples,
            elapsed_ms: raw.elapsed_ms,
            predicates: raw.predicates,
        })
    }

    pub async fn agent_verify(
        &self,
        target_id: &str,
        request: VerifyRequest,
    ) -> ComputerVerifyOutcome {
        match self.verify_inner(target_id, request).await {
            Ok(verify) => {
                self.record(target_id, ComputerAction::Verify, ActivityOutcome::Done);
                ComputerVerifyOutcome::verdict(target_id, verify)
            }
            Err(r) => {
                self.record(target_id, ComputerAction::Verify, r.outcome);
                ComputerVerifyOutcome::refused(target_id, r.slug, r.note)
            }
        }
    }

    /// One action, checked from the top — its turn at the driver held by
    /// the caller (`_turn`) — the switch, the grant and the action against
    /// what the agent last read, the process, whether its window may come to
    /// the front if that is how it is to go (`requested`, or the person's
    /// default), no Stop since it began — and then the helper, which checks
    /// again what only it can see, the Stop count included. A press after the
    /// first of one key (`later`) is held to the Stop count and the sharing
    /// the first went out under.
    pub(in crate::commands::computer) async fn act_once(
        &self,
        _turn: &tokio::sync::MutexGuard<'_, ()>,
        target_id: &str,
        request: &ComputerActRequest,
        requested: Option<ActDelivery>,
        later: Option<LaterPress>,
    ) -> Result<Press, Refusal> {
        // A Stop since the first press ends the presses, whatever has been
        // shared again since.
        let stop = match later {
            Some(first) if self.stopped_since(first.stop) => return Err(stopped()),
            Some(first) => first.stop,
            None => self.stop_count(),
        };
        let config = self.usable().await?;
        if request.needs_launch_switch() && !config.launch_enabled {
            return Err(Refusal::refused(
                ERROR_UNAVAILABLE,
                LAUNCH_OFF_NOTE.to_string(),
            ));
        }
        let blocklist = blocklist_of(&config);
        // What pastes may go only while the clipboard holds what an agent put
        // there itself; the helper checks it is still so as the action goes.
        let owned = self.owned_clipboard();
        let ticket = match self.targets.begin_act(
            target_id,
            now_ms(),
            config.grant_ttl,
            &self.me,
            &blocklist,
            request,
            owned.is_some(),
        ) {
            Ok(ticket) => ticket,
            Err((why, ended)) => {
                self.announce(&ended.into_iter().collect::<Vec<_>>());
                return Err(denied(target_id, why));
            }
        };
        let epoch = ticket.epoch;
        // So does the window taken back and shared again: a new sharing,
        // which the presses did not begin under.
        if later.is_some_and(|first| first.epoch != epoch) {
            return Err(Refusal::refused(
                ERROR_GRANT_REQUIRED,
                reshared_note(target_id),
            ));
        }
        let started_at = self.check_identity(target_id, &ticket.identity)?;
        let aim = ticket.aim;
        // Against the settings as they are now: the front turned off since
        // the last press stops the next.
        let delivery = delivery_for(request, requested, &config)?;
        // The grant was looked at after `stop` was counted, so a Stop in
        // between either revoked it above or shows here.
        if self.stopped_since(stop) {
            return Err(stopped());
        }
        let clipboard = ClipboardUse {
            track: copies(request),
            // Only for what could paste: the helper reads the clipboard's
            // stamp for it, which an ordinary action has no need of.
            paste: owned.filter(|_| may_paste(request)),
        };
        let sent_at = tokio::time::Instant::now();
        let press = self
            .backend
            .act(
                ticket.identity.pid,
                ticket.identity.window_id,
                started_at,
                ticket.identity.content,
                ticket.app.key().map(str::to_string),
                ticket.action,
                delivery,
                clipboard,
                stop,
            )
            .await
            .map(|raw| Press {
                raw,
                aim,
                delivery,
                sent_at,
                stop,
                epoch,
            });
        // What the action put on the clipboard is the agent's own, out of a
        // window it may read — for as long as that window is shared so.
        if let Ok(Press {
            raw: RawAct {
                clipboard: Some(stamp),
                ..
            },
            ..
        }) = &press
        {
            self.own_clipboard(
                OwnedClipboard {
                    stamp: *stamp,
                    source: Some((target_id.to_string(), epoch)),
                },
                stop,
            );
        }
        press.map_err(|e| {
            with_next_step(
                self.backend_act_refusal(target_id, e, stop),
                request,
                &config,
            )
        })
    }
}
