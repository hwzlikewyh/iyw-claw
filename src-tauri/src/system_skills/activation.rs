use std::path::Path;

use sea_orm::DatabaseConnection;

use crate::app_error::AppCommandError;
use crate::web::event_bridge::EventEmitter;

use super::checkout;
use super::git::CheckoutInfo;
use super::state::{self, SystemSkillsUpdateLifecycle, SystemSkillsUpdateState};
use super::storage;

pub struct PendingInstall<'a> {
    pub conn: &'a DatabaseConnection,
    pub data_dir: &'a Path,
    pub emitter: &'a EventEmitter,
    pub version: &'a str,
    pub commit: &'a str,
    pub ids: &'a [String],
    pub previous: Option<CheckoutInfo>,
}

pub async fn finish(
    request: PendingInstall<'_>,
) -> Result<SystemSkillsUpdateState, AppCommandError> {
    state::mutate(request.emitter, |value| {
        value.status = SystemSkillsUpdateLifecycle::Applying;
    });
    activate_or_restore(&request).await?;
    storage::persist_install(
        request.conn,
        request.version,
        request.commit,
        request.previous.as_ref(),
    )
    .await?;
    reconcile_distribution(request.conn, request.version).await;
    Ok(mark_installed(request))
}

async fn activate_or_restore(request: &PendingInstall<'_>) -> Result<(), AppCommandError> {
    if let Err(error) = crate::commands::experts::reconcile_system_repo_links(request.ids).await {
        checkout::restore_previous(
            &super::repository_dir(),
            request.previous.as_ref(),
            request.conn,
            request.data_dir,
        )
        .await;
        return Err(AppCommandError::external_command(
            "activate system skills",
            error.to_string(),
        ));
    }
    Ok(())
}

async fn reconcile_distribution(conn: &DatabaseConnection, version: &str) {
    if let Err(error) = crate::commands::managed_skills::reconcile_all_core(conn).await {
        tracing::warn!(
            target: "system_skills",
            version,
            "system skill distribution reconcile failed: {error}"
        );
    }
}

fn mark_installed(request: PendingInstall<'_>) -> SystemSkillsUpdateState {
    state::mutate(request.emitter, |value| {
        value.status = SystemSkillsUpdateLifecycle::UpToDate;
        value.current_version = Some(request.version.to_string());
        value.current_commit = Some(request.commit.to_string());
        value.previous_version = request.previous.and_then(|checkout| checkout.version);
        value.latest_version = Some(request.version.to_string());
        value.dirty = false;
        value.error = None;
    })
}
