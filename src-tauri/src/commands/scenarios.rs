use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::app_error::AppCommandError;
use crate::db::AppDatabase;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioCategory {
    pub id: String,
    pub key: String,
    pub display_name: String,
    #[serde(default)]
    pub icon: Option<String>,
    pub sort_order: i32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scenario {
    pub id: String,
    pub category_key: String,
    pub slug: String,
    pub display_name: String,
    pub summary: String,
    pub prompt_template: String,
    pub skill_package_id: String,
    pub skill_package_slug: String,
    pub skill_package_version: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub tone: Option<String>,
    pub sort_order: i32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioCatalog {
    pub revision: String,
    pub categories: Vec<ScenarioCategory>,
    pub scenarios: Vec<Scenario>,
}

pub async fn scenarios_catalog_core(db: &AppDatabase) -> Result<ScenarioCatalog, AppCommandError> {
    let builder =
        super::skill_market::client::request(&db.conn, Method::GET, "/scenarios/catalog").await?;
    super::skill_market::parse_value(super::skill_market::client::send(builder).await?, None)
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn scenarios_catalog(
    db: tauri::State<'_, AppDatabase>,
) -> Result<ScenarioCatalog, AppCommandError> {
    scenarios_catalog_core(&db).await
}
