use uuid::Uuid;

use super::error::{BrowserError, BrowserErrorCode};
use super::records::{RecoveryPlan, RecoveryTab, RuntimeTicket, TabTicket};
use super::state::BrowserState;
use super::types::{BrowserRuntimeStatus, BrowserTabStatus};

impl BrowserState {
    pub fn begin_runtime_recovery(&mut self, failed_generation: u64) -> Option<RecoveryPlan> {
        if self.runtime.generation != failed_generation
            || self.runtime.status != BrowserRuntimeStatus::Failed
        {
            return None;
        }
        self.discard_closing_tabs();
        self.runtime.generation = self.runtime.generation.saturating_add(1);
        self.runtime.status = BrowserRuntimeStatus::Recovering;
        self.runtime.failure_code = None;
        let operation_id = Uuid::new_v4().to_string();
        self.runtime.operation_id = Some(operation_id.clone());
        let runtime = RuntimeTicket {
            operation_id,
            generation: self.runtime.generation,
        };
        let tabs = self
            .tabs
            .values_mut()
            .map(|tab| recovery_tab(tab, runtime.generation))
            .collect();
        Some(RecoveryPlan { runtime, tabs })
    }

    pub fn begin_tab_recovery(&mut self, tab_id: &str) -> Result<RecoveryTab, BrowserError> {
        if self.runtime.status != BrowserRuntimeStatus::Running {
            return Err(recovery_unavailable());
        }
        let recoverable = self
            .tabs
            .get(tab_id)
            .ok_or_else(|| BrowserError::tab_not_found(tab_id))?
            .status;
        if !matches!(
            recoverable,
            BrowserTabStatus::Crashed | BrowserTabStatus::Gone
        ) {
            return Err(BrowserError::new(
                BrowserErrorCode::BrowserControlChanged,
                "The browser tab is not awaiting recovery",
            )
            .retryable(true));
        }
        self.clear_tab_cdp(tab_id);
        let runtime_generation = self.runtime.generation;
        let tab = self.tabs.get_mut(tab_id).expect("validated browser tab");
        Ok(prepare_recovery_tab(tab, runtime_generation, true))
    }

    pub fn fail_recovery_tab(&mut self, ticket: &TabTicket) {
        let Some(tab) = self.tabs.get_mut(&ticket.tab_id) else {
            return;
        };
        if self.runtime.generation == ticket.runtime_generation
            && tab.tab_generation == ticket.tab_generation
            && tab.operation_id.as_deref() == Some(&ticket.operation_id)
        {
            tab.status = BrowserTabStatus::Crashed;
            tab.operation_id = None;
        }
    }

    pub fn fail_recovery_plan(&mut self, plan: &RecoveryPlan, failure_code: String) {
        if self.runtime.generation != plan.runtime.generation {
            return;
        }
        self.runtime.status = BrowserRuntimeStatus::Failed;
        self.runtime.operation_id = None;
        self.runtime.failure_code = Some(failure_code);
        for tab in &plan.tabs {
            self.fail_recovery_tab(&tab.ticket);
        }
    }
}

fn recovery_tab(tab: &mut super::records::TabRecord, runtime_generation: u64) -> RecoveryTab {
    prepare_recovery_tab(tab, runtime_generation, false)
}

fn prepare_recovery_tab(
    tab: &mut super::records::TabRecord,
    runtime_generation: u64,
    preserve_target: bool,
) -> RecoveryTab {
    tab.tab_generation = tab.tab_generation.saturating_add(1);
    let target_id = preserve_target.then(|| tab.target_id.clone()).flatten();
    if !preserve_target {
        tab.target_id = None;
    }
    tab.status = BrowserTabStatus::Creating;
    let operation_id = Uuid::new_v4().to_string();
    tab.operation_id = Some(operation_id.clone());
    RecoveryTab {
        ticket: TabTicket {
            operation_id,
            tab_id: tab.id.clone(),
            runtime_generation,
            tab_generation: tab.tab_generation,
            view_generation: tab.view_generation,
        },
        url: tab.url.clone(),
        target_id,
    }
}

fn recovery_unavailable() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserRuntimeUnavailable,
        "The browser runtime is unavailable for tab recovery",
    )
    .retryable(true)
}
