use std::collections::BTreeMap;

use super::cdp_records::{DialogRecord, DownloadRecord, FileChooserRecord};
use super::records::{runtime_snapshot, HostRecord, RuntimeRecord, TabRecord, ViewClaimRecord};
use super::types::{BrowserCapability, BrowserRuntimeStatus, BrowserStateSnapshot};
use super::types_cdp::{
    BrowserDialogSnapshot, BrowserDownloadSnapshot, BrowserFileChooserSnapshot,
};

pub(super) const MAX_BROWSER_TABS: usize = 20;

#[derive(Debug)]
pub(super) struct BrowserState {
    pub(super) capability: BrowserCapability,
    pub(super) runtime: RuntimeRecord,
    pub(super) tabs: BTreeMap<String, TabRecord>,
    pub(super) hosts: BTreeMap<String, HostRecord>,
    pub(super) claims: BTreeMap<String, ViewClaimRecord>,
    pub(super) dialogs: BTreeMap<String, DialogRecord>,
    pub(super) file_choosers: BTreeMap<String, FileChooserRecord>,
    pub(super) downloads: BTreeMap<String, DownloadRecord>,
}

impl BrowserState {
    pub fn new(capability: BrowserCapability) -> Self {
        let status = capability.status;
        Self {
            capability,
            runtime: RuntimeRecord {
                status,
                generation: 0,
                operation_id: None,
                failure_code: None,
            },
            tabs: BTreeMap::new(),
            hosts: BTreeMap::new(),
            claims: BTreeMap::new(),
            dialogs: BTreeMap::new(),
            file_choosers: BTreeMap::new(),
            downloads: BTreeMap::new(),
        }
    }

    pub fn set_capability(&mut self, capability: BrowserCapability) {
        self.capability = capability;
        if !matches!(
            self.runtime.status,
            BrowserRuntimeStatus::Starting
                | BrowserRuntimeStatus::Running
                | BrowserRuntimeStatus::Recovering
                | BrowserRuntimeStatus::Stopping
        ) {
            self.runtime.status = self.capability.status;
            self.runtime.failure_code = None;
        }
    }

    pub fn snapshot(&self) -> BrowserStateSnapshot {
        BrowserStateSnapshot {
            state_revision: 0,
            capability: self.capability.clone(),
            runtime: runtime_snapshot(&self.runtime),
            tabs: self
                .tabs
                .values()
                .map(|tab| tab.snapshot(&self.runtime))
                .collect(),
            hosts: self.hosts.values().map(HostRecord::snapshot).collect(),
            dialogs: self
                .dialogs
                .values()
                .filter_map(|item| {
                    Some(BrowserDialogSnapshot {
                        dialog_id: item.id.clone(),
                        browser_tab_id: item.tab_id.clone(),
                        kind: item.kind,
                        message: item.message.clone(),
                        default_prompt: item.default_prompt.clone(),
                        generations: self.generations_for_snapshot(&item.tab_id)?,
                    })
                })
                .collect(),
            file_choosers: self
                .file_choosers
                .values()
                .filter_map(|item| {
                    Some(BrowserFileChooserSnapshot {
                        chooser_id: item.id.clone(),
                        browser_tab_id: item.tab_id.clone(),
                        mode: item.mode,
                        generations: self.generations_for_snapshot(&item.tab_id)?,
                    })
                })
                .collect(),
            downloads: self
                .downloads
                .values()
                .map(|item| BrowserDownloadSnapshot {
                    download_id: item.id.clone(),
                    browser_tab_id: item.tab_id.clone(),
                    suggested_filename: item.suggested_filename.clone(),
                    status: item.status,
                    received_bytes: item.received_bytes,
                    total_bytes: item.total_bytes,
                    completed_path: item.completed_path.clone(),
                })
                .collect(),
            view_claims: self.claim_snapshots(),
        }
    }

    fn generations_for_snapshot(&self, tab_id: &str) -> Option<super::types::BrowserGenerations> {
        let tab = self.tabs.get(tab_id)?;
        Some(super::types::BrowserGenerations {
            runtime_generation: self.runtime.generation,
            tab_generation: tab.tab_generation,
            view_generation: tab.view_generation,
            control_epoch: 0,
        })
    }
}
