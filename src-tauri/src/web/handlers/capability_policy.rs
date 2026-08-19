use std::sync::Arc;

use axum::{extract::Extension, Json};

use crate::acp::capability_policy::{CapabilityDecision, PolicySnapshotView};
use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::capability_policy::{
    self, CapabilityDecisionRequest, CapabilityPreferenceUpdateRequest, CapabilityPreferenceView,
    CapabilitySubjectRequest,
};

pub async fn snapshot(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<PolicySnapshotView>, AppCommandError> {
    Ok(Json(
        capability_policy::snapshot_core(&state.capability_policy).await,
    ))
}

pub async fn refresh(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<PolicySnapshotView>, AppCommandError> {
    Ok(Json(
        capability_policy::refresh_core(&state.db.conn, &state.capability_policy).await?,
    ))
}

pub async fn list_preferences(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<CapabilitySubjectRequest>,
) -> Result<Json<Vec<CapabilityPreferenceView>>, AppCommandError> {
    Ok(Json(
        capability_policy::list_preferences_core(&state.db.conn, request).await?,
    ))
}

pub async fn set_preference(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<CapabilityPreferenceUpdateRequest>,
) -> Result<Json<CapabilityPreferenceView>, AppCommandError> {
    Ok(Json(
        capability_policy::set_preference_core(&state.db.conn, request).await?,
    ))
}

pub async fn decision(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<CapabilityDecisionRequest>,
) -> Result<Json<CapabilityDecision>, AppCommandError> {
    Ok(Json(
        capability_policy::decision_core(
            &state.db.conn,
            &state.agent_catalog,
            &state.capability_policy,
            request,
        )
        .await?,
    ))
}
