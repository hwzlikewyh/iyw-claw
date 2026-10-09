// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl SnapshotBook {
    /// Remember a snapshot just taken of `(pid, window_id)` — or, with
    /// `None`, that the driver kept none, so nothing from before is current.
    pub fn record(&mut self, pid: u32, window_id: u64, facts: Option<SnapshotFacts>) {
        let key = (pid, window_id);
        // The driver mints ids from one counter: an answer that arrives after
        // a later snapshot's was recorded is not the current one.
        if let (Some(new), Some(held)) = (
            facts.as_ref().and_then(|f| snapshot_number(&f.snapshot_id)),
            self.windows
                .get(&key)
                .and_then(|f| snapshot_number(&f.snapshot_id)),
        ) {
            if new < held {
                return;
            }
        }
        self.order.retain(|k| *k != key);
        match facts {
            Some(facts) => {
                self.windows.insert(key, facts);
                self.order.push_back(key);
                while self.order.len() > BOOK_WINDOWS {
                    if let Some(oldest) = self.order.pop_front() {
                        self.windows.remove(&oldest);
                    }
                }
            }
            None => {
                self.windows.remove(&key);
            }
        }
    }

    /// Forget everything — the driver that took these snapshots is gone.
    pub fn clear(&mut self) {
        self.windows.clear();
        self.order.clear();
    }

    /// Where `element` was on the screen when its snapshot was taken, if that
    /// is still the window's latest snapshot and it said.
    pub fn frame(&self, pid: u32, window_id: u64, element: &ElementRef) -> Option<Rect> {
        self.windows
            .get(&(pid, window_id))
            .filter(|f| f.snapshot_id == element.snapshot_id)?
            .elements
            .get(&element.index)?
            .frame
    }

    /// Check `action`'s element against the latest snapshot of the window:
    /// it is from that snapshot, the snapshot has such an element, and the
    /// element may take what the action does to it.
    /// `paste_ok`: the clipboard is still what the agent put there, and a
    /// control that pastes may be pressed.
    pub fn check(
        &self,
        pid: u32,
        window_id: u64,
        action: &WindowAction,
        app_key: Option<&str>,
        paste_ok: bool,
    ) -> Result<(), HelperError> {
        let Some(element) = action.element() else {
            return Ok(());
        };
        let stale = || {
            HelperError::new(
                HelperErrorCode::StaleRef,
                "That ref is from a snapshot this window has moved past. Take a new \
                 computer_snapshot and use a ref from it.",
            )
        };
        let facts = self
            .windows
            .get(&(pid, window_id))
            .filter(|f| f.snapshot_id == element.snapshot_id)
            .ok_or_else(stale)?;
        let found = facts.elements.get(&element.index).ok_or_else(stale)?;
        // Pressing it pastes: the person's clipboard would land in the
        // window, unless it holds what the agent put there. Scrolling over
        // it does not.
        if found.paste && !paste_ok && !matches!(action, WindowAction::Scroll { .. }) {
            return Err(paste_refused());
        }
        if found.secret && action.writes_text() {
            return Err(HelperError::new(
                HelperErrorCode::SecretField,
                "That is a password or other secret field: typing into it, or setting it, is \
                 left to the user. Ask them to fill it in themselves.",
            ));
        }
        // Safari's pop-up menus with no accessible options are set by the
        // driver through AppleScript against Safari's *front* document — any
        // window of it, not necessarily this one — and in the helper's name.
        // With no key to tell the application by, it could be Safari.
        if matches!(action, WindowAction::SetValue { .. })
            && found.role == "AXPopUpButton"
            && app_key.is_none_or(is_safari)
        {
            return Err(HelperError::new(
                HelperErrorCode::ActionFailed,
                "Choosing from a pop-up menu in Safari cannot be done by setting its value. \
                 Click the menu to open it, then click the option.",
            ));
        }
        Ok(())
    }
}
