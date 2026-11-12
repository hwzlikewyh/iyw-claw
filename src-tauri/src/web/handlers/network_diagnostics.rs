use std::sync::Arc;

use axum::{extract::Extension, Json};
use serde::Deserialize;

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::network_diagnostics::{self, NetworkProbeKind};

pub async fn get_network_traffic(
    Extension(_state): Extension<Arc<AppState>>,
) -> Result<Json<network_diagnostics::NetworkTrafficSnapshot>, AppCommandError> {
    Ok(Json(network_diagnostics::get_network_traffic_core().await?))
}

#[derive(Deserialize)]
pub struct NetworkProbeParams {
    kind: NetworkProbeKind,
}

pub async fn measure_network_speed(
    Extension(_state): Extension<Arc<AppState>>,
    Json(params): Json<NetworkProbeParams>,
) -> Result<Json<network_diagnostics::NetworkProbeResult>, AppCommandError> {
    Ok(Json(
        network_diagnostics::measure_network_speed_core(params.kind).await?,
    ))
}
