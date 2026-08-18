use crate::acp::capability_policy::{
    CapabilityDecision, CapabilityPolicyStore, PolicySnapshotView,
};
use crate::acp::version_center::CatalogStore;
use crate::app_error::AppCommandError;
use crate::commands::capability_policy::{
    self, CapabilityDecisionRequest, CapabilityPreferenceUpdateRequest, CapabilityPreferenceView,
    CapabilitySubjectRequest,
};
use crate::db::AppDatabase;

#[tauri::command]
pub async fn capability_policy_snapshot(
    store: tauri::State<'_, CapabilityPolicyStore>,
) -> Result<PolicySnapshotView, AppCommandError> {
    Ok(capability_policy::snapshot_core(&store).await)
}

#[tauri::command]
pub async fn capability_policy_refresh(
    db: tauri::State<'_, AppDatabase>,
    store: tauri::State<'_, CapabilityPolicyStore>,
) -> Result<PolicySnapshotView, AppCommandError> {
    capability_policy::refresh_core(&db.conn, &store).await
}

#[tauri::command]
pub async fn capability_preference_list(
    request: CapabilitySubjectRequest,
    db: tauri::State<'_, AppDatabase>,
) -> Result<Vec<CapabilityPreferenceView>, AppCommandError> {
    capability_policy::list_preferences_core(&db.conn, request).await
}

#[tauri::command]
pub async fn capability_preference_set(
    request: CapabilityPreferenceUpdateRequest,
    db: tauri::State<'_, AppDatabase>,
) -> Result<CapabilityPreferenceView, AppCommandError> {
    capability_policy::set_preference_core(&db.conn, request).await
}

#[tauri::command]
pub async fn capability_policy_decision(
    request: CapabilityDecisionRequest,
    db: tauri::State<'_, AppDatabase>,
    catalog: tauri::State<'_, CatalogStore>,
    store: tauri::State<'_, CapabilityPolicyStore>,
) -> Result<CapabilityDecision, AppCommandError> {
    capability_policy::decision_core(&db.conn, &catalog, &store, request).await
}
