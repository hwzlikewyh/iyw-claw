use crate::browser::records::TabTicket;
use crate::browser::{BrowserError, BrowserErrorContext};

pub(super) fn navigation_error(error: BrowserError, ticket: &TabTicket) -> BrowserError {
    error.with_context(BrowserErrorContext {
        operation_id: Some(ticket.operation_id.clone()),
        browser_tab_id: Some(ticket.tab_id.clone()),
        runtime_generation: Some(ticket.runtime_generation),
        tab_generation: Some(ticket.tab_generation),
        ..BrowserErrorContext::default()
    })
}
