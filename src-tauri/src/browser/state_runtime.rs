use uuid::Uuid;

use super::error::{BrowserError, BrowserErrorCode, BrowserErrorContext};
use super::records::{remove_tab_record, RuntimeStartDecision, RuntimeTicket};
use super::state::BrowserState;
use super::types::{BrowserRuntimeStatus, BrowserTabStatus};

impl BrowserState {
    pub fn begin_runtime_start(&mut self) -> Result<RuntimeStartDecision, BrowserError> {
        match self.runtime.status {
            BrowserRuntimeStatus::Ready | BrowserRuntimeStatus::Failed => {}
            BrowserRuntimeStatus::Running => {
                return Ok(RuntimeStartDecision::AlreadyRunning);
            }
            BrowserRuntimeStatus::Stopping => return Err(BrowserError::shutting_down()),
            _ => return Err(runtime_not_ready()),
        }
        self.runtime.generation = self.runtime.generation.saturating_add(1);
        self.runtime.status = BrowserRuntimeStatus::Starting;
        self.runtime.failure_code = None;
        let operation_id = Uuid::new_v4().to_string();
        self.runtime.operation_id = Some(operation_id.clone());
        Ok(RuntimeStartDecision::Start(RuntimeTicket {
            operation_id,
            generation: self.runtime.generation,
        }))
    }

    pub fn complete_runtime_start(&mut self, ticket: &RuntimeTicket) -> Result<(), BrowserError> {
        self.validate_runtime_ticket(ticket)?;
        self.runtime.status = BrowserRuntimeStatus::Running;
        self.runtime.operation_id = None;
        Ok(())
    }

    pub fn fail_runtime_start(
        &mut self,
        ticket: &RuntimeTicket,
        failure_code: impl Into<String>,
    ) -> Result<(), BrowserError> {
        self.validate_runtime_ticket(ticket)?;
        let claims = self.claims.keys().cloned().collect::<Vec<_>>();
        for claim_id in claims {
            self.abort_view_claim_unchecked(&claim_id);
        }
        let closing = self
            .tabs
            .values()
            .filter(|tab| {
                matches!(
                    tab.status,
                    BrowserTabStatus::Closing | BrowserTabStatus::Closed
                )
            })
            .map(|tab| tab.id.clone())
            .collect::<Vec<_>>();
        for tab_id in closing {
            self.clear_tab_cdp(&tab_id);
            remove_tab_record(&mut self.tabs, &mut self.hosts, &tab_id);
        }
        self.runtime.status = BrowserRuntimeStatus::Failed;
        self.runtime.operation_id = None;
        self.runtime.failure_code = Some(failure_code.into());
        Ok(())
    }

    pub fn record_runtime_exit(&mut self, generation: u64, failure_code: String) -> bool {
        if self.runtime.generation != generation
            || self.runtime.status != BrowserRuntimeStatus::Running
        {
            return false;
        }
        self.runtime.status = BrowserRuntimeStatus::Failed;
        self.runtime.operation_id = None;
        self.runtime.failure_code = Some(failure_code);
        for tab in self.tabs.values_mut() {
            tab.tab_generation = tab.tab_generation.saturating_add(1);
            tab.status = BrowserTabStatus::Crashed;
            tab.operation_id = None;
        }
        self.dialogs.clear();
        self.file_choosers.clear();
        true
    }

    pub fn begin_runtime_stop(&mut self) -> bool {
        if !matches!(
            self.runtime.status,
            BrowserRuntimeStatus::Starting
                | BrowserRuntimeStatus::Running
                | BrowserRuntimeStatus::Recovering
                | BrowserRuntimeStatus::Failed
        ) {
            return false;
        }
        self.runtime.generation = self.runtime.generation.saturating_add(1);
        self.runtime.status = BrowserRuntimeStatus::Stopping;
        self.runtime.failure_code = None;
        self.runtime.operation_id = Some(Uuid::new_v4().to_string());
        true
    }

    pub fn finish_runtime_stop(&mut self, failure_code: Option<String>) {
        self.runtime.operation_id = None;
        self.runtime.failure_code = failure_code;
        self.runtime.status = if self.runtime.failure_code.is_some() {
            BrowserRuntimeStatus::Failed
        } else {
            self.capability.status
        };
        if self.runtime.failure_code.is_none() {
            self.tabs.clear();
            self.claims.clear();
            self.hosts.clear();
            self.dialogs.clear();
            self.file_choosers.clear();
            self.downloads.clear();
        }
    }

    fn validate_runtime_ticket(&self, ticket: &RuntimeTicket) -> Result<(), BrowserError> {
        if self.runtime.generation == ticket.generation
            && self.runtime.operation_id.as_deref() == Some(&ticket.operation_id)
        {
            return Ok(());
        }
        Err(BrowserError::stale_generation(BrowserErrorContext {
            operation_id: Some(ticket.operation_id.clone()),
            runtime_generation: Some(ticket.generation),
            ..BrowserErrorContext::default()
        }))
    }
}

fn runtime_not_ready() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserRuntimeUnavailable,
        "The browser runtime is not ready to start",
    )
    .retryable(true)
}
