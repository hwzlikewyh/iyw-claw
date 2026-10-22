use serde::{Deserialize, Serialize};

pub const MAX_SELECTION: usize = 32;
pub const PAGE_SIZE: usize = 100;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Choice {
    pub category: u8,
    pub kind: ChoiceKind,
    pub id: String,
    pub name: String,
    pub folder_id: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChoiceKind {
    Folder,
    File,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    pub scope: String,
    pub revision: u64,
    pub selected: Vec<Choice>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateSelection {
    pub conversation_id: i32,
    pub scope: String,
    pub revision: u64,
    pub selected: Vec<Choice>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    #[serde(rename = "folderId")]
    pub id: String,
    #[serde(rename = "folderName")]
    pub name: String,
    #[serde(default)]
    pub file_count: usize,
    #[serde(default)]
    pub children: Vec<Folder>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct File {
    #[serde(rename = "fileId")]
    pub id: String,
    #[serde(rename = "fileName")]
    pub name: String,
    pub folder_id: String,
    #[serde(default)]
    pub doc_id: Option<String>,
    #[serde(default)]
    pub doc_process_status: Option<i32>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePage {
    pub list: Vec<File>,
    pub total: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowseFiles {
    pub category: u8,
    pub folder_id: String,
    pub page: usize,
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && id.chars().all(|ch| ch.is_ascii_digit())
}

pub fn validate_category(category: u8) -> Result<(), crate::app_error::AppCommandError> {
    if category > 1 {
        return Err(crate::app_error::AppCommandError::invalid_input(
            "知识库分类无效",
        ));
    }
    Ok(())
}
