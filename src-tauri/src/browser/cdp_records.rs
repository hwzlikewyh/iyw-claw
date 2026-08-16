use super::types_cdp::{BrowserDialogKind, BrowserDownloadStatus, BrowserFileChooserMode};

#[derive(Debug)]
pub(super) struct DialogRecord {
    pub id: String,
    pub tab_id: String,
    pub session_id: String,
    pub kind: BrowserDialogKind,
    pub message: String,
    pub default_prompt: String,
}

#[derive(Debug)]
pub(super) struct FileChooserRecord {
    pub id: String,
    pub tab_id: String,
    pub session_id: String,
    pub mode: BrowserFileChooserMode,
}

#[derive(Debug)]
pub(super) struct DownloadRecord {
    pub id: String,
    pub tab_id: Option<String>,
    pub suggested_filename: String,
    pub status: BrowserDownloadStatus,
    pub received_bytes: u64,
    pub total_bytes: Option<u64>,
    pub completed_path: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct PopupSeed {
    pub tab_id: String,
    pub tab_generation: u64,
    pub host_id: Option<String>,
}
