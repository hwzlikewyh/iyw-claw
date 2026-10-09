// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl ComputerService {
    /// The clipboard for an agent, where the person allows it: read back
    /// only what an agent put there itself, while the clipboard still holds
    /// it (see [`OwnedClipboard`]); or put text there, which an agent may
    /// then paste.
    pub async fn agent_clipboard(&self, op: ClipboardOp) -> ComputerClipboardOutcome {
        let _turn = self.turn.lock().await;
        let stop = self.stop_count();
        let config = match self.usable().await {
            Ok(config) => config,
            Err(r) => return ComputerClipboardOutcome::refused(r.slug, r.note),
        };
        if !config.clipboard_enabled {
            return ComputerClipboardOutcome::refused(ERROR_UNAVAILABLE, CLIPBOARD_OFF_NOTE);
        }
        match op {
            ClipboardOp::Read => {
                let action = ComputerAction::ClipboardRead;
                let Some(expect) = self.owned_clipboard() else {
                    self.record("", action, ActivityOutcome::Refused);
                    return ComputerClipboardOutcome::refused(
                        ERROR_GRANT_REQUIRED,
                        CLIPBOARD_NOT_YOURS_NOTE,
                    );
                };
                let read = self.backend.clipboard_read(expect).await;
                if self.stopped_since(stop) {
                    return ComputerClipboardOutcome::refused(ERROR_STOPPED, STOPPED_NOTE);
                }
                // The window it was copied from may have been taken back, or
                // shared again, while it was being read.
                if read.is_ok() && self.owned_clipboard() != Some(expect) {
                    self.record("", action, ActivityOutcome::Refused);
                    return ComputerClipboardOutcome::refused(
                        ERROR_GRANT_REQUIRED,
                        CLIPBOARD_NOT_YOURS_NOTE,
                    );
                }
                match read {
                    Ok(raw) => {
                        self.record("", action, ActivityOutcome::Done);
                        ComputerClipboardOutcome {
                            text: Some(raw.text.unwrap_or_default()),
                            ..ComputerClipboardOutcome::default()
                        }
                    }
                    Err(e) => {
                        let r = self.backend_refusal(None, e);
                        self.record("", action, r.outcome);
                        ComputerClipboardOutcome::refused(r.slug, r.note)
                    }
                }
            }
            ClipboardOp::Write { text } => {
                let action = ComputerAction::ClipboardWrite;
                if text.chars().count() > MAX_CLIPBOARD_WRITE_CHARS {
                    return ComputerClipboardOutcome::refused(
                        ERROR_ACTION_FAILED,
                        format!(
                            "That is more than the {MAX_CLIPBOARD_WRITE_CHARS} characters one \
                             write puts on the clipboard; nothing was written."
                        ),
                    );
                }
                if self.stopped_since(stop) {
                    return ComputerClipboardOutcome::refused(ERROR_STOPPED, STOPPED_NOTE);
                }
                match self.backend.clipboard_write(text, stop).await {
                    Ok(stamp) => {
                        if !self.own_clipboard(
                            OwnedClipboard {
                                stamp,
                                source: None,
                            },
                            stop,
                        ) {
                            return ComputerClipboardOutcome::refused(
                                ERROR_STOPPED,
                                format!(
                                    "The user pressed Stop as the text went on the clipboard: \
                                     nothing on it is held as yours. {STOPPED_NOTE}"
                                ),
                            );
                        }
                        self.record("", action, ActivityOutcome::Done);
                        ComputerClipboardOutcome {
                            written: true,
                            note: Some(CLIPBOARD_WRITTEN_NOTE.to_string()),
                            ..ComputerClipboardOutcome::default()
                        }
                    }
                    Err(e) => {
                        let r = if self.stopped_since(stop) {
                            stopped()
                        } else {
                            self.backend_refusal(None, e)
                        };
                        self.record("", action, ActivityOutcome::Failed);
                        ComputerClipboardOutcome::refused(r.slug, r.note)
                    }
                }
            }
        }
    }

    /// Act on a window shared for control, brought to the front for it or
    /// not as `delivery` asks — or as the person set it, when it does not. A
    /// key pressed more than once is that many actions, each checked on its
    /// own and each taking its own turn at the driver: taking the window
    /// back between two presses, the front, or Stop ends the rest. So is a
    /// held key: pressed at once, then — after the delay a held key waits
    /// before it repeats — at the rate it repeats, until its time is up
    /// ([`Presses`]), counted from the first press in real time.
    pub async fn agent_act(
        &self,
        target_id: &str,
        request: ComputerActRequest,
        delivery: Option<ActDelivery>,
    ) -> ComputerActOutcome {
        let action = ComputerAction::of(&request);
        if target_id == SCREEN_TARGET_ID {
            return match self.act_on_screen(&request, delivery).await {
                Ok(report) => {
                    self.record(target_id, action, ActivityOutcome::Done);
                    ComputerActOutcome::done(target_id, report)
                }
                Err(r) => {
                    self.record(target_id, action, r.outcome);
                    ComputerActOutcome::refused(target_id, r.slug, r.note)
                }
            };
        }
        let presses = Presses::of(&request);
        let mark = |press: &Press| {
            #[cfg(feature = "tauri-runtime")]
            if let (Some(desktop), Some(at)) = (&self.desktop, press.aim.landing(&press.raw)) {
                desktop.marker.mark(at, action);
            }
            // No marker in iyw-claw-server: nothing on its screen of iyw-claw's.
            #[cfg(not(feature = "tauri-runtime"))]
            let _ = (press.aim, action);
        };
        let first = {
            let turn = self.turn.lock().await;
            self.act_once(&turn, target_id, &request, delivery, None)
                .await
        };
        let first = match first {
            Ok(press) => press,
            Err(r) => {
                self.record(target_id, action, r.outcome);
                return ComputerActOutcome::refused(target_id, r.slug, r.note);
            }
        };
        mark(&first);
        let later = LaterPress {
            stop: first.stop,
            epoch: first.epoch,
            until: presses.length().map(|length| first.sent_at + length),
        };
        let time_up = |at: tokio::time::Instant| later.until.is_some_and(|until| at >= until);
        let mut next = first.sent_at;
        let mut pressed: u32 = 1;
        let mut done = first;
        loop {
            match presses {
                Presses::Count(count) if pressed >= count => break,
                Presses::Count(_) => {}
                Presses::Held(_) => {
                    next += if pressed == 1 {
                        HOLD_DELAY
                    } else {
                        HOLD_INTERVAL
                    };
                    // A press that took longer than the gap is not made up
                    // for: the next goes as soon as it can.
                    next = next.max(tokio::time::Instant::now());
                    if time_up(next) {
                        break;
                    }
                    tokio::time::sleep_until(next).await;
                }
            }
            let press = {
                let turn = self.turn.lock().await;
                // The turn may have been a while coming.
                if time_up(tokio::time::Instant::now()) {
                    break;
                }
                self.act_once(&turn, target_id, &request, delivery, Some(later))
                    .await
            };
            match press {
                Ok(press) => {
                    mark(&press);
                    pressed += 1;
                    done = press;
                }
                Err(r) => {
                    self.record(target_id, action, r.outcome);
                    let note = format!("{}{}", presses.cut_short(pressed, r.maybe_done), r.note);
                    return ComputerActOutcome::refused(target_id, r.slug, note);
                }
            }
        }
        self.record(target_id, action, ActivityOutcome::Done);
        ComputerActOutcome::done(
            target_id,
            ActReport {
                target_id: target_id.to_string(),
                effect: done.raw.effect,
                route: done.raw.route,
                delivery: done.delivery,
                presses: presses.reported(pressed),
                submitted: done.raw.submitted,
                submit_note: done.raw.submit_note,
            },
        )
    }
}
