use uuid::Uuid;

use super::super::error::{BrowserError, BrowserErrorCode, BrowserErrorContext};
use super::super::records::{remove_tab_record, TabRecord, TabTicket};
use super::super::state::BrowserState;
use super::super::types::BrowserTabStatus;

impl BrowserState {
    pub fn begin_tab_close(&mut self, tab_id: &str) -> Result<TabTicket, BrowserError> {
        let runtime_generation = self.runtime.generation;
        let tab = self
            .tabs
            .get_mut(tab_id)
            .ok_or_else(|| BrowserError::tab_not_found(tab_id))?;
        if tab.status == BrowserTabStatus::Closing {
            return closing_ticket(runtime_generation, tab);
        }
        tab.tab_generation = tab.tab_generation.saturating_add(1);
        tab.status = BrowserTabStatus::Closing;
        let operation_id = Uuid::new_v4().to_string();
        tab.operation_id = Some(operation_id.clone());
        Ok(TabTicket {
            operation_id,
            tab_id: tab_id.to_string(),
            runtime_generation,
            tab_generation: tab.tab_generation,
            view_generation: tab.view_generation,
        })
    }

    pub fn finish_tab_close(&mut self, ticket: &TabTicket) -> Result<(), BrowserError> {
        let Some(tab) = self.tabs.get(&ticket.tab_id) else {
            return Ok(());
        };
        validate_closing_ticket(tab, ticket)?;
        self.remove_closing_tab(&ticket.tab_id);
        Ok(())
    }

    pub(crate) fn discard_closing_tabs(&mut self) {
        let tab_ids = self
            .tabs
            .values()
            .filter(|tab| tab.status == BrowserTabStatus::Closing)
            .map(|tab| tab.id.clone())
            .collect::<Vec<_>>();
        for tab_id in tab_ids {
            self.remove_closing_tab(&tab_id);
        }
    }

    fn remove_closing_tab(&mut self, tab_id: &str) {
        let claim_ids = self
            .claims
            .iter()
            .filter(|(_, claim)| claim.tab_id == tab_id)
            .map(|(claim_id, _)| claim_id.clone())
            .collect::<Vec<_>>();
        for claim_id in claim_ids {
            self.abort_view_claim_unchecked(&claim_id);
        }
        self.clear_tab_cdp(tab_id);
        remove_tab_record(&mut self.tabs, &mut self.hosts, tab_id);
    }
}

fn validate_closing_ticket(tab: &TabRecord, ticket: &TabTicket) -> Result<(), BrowserError> {
    let valid = tab.status == BrowserTabStatus::Closing
        && tab.tab_generation == ticket.tab_generation
        && tab.operation_id.as_deref() == Some(&ticket.operation_id);
    valid.then_some(()).ok_or_else(|| stale_close(ticket))
}

fn closing_ticket(runtime_generation: u64, tab: &TabRecord) -> Result<TabTicket, BrowserError> {
    let operation_id = tab.operation_id.clone().ok_or_else(|| {
        BrowserError::new(
            BrowserErrorCode::BrowserInternal,
            "The closing browser tab has no cleanup operation",
        )
    })?;
    Ok(TabTicket {
        operation_id,
        tab_id: tab.id.clone(),
        runtime_generation,
        tab_generation: tab.tab_generation,
        view_generation: tab.view_generation,
    })
}

fn stale_close(ticket: &TabTicket) -> BrowserError {
    BrowserError::stale_generation(BrowserErrorContext {
        operation_id: Some(ticket.operation_id.clone()),
        browser_tab_id: Some(ticket.tab_id.clone()),
        runtime_generation: Some(ticket.runtime_generation),
        tab_generation: Some(ticket.tab_generation),
        view_generation: Some(ticket.view_generation),
        control_epoch: None,
        ..BrowserErrorContext::default()
    })
}
