use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::error::BrowserError;
use super::process::{wait_for_pid_file, ProcessRecord};
use super::records::TabTicket;
use super::runtime::BrowserRuntimeContext;
use super::tab_binding::{binding_error, wait_for_binding, BINDING_TIMEOUT};
use super::tab_cleanup_registry::{PendingTabCleanupRegistry, TabCleanupToken};
use super::tab_metadata::page_metadata;
use super::tabs::TabRuntimeHandle;

const CREATE_TIMEOUT: Duration = Duration::from_secs(30);
pub(super) struct LaunchedTab {
    pub handle: TabRuntimeHandle,
    pub title: String,
    pub url: String,
}

pub(super) async fn launch_tab(
    cleanups: &PendingTabCleanupRegistry,
    runtime: &BrowserRuntimeContext,
    ticket: &TabTicket,
    url: &str,
    cancellation: CancellationToken,
) -> Result<LaunchedTab, BrowserError> {
    let session = tab_session(&ticket.tab_id);
    let owner = cleanups
        .begin_launch(runtime, ticket, session.clone(), true)
        .await?;
    let result = launch_tab_inner(
        cleanups,
        &owner,
        runtime,
        ticket,
        &session,
        url,
        cancellation.clone(),
    )
    .await;
    let result = reject_cancelled_launch(result, &cancellation);
    finish_owned_launch(cleanups, owner, result, true).await
}

pub(super) async fn bind_existing_tab(
    cleanups: &PendingTabCleanupRegistry,
    runtime: &BrowserRuntimeContext,
    ticket: &TabTicket,
    target_id: &str,
    cancellation: CancellationToken,
) -> Result<LaunchedTab, BrowserError> {
    let session = tab_session(&ticket.tab_id);
    let owner = cleanups
        .begin_launch(runtime, ticket, session.clone(), true)
        .await?;
    let result = bind_existing_inner(
        cleanups,
        &owner,
        runtime,
        ticket,
        &session,
        target_id,
        cancellation.clone(),
    )
    .await;
    let result = reject_cancelled_launch(result, &cancellation);
    finish_owned_launch(cleanups, owner, result, true).await
}

pub(super) async fn bind_existing_tab_preserving_target(
    cleanups: &PendingTabCleanupRegistry,
    runtime: &BrowserRuntimeContext,
    ticket: &TabTicket,
    target_id: &str,
    cancellation: CancellationToken,
) -> Result<LaunchedTab, BrowserError> {
    let session = tab_session(&ticket.tab_id);
    let owner = cleanups
        .begin_launch(runtime, ticket, session.clone(), false)
        .await?;
    let result = bind_existing_inner(
        cleanups,
        &owner,
        runtime,
        ticket,
        &session,
        target_id,
        cancellation.clone(),
    )
    .await;
    let result = reject_cancelled_launch(result, &cancellation);
    finish_owned_launch(cleanups, owner, result, false).await
}

async fn bind_existing_inner(
    cleanups: &PendingTabCleanupRegistry,
    owner: &TabCleanupToken,
    runtime: &BrowserRuntimeContext,
    ticket: &TabTicket,
    session: &str,
    target_id: &str,
    cancellation: CancellationToken,
) -> Result<LaunchedTab, BrowserError> {
    let response = runtime
        .cli
        .run_pinned(
            session,
            &runtime.cdp_url,
            &["tab", target_id],
            CREATE_TIMEOUT,
            cancellation.clone(),
        )
        .await?;
    let binding = wait_for_binding(&runtime.cli, session).await?;
    cleanups
        .record_target(owner, binding.target_id.clone())
        .await?;
    if binding.target_id != target_id {
        return Err(binding_error());
    }
    let daemon = wait_for_pid_file(
        &runtime.cli.pid_path(session),
        runtime.cli.executable_path(),
        BINDING_TIMEOUT,
    )
    .await?;
    cleanups.record_daemon(owner, daemon.clone()).await?;
    let (title, url) = page_metadata(
        &runtime.cli,
        session,
        &runtime.cdp_url,
        &response,
        cancellation,
    )
    .await?;
    Ok(LaunchedTab {
        handle: build_tab_handle(runtime, ticket, session, binding.target_id, daemon),
        title,
        url,
    })
}

async fn launch_tab_inner(
    cleanups: &PendingTabCleanupRegistry,
    owner: &TabCleanupToken,
    runtime: &BrowserRuntimeContext,
    ticket: &TabTicket,
    session: &str,
    url: &str,
    cancellation: CancellationToken,
) -> Result<LaunchedTab, BrowserError> {
    let response = runtime
        .cli
        .run_pinned(
            session,
            &runtime.cdp_url,
            &["open", url],
            CREATE_TIMEOUT,
            cancellation.clone(),
        )
        .await?;
    let binding = wait_for_binding(&runtime.cli, session).await?;
    cleanups
        .record_target(owner, binding.target_id.clone())
        .await?;
    let daemon = wait_for_pid_file(
        &runtime.cli.pid_path(session),
        runtime.cli.executable_path(),
        BINDING_TIMEOUT,
    )
    .await?;
    cleanups.record_daemon(owner, daemon.clone()).await?;
    let (title, actual_url) = page_metadata(
        &runtime.cli,
        session,
        &runtime.cdp_url,
        &response,
        cancellation,
    )
    .await?;
    Ok(LaunchedTab {
        handle: build_tab_handle(runtime, ticket, session, binding.target_id, daemon),
        title,
        url: actual_url,
    })
}

async fn finish_owned_launch(
    cleanups: &PendingTabCleanupRegistry,
    owner: TabCleanupToken,
    result: Result<LaunchedTab, BrowserError>,
    close_target: bool,
) -> Result<LaunchedTab, BrowserError> {
    match result {
        Ok(launched) => match cleanups.release_launch(&owner).await {
            Ok(()) => Ok(launched),
            Err(error) => {
                let _ = cleanups.cleanup_handle(launched.handle, close_target).await;
                Err(error)
            }
        },
        Err(error) => {
            cleanups.finish_failed_launch(owner).await;
            Err(error)
        }
    }
}

fn reject_cancelled_launch(
    result: Result<LaunchedTab, BrowserError>,
    cancellation: &CancellationToken,
) -> Result<LaunchedTab, BrowserError> {
    if cancellation.is_cancelled() {
        return Err(BrowserError::shutting_down());
    }
    result
}

fn build_tab_handle(
    runtime: &BrowserRuntimeContext,
    ticket: &TabTicket,
    session: &str,
    target_id: String,
    daemon: ProcessRecord,
) -> TabRuntimeHandle {
    TabRuntimeHandle {
        tab_id: ticket.tab_id.clone(),
        session: session.to_string(),
        target_id,
        runtime_generation: ticket.runtime_generation,
        cli: runtime.cli.clone(),
        cdp_url: runtime.cdp_url.clone(),
        controller_session: runtime.controller_session.clone(),
        daemon,
        cancellation: CancellationToken::new(),
    }
}

fn tab_session(tab_id: &str) -> String {
    let suffix: String = tab_id
        .chars()
        .filter(char::is_ascii_hexdigit)
        .take(12)
        .collect();
    format!("iyw-tab-{suffix}")
}
