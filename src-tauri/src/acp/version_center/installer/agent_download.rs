use std::path::Path;

use sea_orm::DatabaseConnection;

use super::resumable::download_resumable;
use crate::acp::error::AcpError;
use crate::acp::version_center::capability::{self, RUNTIME};
use crate::acp::version_center::client::AgentPlatformClient;
use crate::acp::version_center::types::{AgentOffer, DownloadRequest, DownloadTicket};
use crate::app_error::{AppCommandError, AppErrorCode};

const MAX_TICKET_REFRESHES: u8 = 2;

pub(super) enum AgentDownloadError {
    Unavailable(AcpError),
    Rejected(AcpError),
}

impl AgentDownloadError {
    pub(super) fn is_unavailable(&self) -> bool {
        matches!(self, Self::Unavailable(_))
    }

    pub(super) fn into_error(self) -> AcpError {
        match self {
            Self::Unavailable(error) | Self::Rejected(error) => error,
        }
    }
}

pub(super) async fn download_archive(
    conn: &DatabaseConnection,
    offer: &AgentOffer,
    current_version: Option<&str>,
    channel: &str,
    archive: &Path,
    on_progress: &impl Fn(&str),
    allow_policy_missing: bool,
) -> Result<DownloadTicket, AgentDownloadError> {
    let mut ticket =
        request_ticket(conn, offer, current_version, channel, allow_policy_missing).await?;
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
                let refreshed =
                    request_ticket(conn, offer, current_version, channel, allow_policy_missing)
                        .await?;
                validate_unchanged(&ticket, &refreshed).map_err(AgentDownloadError::Rejected)?;
                ticket = refreshed;
                on_progress("Agent download ticket refreshed");
            }
            Err(error) => return Err(classify_error(error, allow_policy_missing)),
        }
    }
    Err(AgentDownloadError::Rejected(AcpError::DownloadFailed(
        "Agent download ticket refresh limit reached".into(),
    )))
}

async fn request_ticket(
    conn: &DatabaseConnection,
    offer: &AgentOffer,
    current_version: Option<&str>,
    channel: &str,
    allow_policy_missing: bool,
) -> Result<DownloadTicket, AgentDownloadError> {
    let artifact_id = offer.delivery.artifact_id.as_deref().ok_or_else(|| {
        AgentDownloadError::Rejected(AcpError::DownloadFailed(
            "binary Agent offer has no artifact".into(),
        ))
    })?;
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
    .map_err(|error| classify_error(error, allow_policy_missing))?;
    validate_ticket(&ticket).map_err(rejected_error)?;
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

fn classify_error(error: AppCommandError, allow_policy_missing: bool) -> AgentDownloadError {
    let unavailable = fallback_allowed(&error, allow_policy_missing);
    let error = app_error(error);
    if unavailable {
        AgentDownloadError::Unavailable(error)
    } else {
        AgentDownloadError::Rejected(error)
    }
}

fn rejected_error(error: AppCommandError) -> AgentDownloadError {
    AgentDownloadError::Rejected(app_error(error))
}

fn fallback_allowed(error: &AppCommandError, allow_policy_missing: bool) -> bool {
    crate::acp::version_center::fallback::allowed(error, allow_policy_missing)
}

fn app_error(error: AppCommandError) -> AcpError {
    let detail = error
        .detail
        .map(|detail| format!("{}: {detail}", error.message))
        .unwrap_or(error.message);
    AcpError::DownloadFailed(detail)
}
