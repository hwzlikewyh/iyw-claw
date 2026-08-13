use std::path::Path;

use sea_orm::DatabaseConnection;

use super::resumable::download_resumable;
use crate::acp::error::AcpError;
use crate::acp::version_center::capability::{self, RUNTIME};
use crate::acp::version_center::client::AgentPlatformClient;
use crate::acp::version_center::types::{AgentOffer, DownloadRequest, DownloadTicket};
use crate::app_error::{AppCommandError, AppErrorCode};

const MAX_TICKET_REFRESHES: u8 = 2;

pub(super) async fn download_archive(
    conn: &DatabaseConnection,
    offer: &AgentOffer,
    current_version: Option<&str>,
    channel: &str,
    archive: &Path,
    on_progress: &impl Fn(&str),
) -> Result<DownloadTicket, AcpError> {
    let mut ticket = request_ticket(conn, offer, current_version, channel).await?;
    on_progress("Downloading Agent artifact from version center");
    for refreshes in 0..=MAX_TICKET_REFRESHES {
        let result = download_resumable(
            offer.delivery.artifact_id.as_deref().unwrap_or_default(),
            &ticket.url,
            archive,
            ticket.size,
            &ticket.sha256,
            None,
        )
        .await;
        match result {
            Ok(()) => return Ok(ticket),
            Err(error)
                if error.code == AppErrorCode::AuthenticationFailed
                    && refreshes < MAX_TICKET_REFRESHES =>
            {
                let refreshed = request_ticket(conn, offer, current_version, channel).await?;
                validate_unchanged(&ticket, &refreshed)?;
                ticket = refreshed;
                on_progress("Agent download ticket refreshed");
            }
            Err(error) => return Err(map_error(error)),
        }
    }
    Err(AcpError::DownloadFailed(
        "Agent download ticket refresh limit reached".into(),
    ))
}

async fn request_ticket(
    conn: &DatabaseConnection,
    offer: &AgentOffer,
    current_version: Option<&str>,
    channel: &str,
) -> Result<DownloadTicket, AcpError> {
    let artifact_id = offer
        .delivery
        .artifact_id
        .as_deref()
        .ok_or_else(|| AcpError::DownloadFailed("binary Agent offer has no artifact".into()))?;
    let ticket = AgentPlatformClient::download_agent(
        conn,
        DownloadRequest {
            registry_id: Some(&offer.registry_id),
            tool_id: None,
            version_id: &offer.version_id,
            artifact_id,
            catalog_revision: offer.revision,
            current_version: current_version.unwrap_or_default(),
            client_version: env!("CARGO_PKG_VERSION"),
            runtime: RUNTIME,
            target: capability::current_target(),
            arch: capability::current_arch(),
            channel,
        },
    )
    .await
    .map_err(map_error)?;
    validate_ticket(&ticket).map_err(map_error)?;
    Ok(ticket)
}

fn validate_ticket(ticket: &DownloadTicket) -> Result<(), AppCommandError> {
    let parsed = reqwest::Url::parse(&ticket.url)
        .map_err(|_| AppCommandError::invalid_input("Agent download URL is invalid"))?;
    let local_debug = cfg!(debug_assertions) && parsed.host_str() == Some("127.0.0.1");
    let valid = (parsed.scheme() == "https" || local_debug)
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && ticket.size > 0
        && ticket.sha256.len() == 64
        && ticket.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        && !ticket.signature.trim().is_empty();
    valid
        .then_some(())
        .ok_or_else(|| AppCommandError::invalid_input("Agent download ticket was rejected"))
}

fn validate_unchanged(
    previous: &DownloadTicket,
    refreshed: &DownloadTicket,
) -> Result<(), AcpError> {
    if previous.size != refreshed.size
        || !previous.sha256.eq_ignore_ascii_case(&refreshed.sha256)
        || previous.file_name != refreshed.file_name
        || previous.signature != refreshed.signature
    {
        return Err(AcpError::DownloadFailed(
            "Agent artifact changed while refreshing its download ticket".into(),
        ));
    }
    Ok(())
}

fn map_error(error: AppCommandError) -> AcpError {
    let detail = error
        .detail
        .map(|detail| format!("{}: {detail}", error.message))
        .unwrap_or(error.message);
    AcpError::DownloadFailed(detail)
}
