use super::control::ControlGate;

pub(super) struct QueuedAgentWaiter {
    gate: ControlGate,
    operation_id: String,
    armed: bool,
}

impl QueuedAgentWaiter {
    pub(super) fn new(gate: ControlGate, operation_id: String) -> Self {
        Self {
            gate,
            operation_id,
            armed: true,
        }
    }

    pub(super) fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for QueuedAgentWaiter {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let gate = self.gate.clone();
        let operation_id = self.operation_id.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move { gate.remove_waiter(&operation_id).await });
        }
    }
}
