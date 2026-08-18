use std::panic::AssertUnwindSafe;

use axum::Router;
use futures_util::FutureExt;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::BuiltinMcpClient;

pub(super) fn spawn_tasks(
    tcp: tokio::net::TcpListener,
    router: Router,
    client: BuiltinMcpClient,
    shutdown: CancellationToken,
) -> Vec<JoinHandle<()>> {
    let server = tokio::spawn(run_server(tcp, router, client, shutdown));
    vec![server]
}

async fn run_server(
    tcp: tokio::net::TcpListener,
    router: Router,
    client: BuiltinMcpClient,
    shutdown: CancellationToken,
) {
    let serve =
        axum::serve(tcp, router).with_graceful_shutdown(shutdown.child_token().cancelled_owned());
    let result = AssertUnwindSafe(async move { serve.await })
        .catch_unwind()
        .await;
    let expected_shutdown = shutdown.is_cancelled();
    client
        .ready
        .store(false, std::sync::atomic::Ordering::Release);
    shutdown.cancel();
    if expected_shutdown && matches!(&result, Ok(Ok(()))) {
        return;
    }
    let report = client.revoke_all().await;
    match result {
        Ok(Err(error)) => tracing::error!(
            target: "builtin_mcp", error = %error,
            revoked_sessions = report.revoked_sessions,
            authorities_closed = report.authorities_closed,
            pending_cleanup_empty = report.pending_cleanup_empty,
            protocol_sessions_empty = report.protocol_sessions_empty,
            parent_tasks_reaped = report.parent_tasks_reaped,
            cleanup_tasks_reaped = report.cleanup_tasks_reaped,
            "HTTP MCP service exited"
        ),
        Ok(Ok(())) if !expected_shutdown => tracing::error!(
            target: "builtin_mcp",
            revoked_sessions = report.revoked_sessions,
            authorities_closed = report.authorities_closed,
            pending_cleanup_empty = report.pending_cleanup_empty,
            protocol_sessions_empty = report.protocol_sessions_empty,
            parent_tasks_reaped = report.parent_tasks_reaped,
            cleanup_tasks_reaped = report.cleanup_tasks_reaped,
            "HTTP MCP service exited unexpectedly"
        ),
        Err(_) => tracing::error!(
            target: "builtin_mcp",
            revoked_sessions = report.revoked_sessions,
            authorities_closed = report.authorities_closed,
            pending_cleanup_empty = report.pending_cleanup_empty,
            protocol_sessions_empty = report.protocol_sessions_empty,
            parent_tasks_reaped = report.parent_tasks_reaped,
            cleanup_tasks_reaped = report.cleanup_tasks_reaped,
            "HTTP MCP service panicked; session authority revoked"
        ),
        Ok(Ok(())) => {}
    }
}

pub(super) async fn wait_for_task(
    join: &mut JoinHandle<()>,
    deadline: tokio::time::Instant,
) -> bool {
    match tokio::time::timeout_at(deadline, &mut *join).await {
        Ok(Ok(())) => true,
        Ok(Err(error)) => {
            tracing::error!(target: "builtin_mcp", error = %error,
                "HTTP MCP service task failed");
            false
        }
        Err(_) => abort_and_reap(join).await,
    }
}

async fn abort_and_reap(join: &mut JoinHandle<()>) -> bool {
    join.abort();
    match (&mut *join).await {
        Ok(()) => true,
        Err(error) if error.is_cancelled() => true,
        Err(error) => {
            tracing::error!(target: "builtin_mcp", error = %error,
                "HTTP MCP service task failed after abort");
            false
        }
    }
}
