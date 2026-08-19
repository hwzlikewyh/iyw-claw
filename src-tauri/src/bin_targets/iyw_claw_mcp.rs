//! `iyw-claw-mcp` — the per-launch stdio MCP companion that an agent CLI runs
//! to surface iyw-claw's tools to its LLM: the multi-agent delegation tools
//! (`delegate_to_agent` etc.), `check_user_feedback` (pull the user's mid-turn
//! steering notes), `ask_user_question` (block on a multiple-choice card), and
//! `get_session_info` (resolve a referenced session by id), gated by the
//! `--features` groups (`delegation` / `feedback` / `ask` / `sessions` /
//! `images` / `memory` / `memory-proposal` / `memory-recall` / `artifacts` /
//! `channels` / `browser`).
//!
//! The agent's MCP config (injected by iyw-claw via `load_mcp_servers_for_agent`)
//! spawns this binary with three required flags:
//!
//!   iyw-claw-mcp \
//!     --parent-connection-id <uuid> \
//!     --socket-path <abs path> \
//!     --token <ephemeral secret>
//!
//! All three are required in stdio MCP mode. The separate
//! `tool scheduled-task` mode uses only the host socket and carries no token.
//! Everything heavyweight — JSON-RPC dispatch, UDS round-trip, MCP tool
//! schema, cancellation tracking — lives in
//! `iyw_claw_lib::acp::delegation::{companion, transport}` so it's
//! unit-testable without spawning a process.
//!
//! Stdin lines are dispatched concurrently: synchronous methods
//! (`initialize`, `tools/list`) emit a response inline, `tools/call`
//! spawns a tokio task that drives the UDS round-trip racing a cancel
//! channel, and `notifications/cancelled` wakes the relevant task without
//! blocking the reader. Stdout writes are serialized through a mutex so
//! interleaved frames never corrupt the wire.

use std::io::Write;
use std::process::ExitCode;
use std::sync::Arc;

use iyw_claw_lib::acp::delegation::companion::{
    companion_ready_report_after_tools_list, dispatch_line, drain_and_cancel_all,
    send_companion_ready_report, CompanionContext, CompanionFeatures, InflightCalls,
    JsonRpcResponse, LineAction, SpawnResult,
};
use iyw_claw_lib::acp::delegation::parent_watcher::{wait_for_parent_exit, DEFAULT_POLL_INTERVAL};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Stdout};
use tokio::sync::Mutex;

mod iyw_claw_mcp_args;
mod iyw_claw_mcp_cli;

use iyw_claw_mcp_args::parse_args;
use iyw_claw_mcp_cli::run_tool_cli;

/// Serialize a `JsonRpcResponse` and append a newline; small enough to keep
/// inline so the write-mutex critical section stays tight.
async fn write_response(
    stdout: &Arc<Mutex<Stdout>>,
    resp: &JsonRpcResponse,
) -> std::io::Result<()> {
    let serialized = serde_json::to_string(resp).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, format!("encode: {e}"))
    })?;
    let mut guard = stdout.lock().await;
    guard.write_all(serialized.as_bytes()).await?;
    guard.write_all(b"\n").await?;
    guard.flush().await?;
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    // Stderr-only subscriber: stdout is the JSON-RPC protocol channel, and
    // concurrent mcp processes share no log file. No hub/buffer/emitter.
    let _log_guard = iyw_claw_lib::logging::init::init_mcp();

    let raw_args = std::env::args().skip(1).collect::<Vec<_>>();
    if raw_args.first().map(String::as_str) == Some("tool") {
        return run_tool_cli(&raw_args[1..]).await;
    }

    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "iyw-claw-mcp: {e}");
            return ExitCode::from(2);
        }
    };
    let ctx = CompanionContext {
        parent_connection_id: args.parent_connection_id,
        socket_path: args.socket_path,
        token: args.token,
        working_dir: args.working_dir,
        agent_type: args.agent_type,
        features: CompanionFeatures::parse(args.features.as_deref()),
    };

    let stdin = tokio::io::stdin();
    let stdout = Arc::new(Mutex::new(tokio::io::stdout()));
    let inflight = Arc::new(InflightCalls::new());
    let mut lines = BufReader::new(stdin).lines();

    // Optional parent-PID watchdog. Composed as a separate future so the
    // main loop can race it against stdin reads via `tokio::select!`;
    // when no PID was provided we substitute a never-ready future, which
    // tokio's branch evaluation skips for free.
    let watchdog: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> =
        match args.parent_pid {
            Some(pid) => Box::pin(wait_for_parent_exit(pid, DEFAULT_POLL_INTERVAL)),
            None => Box::pin(std::future::pending()),
        };
    tokio::pin!(watchdog);

    loop {
        tokio::select! {
            // Bias toward parent-exit detection: if the watchdog fires
            // mid-stdin-read we want to bail rather than finish a round-trip
            // whose response no one will read.
            biased;
            _ = &mut watchdog => {
                // Best-effort: cancel every in-flight delegation BEFORE we
                // hard-exit so the broker doesn't park each pending row
                // on `rx.await` waiting for a TurnComplete it can never
                // deliver. cancel_by_parent on the iyw-claw main side is the
                // ultimate backstop, but firing the explicit cancels here
                // closes the window between MCP shutdown and parent ACP
                // disconnect detection on the iyw-claw side.
                drain_and_cancel_all(&ctx, &inflight, "parent process exited").await;
                let _ = writeln!(
                    std::io::stderr(),
                    "iyw-claw-mcp: parent process exited, shutting down"
                );
                // Hard exit on purpose: `tokio::io::stdin()` parks a
                // blocking worker thread that the runtime can't cancel,
                // so returning normally would keep the process alive
                // until the parent agent CLI also closes stdin — defeating
                // the watchdog. The agent CLI sees the stdout pipe close
                // and tears down its MCP client cleanly.
                std::process::exit(0);
            }
            line_result = lines.next_line() => {
                let line = match line_result {
                    Ok(Some(l)) => l,
                    Ok(None) => {
                        // Parent closed stdin. Same shutdown rationale as
                        // the watchdog branch: drain pending delegations
                        // before returning so the broker can resolve them
                        // immediately.
                        drain_and_cancel_all(&ctx, &inflight, "companion stdio closed").await;
                        break;
                    }
                    Err(e) => {
                        let _ = writeln!(std::io::stderr(), "iyw-claw-mcp: read stdin: {e}");
                        drain_and_cancel_all(&ctx, &inflight, "companion stdin error").await;
                        return ExitCode::from(1);
                    }
                };
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }
                let action = dispatch_line(&ctx, inflight.clone(), &line).await;
                match action {
                    LineAction::Respond(resp) => {
                        if let Err(e) = write_response(&stdout, &resp).await {
                            let _ = writeln!(std::io::stderr(), "iyw-claw-mcp: write stdout: {e}");
                            return ExitCode::from(1);
                        }
                        if let Some(report) =
                            companion_ready_report_after_tools_list(&ctx, &line, &resp)
                        {
                            tokio::spawn(send_companion_ready_report(
                                ctx.socket_path.clone(),
                                report,
                            ));
                        }
                    }
                    LineAction::Spawn(spawned) => {
                        // Drive the round-trip in a detached task so the
                        // stdin reader stays responsive — the next line may
                        // be a `notifications/cancelled` for THIS request.
                        let stdout = stdout.clone();
                        tokio::spawn(async move {
                            let SpawnResult {
                                response,
                                after_relay,
                            } = spawned.future.await;
                            // `None` → cancellation won; suppress per MCP spec.
                            let Some(resp) = response else {
                                return;
                            };
                            if let Err(e) = write_response(&stdout, &resp).await {
                                let _ = writeln!(
                                    std::io::stderr(),
                                    "iyw-claw-mcp: write stdout: {e}"
                                );
                                // Relay failed (agent stdin gone) → skip any
                                // post-relay action so feedback notes stay
                                // pending for the next check (at-least-once).
                                return;
                            }
                            // The response reached the agent's stdin. Only now
                            // run any post-relay action — for
                            // `check_user_feedback`, the delivery commit that
                            // marks the pulled notes `Delivered`.
                            if let Some(after) = after_relay {
                                if !after.run().await {
                                    let _ = writeln!(
                                        std::io::stderr(),
                                        "iyw-claw-mcp: feedback delivery commit failed"
                                    );
                                }
                            }
                        });
                    }
                    LineAction::Silent => {}
                }
            }
        }
    }
    ExitCode::SUCCESS
}
