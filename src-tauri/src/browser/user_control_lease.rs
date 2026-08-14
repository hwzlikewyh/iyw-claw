use super::control::ControlGate;

pub(super) struct UserControlLease {
    gate: ControlGate,
    completed: bool,
}

impl UserControlLease {
    pub(super) fn new(gate: ControlGate) -> Self {
        Self {
            gate,
            completed: false,
        }
    }

    pub(super) async fn finish(mut self) {
        self.completed = true;
        self.gate.complete_user().await;
    }
}

impl Drop for UserControlLease {
    fn drop(&mut self) {
        if self.completed {
            return;
        }
        let gate = self.gate.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move { gate.complete_user().await });
        }
    }
}
