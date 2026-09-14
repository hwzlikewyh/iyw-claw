use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

use super::super::RuntimeHandle;
use crate::browser::process::{find_processes_by_executable_arg, wait_for_exit};

const CLOSE_TIMEOUT: Duration = Duration::from_secs(3);

pub(super) async fn close(handle: &RuntimeHandle) {
    // 官方 Browser.close 正常关闭浏览器，先给 profile 刷盘机会，再核验进程兜底。
    let cli = handle.cli.clone();
    let processes = tokio::task::spawn_blocking(move || {
        find_processes_by_executable_arg(
            &cli.engine_path,
            &cli.profile_path.to_string_lossy(),
            "browser-engine",
        )
    })
    .await;
    let processes = match processes {
        Ok(processes) => processes,
        Err(error) => {
            tracing::warn!(target: "iyw_claw_browser", runtime_generation = handle.generation,
                error = %error, "browser graceful process lookup failed; using cleanup fallback");
            return;
        }
    };
    let deadline = Instant::now() + CLOSE_TIMEOUT;
    let outcome = tokio::time::timeout(CLOSE_TIMEOUT, async {
        request_close(&handle.cdp_url).await?;
        for process in &processes {
            if !wait_for_exit(process, deadline.saturating_duration_since(Instant::now())).await {
                return Err("process_exit_timeout");
            }
        }
        Ok(())
    })
    .await;
    let result = match outcome {
        Ok(Ok(())) => "exited",
        Ok(Err(reason)) => reason,
        Err(_) => "close_timeout",
    };
    tracing::info!(target: "iyw_claw_browser", runtime_generation = handle.generation,
        result, "browser graceful shutdown finished; checking owned processes");
}

async fn request_close(url: &str) -> Result<(), &'static str> {
    let (mut socket, _) = tokio_tungstenite::connect_async(url)
        .await
        .map_err(|_| "connect_failed")?;
    socket
        .send(Message::Text(
            serde_json::json!({ "id": 1, "method": "Browser.close" })
                .to_string()
                .into(),
        ))
        .await
        .map_err(|_| "send_failed")?;
    while let Some(message) = socket.next().await {
        match message {
            Ok(Message::Text(text)) => {
                let reply: serde_json::Value =
                    serde_json::from_str(&text).map_err(|_| "invalid_response")?;
                if reply.get("id").and_then(serde_json::Value::as_u64) == Some(1) {
                    return if reply.get("error").is_some() {
                        Err("close_rejected")
                    } else {
                        Ok(())
                    };
                }
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }
    // 浏览器可能先断开调试连接，调用方仍须按已核验的进程身份等待退出。
    Ok(())
}
