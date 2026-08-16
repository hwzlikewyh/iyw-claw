use uuid::Uuid;

use super::cdp_records::{DialogRecord, DownloadRecord, FileChooserRecord, PopupSeed};
use super::error::{BrowserError, BrowserErrorContext};
use super::state::BrowserState;
use super::types::{BrowserGenerations, BrowserTabStatus};
use super::types_cdp::{BrowserDialogKind, BrowserDownloadStatus, BrowserFileChooserMode};

impl BrowserState {
    pub fn popup_seed(&self, opener_target_id: &str) -> Option<PopupSeed> {
        self.tabs
            .values()
            .find(|tab| {
                tab.target_id.as_deref() == Some(opener_target_id)
                    && matches!(
                        tab.status,
                        BrowserTabStatus::Live | BrowserTabStatus::Navigating
                    )
            })
            .map(|tab| PopupSeed {
                tab_id: tab.id.clone(),
                tab_generation: tab.tab_generation,
                host_id: tab.host_id.clone(),
            })
    }

    pub fn commit_popup_live(
        &mut self,
        ticket: &super::records::TabTicket,
        opener_target_id: &str,
        seed: &PopupSeed,
        target_id: String,
        title: String,
        url: String,
    ) -> Result<(), super::error::BrowserError> {
        self.validate_tab_ticket(ticket)?;
        let opener_matches = self.tabs.values().any(|tab| {
            tab.id == seed.tab_id
                && tab.target_id.as_deref() == Some(opener_target_id)
                && tab.tab_generation == seed.tab_generation
                && matches!(
                    tab.status,
                    BrowserTabStatus::Live | BrowserTabStatus::Navigating
                )
        });
        if !opener_matches {
            return Err(BrowserError::stale_generation(BrowserErrorContext {
                operation_id: Some(ticket.operation_id.clone()),
                browser_tab_id: Some(seed.tab_id.clone()),
                runtime_generation: Some(ticket.runtime_generation),
                tab_generation: Some(seed.tab_generation),
                ..BrowserErrorContext::default()
            }));
        }
        self.commit_tab_live(ticket, target_id, title, url)
    }

    pub fn tab_id_for_target(&self, target_id: &str) -> Option<String> {
        self.tabs
            .values()
            .find(|tab| tab.target_id.as_deref() == Some(target_id))
            .map(|tab| tab.id.clone())
    }

    pub fn update_target_info(&mut self, target_id: &str, title: String, url: String) {
        let Some(tab) = self
            .tabs
            .values_mut()
            .find(|tab| tab.target_id.as_deref() == Some(target_id))
        else {
            return;
        };
        tab.title = title;
        tab.url = url;
    }

    pub fn record_target_failure(&mut self, target_id: &str, crashed: bool) -> Option<String> {
        let tab_id = {
            let tab = self
                .tabs
                .values_mut()
                .find(|tab| tab.target_id.as_deref() == Some(target_id))?;
            if matches!(
                tab.status,
                BrowserTabStatus::Closing | BrowserTabStatus::Closed
            ) {
                return None;
            }
            tab.tab_generation = tab.tab_generation.saturating_add(1);
            tab.status = if crashed {
                BrowserTabStatus::Crashed
            } else {
                BrowserTabStatus::Gone
            };
            tab.operation_id = None;
            tab.id.clone()
        };
        self.clear_tab_cdp(&tab_id);
        Some(tab_id)
    }

    pub fn record_document_init(&mut self, target_id: &str) {
        if let Some(tab) = self
            .tabs
            .values_mut()
            .find(|tab| tab.target_id.as_deref() == Some(target_id))
        {
            tab.document_epoch = tab.document_epoch.saturating_add(1);
        }
    }

    pub fn open_dialog(
        &mut self,
        target_id: &str,
        session_id: String,
        kind: BrowserDialogKind,
        message: String,
        default_prompt: String,
    ) {
        let Some(tab_id) = self.tab_id_for_target(target_id) else {
            return;
        };
        self.dialogs.retain(|_, record| record.tab_id != tab_id);
        let id = Uuid::new_v4().to_string();
        self.dialogs.insert(
            id.clone(),
            DialogRecord {
                id,
                tab_id,
                session_id,
                kind,
                message,
                default_prompt,
            },
        );
    }

    pub fn close_dialog_for_target(&mut self, target_id: &str) {
        let Some(tab_id) = self.tab_id_for_target(target_id) else {
            return;
        };
        self.dialogs.retain(|_, record| record.tab_id != tab_id);
    }

    pub fn open_file_chooser(
        &mut self,
        target_id: &str,
        session_id: String,
        mode: BrowserFileChooserMode,
    ) {
        let Some(tab_id) = self.tab_id_for_target(target_id) else {
            return;
        };
        self.file_choosers
            .retain(|_, record| record.tab_id != tab_id);
        let id = Uuid::new_v4().to_string();
        self.file_choosers.insert(
            id.clone(),
            FileChooserRecord {
                id,
                tab_id,
                session_id,
                mode,
            },
        );
    }

    pub fn begin_download(
        &mut self,
        id: String,
        target_id: Option<&str>,
        suggested_filename: String,
    ) {
        let tab_id = target_id.and_then(|target| self.tab_id_for_target(target));
        self.downloads.insert(
            id.clone(),
            DownloadRecord {
                id,
                tab_id,
                suggested_filename,
                status: BrowserDownloadStatus::InProgress,
                received_bytes: 0,
                total_bytes: None,
                completed_path: None,
            },
        );
    }

    pub fn update_download(
        &mut self,
        id: &str,
        status: BrowserDownloadStatus,
        received_bytes: u64,
        total_bytes: Option<u64>,
        path: Option<String>,
    ) {
        let Some(download) = self.downloads.get_mut(id) else {
            return;
        };
        download.status = status;
        download.received_bytes = received_bytes;
        download.total_bytes = total_bytes;
        if path.is_some() {
            download.completed_path = path;
        }
    }

    pub fn dialog_command(&self, id: &str) -> Option<(String, BrowserGenerations)> {
        let record = self.dialogs.get(id)?;
        Some((
            record.session_id.clone(),
            self.generations_for(&record.tab_id)?,
        ))
    }

    pub fn chooser_command(&self, id: &str) -> Option<(String, BrowserGenerations)> {
        let record = self.file_choosers.get(id)?;
        Some((
            record.session_id.clone(),
            self.generations_for(&record.tab_id)?,
        ))
    }

    pub fn finish_dialog(&mut self, id: &str) {
        self.dialogs.remove(id);
    }

    pub fn finish_chooser(&mut self, id: &str) {
        self.file_choosers.remove(id);
    }

    pub fn completed_download_path(&self, id: &str) -> Option<std::path::PathBuf> {
        let download = self.downloads.get(id)?;
        (download.status == BrowserDownloadStatus::Completed)
            .then(|| {
                download
                    .completed_path
                    .as_ref()
                    .map(std::path::PathBuf::from)
            })
            .flatten()
    }

    pub fn clear_tab_cdp(&mut self, tab_id: &str) {
        self.dialogs.retain(|_, record| record.tab_id != tab_id);
        self.file_choosers
            .retain(|_, record| record.tab_id != tab_id);
    }

    fn generations_for(&self, tab_id: &str) -> Option<BrowserGenerations> {
        let tab = self.tabs.get(tab_id)?;
        Some(BrowserGenerations {
            runtime_generation: self.runtime.generation,
            tab_generation: tab.tab_generation,
            view_generation: tab.view_generation,
            control_epoch: 0,
        })
    }
}
