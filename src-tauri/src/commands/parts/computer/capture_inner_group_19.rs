// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl ComputerService {
    pub(in crate::commands::computer) async fn capture_inner(
        &self,
        target_id: &str,
        max_dimension: Option<u32>,
    ) -> Result<WindowCapture, Refusal> {
        if target_id == SCREEN_TARGET_ID {
            return self.capture_screen(max_dimension).await;
        }
        let _turn = self.turn.lock().await;
        let admitted = self.begin(target_id).await?;
        let ticket = &admitted.ticket;
        let max = max_dimension
            .unwrap_or(DEFAULT_MAX_DIMENSION)
            .clamp(1, DEFAULT_MAX_DIMENSION);
        let raw = self
            .backend
            .capture(ticket.identity.pid, ticket.identity.window_id, Some(max))
            .await
            .map_err(|e| self.backend_read_refusal(Some(target_id), e, admitted.stop))?;
        let window_bounds = if raw.window_bounds.is_empty() {
            ticket.bounds
        } else {
            raw.window_bounds
        };
        let mark = ReadMark::Capture {
            width: raw.width,
            height: raw.height,
            native_width: raw.native_width,
            native_height: raw.native_height,
            // Only the helper's own measure of the window says whether the
            // capture is its full size; bounds iyw-claw fills in from the
            // listing are not that.
            full_size: raw.full_size && !raw.window_bounds.is_empty(),
            window_bounds: raw.window_bounds,
        };
        let generation = self.finish(&admitted, Some(mark)).await?;
        Ok(WindowCapture {
            target_id: target_id.to_string(),
            generation,
            mime: "image/png".to_string(),
            data: raw.png_base64,
            width: raw.width,
            height: raw.height,
            window_bounds,
            title: self.title_for(target_id, raw.title),
        })
    }

    /// A picture of the entire screen, for an agent — read as a window is,
    /// minus the window: the screen shared and its grant in force, then,
    /// once the helper has taken it, no Stop since, the switch not off since,
    /// the same sharing of the screen, and nothing added to the never-share
    /// list while it was taken (the helper painted over what the list said
    /// when it began).
    pub(in crate::commands::computer) async fn capture_screen(
        &self,
        max_dimension: Option<u32>,
    ) -> Result<WindowCapture, Refusal> {
        let _turn = self.turn.lock().await;
        let stop = self.stop_count();
        let config = self.usable().await?;
        let refused =
            || Refusal::refused(ERROR_GRANT_REQUIRED, SCREEN_GRANT_REQUIRED_NOTE.to_string());
        let ticket = match self.targets.begin_screen_read(now_ms(), config.grant_ttl) {
            Ok(ticket) => ticket,
            Err((_, ended)) => {
                self.announce_change(ended);
                return Err(refused());
            }
        };
        let rules = self.screen_rules(&config);
        let max = max_dimension
            .unwrap_or(DEFAULT_MAX_DIMENSION)
            .clamp(1, DEFAULT_MAX_DIMENSION);
        let raw = self
            .backend
            .capture_screen(rules.clone(), Some(max))
            .await
            .map_err(|e| self.backend_read_refusal(None, e, stop))?;
        if self.stopped_since(stop) {
            return Err(stopped());
        }
        let now = self.usable().await?;
        if now.switched_off != config.switched_off {
            return Err(refused());
        }
        let grew = blocklist_of(&now)
            .entries()
            .iter()
            .any(|entry| !rules.blocklist.contains(entry));
        if grew {
            return Err(Refusal::failed(
                ERROR_READ_FAILED,
                SCREEN_RULES_CHANGED_NOTE.to_string(),
            ));
        }
        let mark = ReadMark::Capture {
            width: raw.width,
            height: raw.height,
            native_width: raw.native_width,
            native_height: raw.native_height,
            full_size: raw.full_size && !raw.window_bounds.is_empty(),
            window_bounds: raw.window_bounds,
        };
        let generation = self
            .targets
            .finish_screen_read(&ticket, mark)
            .map_err(|_| refused())?;
        Ok(WindowCapture {
            target_id: SCREEN_TARGET_ID.to_string(),
            generation,
            mime: "image/png".to_string(),
            data: raw.png_base64,
            width: raw.width,
            height: raw.height,
            window_bounds: raw.window_bounds,
            title: None,
        })
    }

    pub async fn agent_capture(
        &self,
        target_id: &str,
        max_dimension: Option<u32>,
    ) -> ComputerCaptureOutcome {
        match self.capture_inner(target_id, max_dimension).await {
            Ok(capture) => {
                self.record(target_id, ComputerAction::Capture, ActivityOutcome::Done);
                ComputerCaptureOutcome::image(target_id, capture)
            }
            Err(r) => {
                self.record(target_id, ComputerAction::Capture, r.outcome);
                ComputerCaptureOutcome::refused(target_id, r.slug, r.note)
            }
        }
    }

    pub(in crate::commands::computer) async fn snapshot_inner(
        &self,
        target_id: &str,
        request: SnapshotRequest,
    ) -> Result<WindowSnapshot, Refusal> {
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
            .snapshot(
                ticket.identity.pid,
                ticket.identity.window_id,
                SnapshotOptions {
                    max_depth: request.max_depth,
                    max_elements: request.max_elements,
                    query: request.query,
                    app_menus: ticket.scope != GrantScope::Window,
                },
            )
            .await
            .map_err(|e| self.backend_read_refusal(Some(target_id), e, admitted.stop))?;
        let (tree, cut) = cut_tree(
            &raw.tree,
            request.max_chars.unwrap_or(DEFAULT_SNAPSHOT_MAX_CHARS),
        );
        // A ref is usable when its line is in what the agent is given: the
        // tree is cut between lines, so a line that starts before the cut
        // is there.
        let kept = tree.len();
        let mut mark_shown = BTreeSet::new();
        let mut mark_cut = BTreeSet::new();
        let mut mark_secret = BTreeSet::new();
        for r in &raw.refs {
            if (r.offset as usize) < kept {
                mark_shown.insert(r.index);
            } else {
                mark_cut.insert(r.index);
            }
            if r.secret {
                mark_secret.insert(r.index);
            }
        }
        let mark = ReadMark::Snapshot {
            snapshot_id: raw.snapshot_id.clone(),
            shown: mark_shown,
            cut: mark_cut,
            secret: mark_secret,
        };
        let generation = self.finish(&admitted, Some(mark)).await?;
        Ok(WindowSnapshot {
            target_id: target_id.to_string(),
            generation,
            title: self.title_for(target_id, raw.title),
            window_bounds: raw.window_bounds.filter(|b: &Rect| !b.is_empty()),
            tree,
            element_count: raw.element_count,
            truncated: cut || raw.truncated,
            degraded: raw.degraded,
        })
    }

    pub async fn agent_snapshot(
        &self,
        target_id: &str,
        request: SnapshotRequest,
    ) -> ComputerSnapshotOutcome {
        match self.snapshot_inner(target_id, request).await {
            Ok(snapshot) => {
                self.record(target_id, ComputerAction::Snapshot, ActivityOutcome::Done);
                ComputerSnapshotOutcome::tree(target_id, snapshot)
            }
            Err(r) => {
                self.record(target_id, ComputerAction::Snapshot, r.outcome);
                ComputerSnapshotOutcome::refused(target_id, r.slug, r.note)
            }
        }
    }
}
