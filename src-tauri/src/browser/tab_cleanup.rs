use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::command_runner::AgentBrowserCli;
use super::error::{BrowserError, BrowserErrorCode};
use super::process::{
    find_processes_by_exact_session, kill_tree_checked, process_matches, wait_for_exit,
    wait_for_pid_file, ProcessRecord,
};
use super::records::TabTicket;
use super::runtime::BrowserRuntimeContext;
use super::tab_binding::read_binding;
use super::tabs::TabRuntimeHandle;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
const IDENTITY_WAIT: Duration = Duration::from_millis(300);
const REQUIRED_EMPTY_IDENTITY_CHECKS: u8 = 2;
const STOP_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub(super) struct PendingTabCleanup {
    pub(super) id: u64,
    pub(super) tab_id: String,
    pub(super) session: String,
    pub(super) runtime_generation: u64,
    cli: AgentBrowserCli,
    cdp_url: String,
    controller_session: String,
    pub(super) target_id: Option<String>,
    daemons: Vec<ProcessRecord>,
    target_closed: bool,
    identity_observed: bool,
    empty_identity_checks: u8,
}

impl PendingTabCleanup {
    pub fn for_launch(
        id: u64,
        runtime: &BrowserRuntimeContext,
        ticket: &TabTicket,
        session: String,
        close_target: bool,
    ) -> Self {
        Self {
            id,
            tab_id: ticket.tab_id.clone(),
            session,
            runtime_generation: ticket.runtime_generation,
            cli: runtime.cli.clone(),
            cdp_url: runtime.cdp_url.clone(),
            controller_session: runtime.controller_session.clone(),
            target_id: None,
            daemons: Vec::new(),
            target_closed: !close_target,
            identity_observed: false,
            empty_identity_checks: 0,
        }
    }

    pub fn from_handle(id: u64, handle: TabRuntimeHandle, close_target: bool) -> Self {
        handle.cancellation.cancel();
        Self {
            id,
            tab_id: handle.tab_id,
            session: handle.session,
            runtime_generation: handle.runtime_generation,
            cli: handle.cli,
            cdp_url: handle.cdp_url,
            controller_session: handle.controller_session,
            target_id: Some(handle.target_id),
            daemons: vec![handle.daemon],
            target_closed: !close_target,
            identity_observed: true,
            empty_identity_checks: 0,
        }
    }

    fn from_handle_ref(id: u64, handle: &TabRuntimeHandle, close_target: bool) -> Self {
        handle.cancellation.cancel();
        Self {
            id,
            tab_id: handle.tab_id.clone(),
            session: handle.session.clone(),
            runtime_generation: handle.runtime_generation,
            cli: handle.cli.clone(),
            cdp_url: handle.cdp_url.clone(),
            controller_session: handle.controller_session.clone(),
            target_id: Some(handle.target_id.clone()),
            daemons: vec![handle.daemon.clone()],
            target_closed: !close_target,
            identity_observed: true,
            empty_identity_checks: 0,
        }
    }

    pub fn record_target(&mut self, target_id: String) {
        self.target_id = Some(target_id);
    }

    pub fn record_daemon(&mut self, daemon: ProcessRecord) {
        merge_process(&mut self.daemons, daemon);
        self.identity_observed = true;
    }
}

pub(super) async fn cleanup_pending_owner(
    owner: &mut PendingTabCleanup,
) -> Result<(), BrowserError> {
    let target_result = cleanup_owned_target(owner).await;
    let session_result = cleanup_owned_session(owner).await;
    if target_result.is_ok() && session_result.is_ok() {
        cleanup_session_files(&owner.cli, &owner.session).await;
    }
    target_result.and(session_result)
}

pub(super) async fn cleanup_tab_ref(
    handle: &TabRuntimeHandle,
    close_target: bool,
) -> Result<(), BrowserError> {
    let mut owner = PendingTabCleanup::from_handle_ref(0, handle, close_target);
    cleanup_pending_owner(&mut owner).await
}

async fn cleanup_owned_target(owner: &mut PendingTabCleanup) -> Result<(), BrowserError> {
    if owner.target_closed {
        return Ok(());
    }
    discover_target(owner).await?;
    let Some(target_id) = owner.target_id.as_deref() else {
        return Ok(());
    };
    close_target_by_id(&owner.cli, &owner.controller_session, target_id).await?;
    owner.target_closed = true;
    Ok(())
}

async fn discover_target(owner: &mut PendingTabCleanup) -> Result<(), BrowserError> {
    if owner.target_id.is_some() {
        return Ok(());
    }
    let path = owner.cli.target_path(&owner.session);
    match read_binding(&path).await {
        Ok(binding) => {
            owner.target_id = Some(binding.target_id);
            Ok(())
        }
        Err(error) => match tokio::fs::metadata(path).await {
            Err(io_error) if io_error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            _ => Err(error),
        },
    }
}

async fn cleanup_owned_session(owner: &mut PendingTabCleanup) -> Result<(), BrowserError> {
    refresh_process_identities(owner).await?;
    let live_daemon = owner.daemons.iter().find(|daemon| process_matches(daemon));
    let close_result = match live_daemon {
        Some(daemon) => close_session(&owner.cli, &owner.session, &owner.cdp_url, daemon).await,
        None => Ok(()),
    };
    let force_result = stop_known_processes(&owner.daemons).await;
    let remaining = exact_session_processes(owner);
    if !remaining.is_empty() {
        owner.daemons = remaining;
        return close_result
            .and(force_result)
            .and(Err(session_process_remains()));
    }
    close_result.and(force_result)
}

async fn refresh_process_identities(owner: &mut PendingTabCleanup) -> Result<(), BrowserError> {
    let mut identity_error = None;
    if !owner.identity_observed {
        match wait_for_pid_file(
            &owner.cli.pid_path(&owner.session),
            owner.cli.executable_path(),
            IDENTITY_WAIT,
        )
        .await
        {
            Ok(daemon) => owner.record_daemon(daemon),
            Err(error) => identity_error = Some(error),
        }
    }
    for daemon in exact_session_processes(owner) {
        owner.record_daemon(daemon);
    }
    if owner.identity_observed {
        return Ok(());
    }
    owner.empty_identity_checks = owner.empty_identity_checks.saturating_add(1);
    if owner.empty_identity_checks >= REQUIRED_EMPTY_IDENTITY_CHECKS {
        return Ok(());
    }
    Err(identity_error.unwrap_or_else(process_identity_pending))
}

async fn stop_known_processes(daemons: &[ProcessRecord]) -> Result<(), BrowserError> {
    let mut result = Ok(());
    for daemon in daemons {
        let process_result = kill_tree_checked(daemon).await;
        result = result.and(process_result);
    }
    result
}

fn exact_session_processes(owner: &PendingTabCleanup) -> Vec<ProcessRecord> {
    find_processes_by_exact_session(
        owner.cli.executable_path(),
        &owner.session,
        "agent-browser-daemon",
    )
}

fn merge_process(processes: &mut Vec<ProcessRecord>, daemon: ProcessRecord) {
    let exists = processes
        .iter()
        .any(|current| current.pid == daemon.pid && current.started_at == daemon.started_at);
    if !exists {
        processes.push(daemon);
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
        Err(error)
            if matches!(
                error.code,
                BrowserErrorCode::BrowserTabGone | BrowserErrorCode::BrowserRuntimeUnavailable
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
}

async fn close_session(
    cli: &AgentBrowserCli,
    session: &str,
    cdp_url: &str,
    daemon: &ProcessRecord,
) -> Result<(), BrowserError> {
    let result = cli
        .run_pinned(
            session,
            cdp_url,
            &["close"],
            STOP_TIMEOUT,
            CancellationToken::new(),
        )
        .await;
    if !wait_for_exit(daemon, STOP_TIMEOUT).await {
        kill_tree_checked(daemon).await?;
    }
    match result {
        Ok(_) => Ok(()),
        Err(_) if !process_matches(daemon) => Ok(()),
        Err(error) => Err(error),
    }
}

async fn cleanup_session_files(cli: &AgentBrowserCli, session: &str) {
    let _ = tokio::fs::remove_file(cli.pid_path(session)).await;
    let _ = tokio::fs::remove_file(cli.target_path(session)).await;
}

fn process_identity_pending() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserRuntimeStartTimeout,
        "The browser tab process identity is still pending",
    )
    .retryable(true)
}

fn session_process_remains() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserInternal,
        "An exact browser tab session process remained alive after cleanup",
    )
    .retryable(true)
}
