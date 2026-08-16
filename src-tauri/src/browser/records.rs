use std::collections::BTreeMap;
use std::time::Instant;

use super::error::{BrowserError, BrowserErrorCode};
use super::types::{
    BrowserControlStatus, BrowserGenerations, BrowserHostKind, BrowserHostSnapshot,
    BrowserRuntimeSnapshot, BrowserRuntimeStatus, BrowserTabSnapshot, BrowserTabStatus,
    BrowserViewStatus,
};

#[derive(Debug)]
pub(super) struct RuntimeRecord {
    pub status: BrowserRuntimeStatus,
    pub generation: u64,
    pub operation_id: Option<String>,
    pub failure_code: Option<String>,
}

#[derive(Debug)]
pub(super) struct TabRecord {
    pub id: String,
    pub target_id: Option<String>,
    pub title: String,
    pub url: String,
    pub status: BrowserTabStatus,
    pub view_status: BrowserViewStatus,
    pub tab_generation: u64,
    pub view_generation: u64,
    pub document_epoch: u64,
    pub host_id: Option<String>,
    pub operation_id: Option<String>,
}

impl TabRecord {
    pub fn creating(
        id: String,
        operation_id: String,
        url: String,
        host_id: Option<String>,
        view_status: BrowserViewStatus,
    ) -> Self {
        Self {
            id,
            target_id: None,
            title: "New tab".to_string(),
            url,
            status: BrowserTabStatus::Creating,
            view_status,
            tab_generation: 1,
            view_generation: 1,
            document_epoch: 0,
            host_id,
            operation_id: Some(operation_id),
        }
    }

    pub fn snapshot(&self, runtime: &RuntimeRecord) -> BrowserTabSnapshot {
        BrowserTabSnapshot {
            browser_tab_id: self.id.clone(),
            title: self.title.clone(),
            url: self.url.clone(),
            status: self.status,
            view_status: self.view_status,
            control_status: BrowserControlStatus::Idle,
            document_epoch: self.document_epoch,
            generations: BrowserGenerations {
                runtime_generation: runtime.generation,
                tab_generation: self.tab_generation,
                view_generation: self.view_generation,
                control_epoch: 0,
            },
            host_id: self.host_id.clone(),
        }
    }
}

#[derive(Debug)]
pub(super) struct HostRecord {
    pub id: String,
    pub window_label: String,
    pub kind: BrowserHostKind,
    pub generation: u64,
    pub visible: bool,
    pub tab_order: Vec<String>,
    pub active_tab_id: Option<String>,
    pub last_heartbeat: Instant,
}

impl HostRecord {
    pub fn snapshot(&self) -> BrowserHostSnapshot {
        BrowserHostSnapshot {
            host_id: self.id.clone(),
            window_label: self.window_label.clone(),
            kind: self.kind,
            generation: self.generation,
            visible: self.visible,
            tab_order: self.tab_order.clone(),
            active_tab_id: self.active_tab_id.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ViewClaimRecord {
    pub id: String,
    pub tab_id: String,
    pub source_host_id: Option<String>,
    pub target_host_id: String,
    pub target_index: usize,
    pub source_view_status: BrowserViewStatus,
    pub target_view_status: BrowserViewStatus,
    pub runtime_generation: u64,
    pub tab_generation: u64,
    pub source_view_generation: u64,
    pub target_view_generation: u64,
    pub source_host_generation: Option<u64>,
    pub target_host_generation: u64,
    pub first_frame_seq: Option<u64>,
    pub expires_at: Instant,
}

#[derive(Debug, Clone)]
pub(super) struct RuntimeTicket {
    pub operation_id: String,
    pub generation: u64,
}

#[derive(Debug, Clone)]
pub(super) enum RuntimeStartDecision {
    AlreadyRunning,
    Start(RuntimeTicket),
}

#[derive(Debug, Clone)]
pub(super) struct TabTicket {
    pub operation_id: String,
    pub tab_id: String,
    pub runtime_generation: u64,
    pub tab_generation: u64,
    pub view_generation: u64,
}

#[derive(Debug, Clone)]
pub(super) struct RecoveryTab {
    pub ticket: TabTicket,
    pub url: String,
    pub target_id: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct RecoveryPlan {
    pub runtime: RuntimeTicket,
    pub tabs: Vec<RecoveryTab>,
}

pub(super) fn runtime_snapshot(record: &RuntimeRecord) -> BrowserRuntimeSnapshot {
    BrowserRuntimeSnapshot {
        status: record.status,
        generation: record.generation,
        operation_id: record.operation_id.clone(),
        failure_code: record.failure_code.clone(),
    }
}

pub(super) fn attach_reserved_tab(
    hosts: &mut BTreeMap<String, HostRecord>,
    tabs: &mut BTreeMap<String, TabRecord>,
    host_id: &str,
    tab_id: &str,
) -> Result<(), BrowserError> {
    let Some(host) = hosts.get_mut(host_id) else {
        tabs.remove(tab_id);
        return Err(BrowserError::new(
            BrowserErrorCode::BrowserViewConflict,
            "The target browser view host does not exist",
        ));
    };
    host.tab_order.push(tab_id.to_string());
    host.active_tab_id = Some(tab_id.to_string());
    host.generation = host.generation.saturating_add(1);
    Ok(())
}

pub(super) fn remove_tab_record(
    tabs: &mut BTreeMap<String, TabRecord>,
    hosts: &mut BTreeMap<String, HostRecord>,
    tab_id: &str,
) {
    tabs.remove(tab_id);
    for host in hosts.values_mut() {
        let removed = host.tab_order.iter().any(|id| id == tab_id);
        host.tab_order.retain(|id| id != tab_id);
        if host.active_tab_id.as_deref() == Some(tab_id) {
            host.active_tab_id = host.tab_order.first().cloned();
        }
        if removed {
            host.generation = host.generation.saturating_add(1);
        }
    }
}
