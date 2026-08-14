use std::cmp::min;

use super::*;

const MAX_RETRY_DELAY: Duration = Duration::from_secs(60);

pub(super) async fn poll_loop(
    channel_id: i32,
    generation: u64,
    interval: Duration,
    state: Arc<State>,
    command_tx: mpsc::Sender<IncomingCommand>,
    runtime_tx: mpsc::Sender<ChannelRuntimeEvent>,
    mut stop_rx: watch::Receiver<bool>,
) {
    let mut window_start = Local::now();
    // manager 会在 `start` 返回后才登记 backend；预检已覆盖启动阶段，
    // 因此运行态轮询延后一个周期，避免状态事件早于登记。
    let mut delay = interval;
    let mut consecutive_failures: u32 = 0;
    loop {
        if wait_or_stop(delay, &mut stop_rx).await {
            tracing::info!(channel_id, "[WeCom] poll loop stopped");
            return;
        }
        let now = Local::now();
        let begin = window_start - ChronoDuration::seconds(POLL_OVERLAP_SECS);
        match poll_once(channel_id, &state, &command_tx, begin, now).await {
            Ok(()) => {
                window_start = now;
                consecutive_failures = 0;
                delay = interval;
                report_recovery(channel_id, generation, &state, &runtime_tx).await;
            }
            Err(error) => {
                consecutive_failures = consecutive_failures.saturating_add(1);
                delay = retry_delay(interval, consecutive_failures);
                if report_failure(channel_id, generation, &state, &runtime_tx, &error).await {
                    return;
                }
            }
        }
    }
}

async fn wait_or_stop(delay: Duration, stop_rx: &mut watch::Receiver<bool>) -> bool {
    if delay.is_zero() {
        return false;
    }
    tokio::select! {
        _ = tokio::time::sleep(delay) => false,
        changed = stop_rx.changed() => changed.is_err() || *stop_rx.borrow(),
    }
}

async fn report_recovery(
    channel_id: i32,
    generation: u64,
    state: &State,
    runtime_tx: &mpsc::Sender<ChannelRuntimeEvent>,
) {
    if transition_status(state, ChannelConnectionStatus::Connected).await {
        send_event(
            runtime_tx,
            ChannelRuntimeEvent::Connected {
                channel_id,
                generation,
            },
        )
        .await;
        tracing::info!(channel_id, "[WeCom] polling recovered");
    }
}

async fn report_failure(
    channel_id: i32,
    generation: u64,
    state: &State,
    runtime_tx: &mpsc::Sender<ChannelRuntimeEvent>,
    error: &ChatChannelError,
) -> bool {
    let terminal = matches!(
        error,
        ChatChannelError::AuthenticationFailed(_) | ChatChannelError::ConfigurationInvalid(_)
    );
    let changed = transition_status(state, ChannelConnectionStatus::Error).await;
    tracing::warn!(channel_id, terminal, error = %error, "[WeCom] poll failed");
    if terminal || changed {
        send_event(
            runtime_tx,
            ChannelRuntimeEvent::Error {
                channel_id,
                generation,
                error: error.to_string(),
            },
        )
        .await;
    }
    terminal
}

async fn transition_status(state: &State, next: ChannelConnectionStatus) -> bool {
    let mut status = state.status.lock().await;
    if *status == next {
        return false;
    }
    *status = next;
    true
}

async fn send_event(runtime_tx: &mpsc::Sender<ChannelRuntimeEvent>, event: ChannelRuntimeEvent) {
    if let Err(error) = runtime_tx.send(event).await {
        tracing::warn!(error = %error, "[WeCom] runtime event delivery failed");
    }
}

fn retry_delay(interval: Duration, consecutive_failures: u32) -> Duration {
    let exponent = consecutive_failures.saturating_sub(1).min(6);
    let scaled = interval.saturating_mul(1_u32 << exponent);
    min(scaled, MAX_RETRY_DELAY)
}
