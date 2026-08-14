use serde::{Deserialize, Serialize};

use super::types::BrowserGenerations;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserDialogKind {
    Alert,
    Confirm,
    Prompt,
    BeforeUnload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserDialogSnapshot {
    pub dialog_id: String,
    pub browser_tab_id: String,
    pub kind: BrowserDialogKind,
    pub message: String,
    pub default_prompt: String,
    pub generations: BrowserGenerations,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserFileChooserMode {
    SelectSingle,
    SelectMultiple,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserFileChooserSnapshot {
    pub chooser_id: String,
    pub browser_tab_id: String,
    pub mode: BrowserFileChooserMode,
    pub generations: BrowserGenerations,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserDownloadStatus {
    InProgress,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserDownloadSnapshot {
    pub download_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub browser_tab_id: Option<String>,
    pub suggested_filename: String,
    pub status: BrowserDownloadStatus,
    pub received_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_path: Option<String>,
}
