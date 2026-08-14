use uuid::Uuid;

use super::error::{BrowserError, BrowserErrorCode, BrowserErrorContext};
use super::records::{attach_reserved_tab, remove_tab_record, TabRecord, TabTicket};
use super::state::{BrowserState, MAX_BROWSER_TABS};
use super::types::{
    AgentAccess, BrowserHostKind, BrowserRuntimeStatus, BrowserTabStatus, BrowserViewStatus,
};

impl BrowserState {
    pub fn set_tab_agent_access(
        &mut self,
        tab_id: &str,
        access: AgentAccess,
    ) -> Result<(), BrowserError> {
        let tab = self
            .tabs
            .get_mut(tab_id)
            .ok_or_else(|| BrowserError::tab_not_found(tab_id))?;
        if matches!(
            tab.status,
            BrowserTabStatus::Closing | BrowserTabStatus::Closed
        ) {
            return Err(BrowserError::new(
                BrowserErrorCode::BrowserTabGone,
                "The browser tab is closing",
            ));
        }
        if tab.agent_access != access {
            tab.agent_access = access;
            tab.access_generation = tab.access_generation.saturating_add(1);
        }
        Ok(())
    }

    pub fn reserve_tab(
        &mut self,
        url: String,
        access: AgentAccess,
        host_id: Option<String>,
    ) -> Result<TabTicket, BrowserError> {
        if self.runtime.status != BrowserRuntimeStatus::Running {
            return Err(runtime_unavailable());
        }
        if self.tabs.len() >= MAX_BROWSER_TABS {
            return Err(BrowserError::new(
                BrowserErrorCode::BrowserTabLimit,
                "The browser tab limit has been reached",
            ));
        }
        let tab_id = Uuid::new_v4().to_string();
        let operation_id = Uuid::new_v4().to_string();
        let view_status = self.view_status(host_id.as_deref());
        let tab = TabRecord::creating(
            tab_id.clone(),
            operation_id.clone(),
            url,
            access,
            host_id.clone(),
            view_status,
        );
        self.tabs.insert(tab_id.clone(), tab);
        if let Some(host_id) = host_id {
            attach_reserved_tab(&mut self.hosts, &mut self.tabs, &host_id, &tab_id)?;
        }
        Ok(TabTicket {
            operation_id,
            tab_id,
            runtime_generation: self.runtime.generation,
            tab_generation: 1,
            view_generation: 1,
        })
    }

    pub fn commit_tab_live(
        &mut self,
        ticket: &TabTicket,
        target_id: String,
        title: String,
        url: String,
    ) -> Result<(), BrowserError> {
        self.validate_tab_ticket(ticket)?;
        if self.target_is_owned(&ticket.tab_id, &target_id) {
            return Err(BrowserError::new(
                BrowserErrorCode::BrowserInternal,
                "The browser target is already owned by another logical tab",
            ));
        }
        let tab = self.tabs.get_mut(&ticket.tab_id).expect("validated tab");
        tab.target_id = Some(target_id);
        tab.title = title;
        tab.url = url;
        tab.status = BrowserTabStatus::Live;
        tab.operation_id = None;
        Ok(())
    }

    pub fn rollback_tab(&mut self, ticket: &TabTicket) -> Result<(), BrowserError> {
        self.validate_tab_ticket(ticket)?;
        self.clear_tab_cdp(&ticket.tab_id);
        remove_tab_record(&mut self.tabs, &mut self.hosts, &ticket.tab_id);
        Ok(())
    }

    pub fn begin_tab_navigation(&mut self, tab_id: &str) -> Result<TabTicket, BrowserError> {
        let runtime_generation = self.runtime.generation;
        let tab = self.live_tab_mut(tab_id)?;
        tab.tab_generation = tab.tab_generation.saturating_add(1);
        tab.status = BrowserTabStatus::Navigating;
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

    pub fn finish_tab_navigation(
        &mut self,
        ticket: &TabTicket,
        title: String,
        url: String,
    ) -> Result<(), BrowserError> {
        self.validate_tab_ticket(ticket)?;
        let tab = self.tabs.get_mut(&ticket.tab_id).expect("validated tab");
        tab.title = title;
        tab.url = url;
        tab.status = BrowserTabStatus::Live;
        tab.document_epoch = tab.document_epoch.saturating_add(1);
        tab.operation_id = None;
        Ok(())
    }

    pub fn fail_tab_navigation(&mut self, ticket: &TabTicket) -> Result<(), BrowserError> {
        self.validate_tab_ticket(ticket)?;
        let tab = self.tabs.get_mut(&ticket.tab_id).expect("validated tab");
        tab.status = BrowserTabStatus::Live;
        tab.operation_id = None;
        Ok(())
    }

    pub fn mark_tab_gone(&mut self, ticket: &TabTicket) -> Result<(), BrowserError> {
        self.validate_tab_ticket(ticket)?;
        let tab = self.tabs.get_mut(&ticket.tab_id).expect("validated tab");
        tab.status = BrowserTabStatus::Gone;
        tab.operation_id = None;
        Ok(())
    }

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
        self.validate_tab_ticket(ticket)?;
        self.clear_tab_cdp(&ticket.tab_id);
        remove_tab_record(&mut self.tabs, &mut self.hosts, &ticket.tab_id);
        Ok(())
    }

    pub fn record_tab_crash(&mut self, tab_id: &str, runtime_generation: u64) -> bool {
        if self.runtime.generation != runtime_generation {
            return false;
        }
        {
            let Some(tab) = self.tabs.get_mut(tab_id) else {
                return false;
            };
            if matches!(
                tab.status,
                BrowserTabStatus::Closing | BrowserTabStatus::Closed
            ) {
                return false;
            }
            tab.tab_generation = tab.tab_generation.saturating_add(1);
            tab.status = BrowserTabStatus::Crashed;
            tab.operation_id = None;
        }
        self.clear_tab_cdp(tab_id);
        true
    }

    pub(super) fn validate_tab_ticket(&self, ticket: &TabTicket) -> Result<(), BrowserError> {
        let tab = self
            .tabs
            .get(&ticket.tab_id)
            .ok_or_else(|| BrowserError::tab_not_found(&ticket.tab_id))?;
        if self.runtime.generation == ticket.runtime_generation
            && tab.tab_generation == ticket.tab_generation
            && tab.view_generation == ticket.view_generation
            && tab.operation_id.as_deref() == Some(&ticket.operation_id)
        {
            return Ok(());
        }
        Err(BrowserError::stale_generation(BrowserErrorContext {
            operation_id: Some(ticket.operation_id.clone()),
            browser_tab_id: Some(ticket.tab_id.clone()),
            runtime_generation: Some(ticket.runtime_generation),
            tab_generation: Some(ticket.tab_generation),
            view_generation: Some(ticket.view_generation),
            control_epoch: None,
        }))
    }

    fn live_tab_mut(&mut self, tab_id: &str) -> Result<&mut TabRecord, BrowserError> {
        let tab = self
            .tabs
            .get_mut(tab_id)
            .ok_or_else(|| BrowserError::tab_not_found(tab_id))?;
        match tab.status {
            BrowserTabStatus::Live => Ok(tab),
            BrowserTabStatus::Crashed | BrowserTabStatus::Gone => Err(BrowserError::new(
                BrowserErrorCode::BrowserTabCrashed,
                "The browser tab is unavailable",
            )),
            _ => Err(BrowserError::new(
                BrowserErrorCode::BrowserControlChanged,
                "The browser tab is busy",
            )
            .retryable(true)),
        }
    }

    fn target_is_owned(&self, tab_id: &str, target_id: &str) -> bool {
        self.tabs
            .values()
            .any(|tab| tab.id != tab_id && tab.target_id.as_deref() == Some(target_id))
    }

    fn view_status(&self, host_id: Option<&str>) -> BrowserViewStatus {
        host_id
            .and_then(|id| self.hosts.get(id))
            .map(|host| match host.kind {
                BrowserHostKind::Docked => BrowserViewStatus::Docked,
                BrowserHostKind::Detached => BrowserViewStatus::Detached,
            })
            .unwrap_or(BrowserViewStatus::Unclaimed)
    }
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

fn runtime_unavailable() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserRuntimeUnavailable,
        "The browser runtime is not running",
    )
    .retryable(true)
}
