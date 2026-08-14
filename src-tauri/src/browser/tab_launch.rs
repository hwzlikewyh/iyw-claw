use std::path::Path;
use std::time::{Duration, Instant};

use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use super::command_runner::AgentBrowserCli;
use super::error::{BrowserError, BrowserErrorCode};
use super::process::{kill_tree_checked, wait_for_exit, wait_for_pid_file, ProcessRecord};
use super::records::TabTicket;
use super::runtime::BrowserRuntimeContext;
use super::tab_metadata::page_metadata;
use super::tabs::TabRuntimeHandle;

const CREATE_TIMEOUT: Duration = Duration::from_secs(30);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
const BINDING_TIMEOUT: Duration = Duration::from_secs(3);
const STOP_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_BINDING_BYTES: u64 = 16 * 1024;

pub(super) struct LaunchedTab {
    pub handle: TabRuntimeHandle,
    pub title: String,
    pub url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TabBinding {
    target_id: String,
    pinned: bool,
}

pub(super) async fn launch_tab(
    runtime: &BrowserRuntimeContext,
    ticket: &TabTicket,
    url: &str,
) -> Result<LaunchedTab, BrowserError> {
    let session = tab_session(&ticket.tab_id);
    let result = launch_tab_inner(runtime, ticket, &session, url).await;
    if result.is_err() {
        cleanup_failed_launch(runtime, &session, true).await;
    }
    result
}

pub(super) async fn bind_existing_tab(
    runtime: &BrowserRuntimeContext,
    ticket: &TabTicket,
    target_id: &str,
) -> Result<LaunchedTab, BrowserError> {
    let session = tab_session(&ticket.tab_id);
    let result = bind_existing_inner(runtime, ticket, &session, target_id).await;
    if result.is_err() {
        cleanup_failed_launch(runtime, &session, true).await;
    }
    result
}

pub(super) async fn bind_existing_tab_preserving_target(
    runtime: &BrowserRuntimeContext,
    ticket: &TabTicket,
    target_id: &str,
) -> Result<LaunchedTab, BrowserError> {
    let session = tab_session(&ticket.tab_id);
    let result = bind_existing_inner(runtime, ticket, &session, target_id).await;
    if result.is_err() {
        cleanup_failed_launch(runtime, &session, false).await;
    }
    result
}

async fn bind_existing_inner(
    runtime: &BrowserRuntimeContext,
    ticket: &TabTicket,
    session: &str,
    target_id: &str,
) -> Result<LaunchedTab, BrowserError> {
    let response = runtime
        .cli
        .run(
            session,
            &["--cdp", &runtime.cdp_url, "--pin-tab", "tab", target_id],
            CREATE_TIMEOUT,
            CancellationToken::new(),
        )
        .await?;
    let binding = wait_for_binding(&runtime.cli, session).await?;
    if binding.target_id != target_id {
        return Err(runtime_unavailable());
    }
    let daemon = wait_for_pid_file(
        &runtime.cli.pid_path(session),
        runtime.cli.executable_path(),
        BINDING_TIMEOUT,
    )
    .await?;
    let (title, url) =
        page_metadata(&runtime.cli, session, &response, CancellationToken::new()).await?;
    Ok(LaunchedTab {
        handle: build_tab_handle(runtime, ticket, session, binding.target_id, daemon),
        title,
        url,
    })
}

async fn launch_tab_inner(
    runtime: &BrowserRuntimeContext,
    ticket: &TabTicket,
    session: &str,
    url: &str,
) -> Result<LaunchedTab, BrowserError> {
    let response = runtime
        .cli
        .run(
            session,
            &["--cdp", &runtime.cdp_url, "--pin-tab", "open", url],
            CREATE_TIMEOUT,
            CancellationToken::new(),
        )
        .await?;
    let binding = wait_for_binding(&runtime.cli, session).await?;
    let daemon = wait_for_pid_file(
        &runtime.cli.pid_path(session),
        runtime.cli.executable_path(),
        BINDING_TIMEOUT,
    )
    .await?;
    let (title, actual_url) =
        page_metadata(&runtime.cli, session, &response, CancellationToken::new()).await?;
    Ok(LaunchedTab {
        handle: build_tab_handle(runtime, ticket, session, binding.target_id, daemon),
        title,
        url: actual_url,
    })
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
        controller_session: runtime.controller_session.clone(),
        daemon,
        cancellation: CancellationToken::new(),
    }
}

pub(super) async fn cleanup_tab(
    handle: TabRuntimeHandle,
    close_target: bool,
) -> Result<(), BrowserError> {
    handle.cancellation.cancel();
    let target_result = if close_target {
        close_target_by_id(&handle.cli, &handle.controller_session, &handle.target_id).await
    } else {
        Ok(())
    };
    let session_result = close_session(&handle.cli, &handle.session, &handle.daemon).await;
    cleanup_session_files(&handle.cli, &handle.session).await;
    target_result.and(session_result)
}

pub(super) async fn cleanup_dead_tab_session(handle: TabRuntimeHandle) {
    handle.cancellation.cancel();
    cleanup_session_files(&handle.cli, &handle.session).await;
}

async fn cleanup_failed_launch(runtime: &BrowserRuntimeContext, session: &str, close_target: bool) {
    if close_target {
        close_failed_target(runtime, session).await;
    }
    if let Ok(daemon) = wait_for_pid_file(
        &runtime.cli.pid_path(session),
        runtime.cli.executable_path(),
        Duration::from_millis(300),
    )
    .await
    {
        let _ = close_session(&runtime.cli, session, &daemon).await;
    }
    cleanup_session_files(&runtime.cli, session).await;
}

async fn close_failed_target(runtime: &BrowserRuntimeContext, session: &str) {
    if let Ok(binding) = read_binding(&runtime.cli.target_path(session)).await {
        let _ = close_target_by_id(
            &runtime.cli,
            &runtime.controller_session,
            &binding.target_id,
        )
        .await;
    }
}

pub(super) async fn close_target_by_id(
    cli: &AgentBrowserCli,
    controller_session: &str,
    target_id: &str,
) -> Result<(), BrowserError> {
    match cli
        .run(
            controller_session,
            &["tab", "close", target_id],
            COMMAND_TIMEOUT,
            CancellationToken::new(),
        )
        .await
    {
        Ok(_) => Ok(()),
        Err(error) if error.code == BrowserErrorCode::BrowserTabGone => Ok(()),
        Err(error) => Err(error),
    }
}

async fn close_session(
    cli: &AgentBrowserCli,
    session: &str,
    daemon: &ProcessRecord,
) -> Result<(), BrowserError> {
    let result = cli
        .run(session, &["close"], STOP_TIMEOUT, CancellationToken::new())
        .await;
    if !wait_for_exit(daemon, STOP_TIMEOUT).await {
        kill_tree_checked(daemon).await?;
    }
    match result {
        Ok(_) => Ok(()),
        Err(_error) if !super::process::process_matches(daemon) => Ok(()),
        Err(error) => Err(error),
    }
}

async fn wait_for_binding(
    cli: &AgentBrowserCli,
    session: &str,
) -> Result<TabBinding, BrowserError> {
    let path = cli.target_path(session);
    let deadline = Instant::now() + BINDING_TIMEOUT;
    while Instant::now() < deadline {
        if let Ok(binding) = read_binding(&path).await {
            return Ok(binding);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(runtime_unavailable())
}

async fn read_binding(path: &Path) -> Result<TabBinding, BrowserError> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|_| runtime_unavailable())?;
    if metadata.len() > MAX_BINDING_BYTES {
        return Err(runtime_unavailable());
    }
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|_| runtime_unavailable())?;
    let binding: TabBinding = serde_json::from_slice(&bytes).map_err(|_| runtime_unavailable())?;
    if !binding.pinned || binding.target_id.is_empty() || binding.target_id.len() > 256 {
        return Err(runtime_unavailable());
    }
    Ok(binding)
}

fn tab_session(tab_id: &str) -> String {
    let suffix: String = tab_id
        .chars()
        .filter(char::is_ascii_hexdigit)
        .take(12)
        .collect();
    format!("iyw-tab-{suffix}")
}

async fn cleanup_session_files(cli: &AgentBrowserCli, session: &str) {
    let _ = tokio::fs::remove_file(cli.pid_path(session)).await;
    let _ = tokio::fs::remove_file(cli.target_path(session)).await;
}

fn runtime_unavailable() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserRuntimeUnavailable,
        "The pinned browser tab could not be initialized",
    )
    .retryable(true)
}
