use reqwest::Method;
use serde::{Deserialize, Serialize};

use super::scenarios::Scenario;
use crate::app_error::AppCommandError;
use crate::db::AppDatabase;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioCase {
    #[serde(default)]
    pub model: Option<String>,
    pub id: String,
    pub category_key: String,
    pub scenario_id: String,
    pub title: String,
    pub summary: String,
    pub tag: String,
    pub cover_key: String,
    pub cover_tone: String,
    pub tag_at_footer: bool,
    pub sort_order: i32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScenarioCasesCatalog {
    pub items: Vec<ScenarioCase>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioCaseAsset {
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub step: Option<String>,
    pub file_name: String,
    pub kind: String,
    pub url: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScenarioCaseDetail {
    #[serde(flatten)]
    pub case: ScenarioCase,
    pub scenario: Scenario,
    pub assets: Vec<ScenarioCaseAsset>,
}

pub async fn scenario_cases_catalog_core(
    db: &AppDatabase,
) -> Result<ScenarioCasesCatalog, AppCommandError> {
    let request =
        super::skill_market::client::request(&db.conn, Method::GET, "/scenarios/cases/catalog")
            .await?;
    super::skill_market::parse_value(super::skill_market::client::send(request).await?, None)
}

pub async fn scenario_case_detail_core(
    db: &AppDatabase,
    id: String,
) -> Result<ScenarioCaseDetail, AppCommandError> {
    if id.parse::<i64>().ok().filter(|value| *value > 0).is_none() {
        return Err(AppCommandError::invalid_input("Invalid scenario case ID"));
    }
    let request =
        super::skill_market::client::request(&db.conn, Method::POST, "/scenarios/cases/detail")
            .await?
            .json(&serde_json::json!({ "id": id }));
    super::skill_market::parse_value(super::skill_market::client::send(request).await?, None)
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn scenario_cases_catalog(
    db: tauri::State<'_, AppDatabase>,
) -> Result<ScenarioCasesCatalog, AppCommandError> {
    scenario_cases_catalog_core(&db).await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn scenario_case_detail(
    db: tauri::State<'_, AppDatabase>,
    id: String,
) -> Result<ScenarioCaseDetail, AppCommandError> {
    scenario_case_detail_core(&db, id).await
}
