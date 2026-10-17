use sea_orm::DatabaseConnection;

use crate::app_error::AppCommandError;
use crate::commands::{acp, managed_skills, office_tools};

pub(super) fn installed_for_root(root_skill_id: i64) -> Vec<String> {
    let central = acp::shared_skills_dir();
    office_tools::managed_office_skill_ids()
        .into_iter()
        .filter(|skill_id| {
            acp::read_market_skill_marker(&central.join(skill_id)).is_some_and(|marker| {
                marker.skill_id == root_skill_id
                    || marker.target_references.contains_key(&root_skill_id)
            })
        })
        .collect()
}

pub(super) async fn restore_after_uninstall(
    conn: &DatabaseConnection,
    skill_ids: Vec<String>,
) -> Result<(), AppCommandError> {
    let central = acp::shared_skills_dir();
    let removed = skill_ids
        .into_iter()
        .filter(|skill_id| !central.join(skill_id).join("SKILL.md").is_file())
        .collect::<Vec<_>>();
    if removed.is_empty() {
        return Ok(());
    }
    if !office_tools::officecli_detect().await.installed {
        tracing::info!(skill_ids = ?removed, "[skill-market] OfficeCLI runtime absent; no managed Skills to restore");
        return Ok(());
    }
    tracing::info!(skill_ids = ?removed, "[skill-market] restoring managed OfficeCLI Skills after uninstall");
    let report = office_tools::officecli_sync_skills_core()
        .await
        .map_err(|error| restore_error(error.to_string()))?;
    if !report.errors.is_empty() {
        return Err(restore_error(report.errors.join("\n")));
    }
    reconcile_restored_skills(conn).await?;
    tracing::info!(skill_ids = ?removed, "[skill-market] managed OfficeCLI Skills restored");
    Ok(())
}

async fn reconcile_restored_skills(conn: &DatabaseConnection) -> Result<(), AppCommandError> {
    let report = managed_skills::reconcile_persisted_family_core(
        conn,
        managed_skills::ManagedSkillFamily::OfficeTools,
    )
    .await?;
    let errors = report
        .results
        .iter()
        .filter(|result| !result.ok)
        .map(|result| {
            result
                .error
                .as_deref()
                .unwrap_or("Skill publication failed")
        })
        .collect::<Vec<_>>();
    if !errors.is_empty() {
        return Err(restore_error(errors.join("\n")));
    }
    Ok(())
}

fn restore_error(detail: String) -> AppCommandError {
    tracing::error!(error = %detail, "[skill-market] managed OfficeCLI Skill restore failed");
    AppCommandError::task_execution_failed(
        "Market Skill was removed but managed OfficeCLI Skill restore failed",
    )
    .with_detail(detail)
}
