use tokio::sync::watch;
use tokio::time::{Duration, Instant, MissedTickBehavior};

use codex_protocol::protocol::{EventMsg, TerminalInteractionEvent};

use super::head_tail_buffer::HeadTailBuffer;
use super::process::OutputHandles;
use super::{UnifiedExecProcessManager, WriteStdinInteractionEvent};

const POLL_ACTIVITY_INTERVAL: Duration = Duration::from_secs(30);

pub(super) struct PollOutput<'a, const MAX_BYTES: usize> {
    pub output: &'a OutputHandles<MAX_BYTES>,
    pub pause_state: Option<watch::Receiver<bool>>,
    pub deadline: Instant,
    pub interaction: Option<&'a WriteStdinInteractionEvent<'a>>,
    pub call_id: &'a str,
    pub process_id: i32,
}

pub(super) async fn collect_output<const MAX_BYTES: usize>(
    poll: PollOutput<'_, MAX_BYTES>,
) -> HeadTailBuffer<MAX_BYTES> {
    let collecting = UnifiedExecProcessManager::collect_output_until_deadline(
        poll.output,
        poll.pause_state,
        poll.deadline,
    );
    let Some(interaction) = poll.interaction else {
        return collecting.await;
    };
    tokio::pin!(collecting);
    let mut activity = tokio::time::interval(POLL_ACTIVITY_INTERVAL);
    activity.set_missed_tick_behavior(MissedTickBehavior::Delay);
    // 只在本次等待存活时报告活动，取消会同步丢弃计时器，不能续命下一轮。
    loop {
        tokio::select! {
            biased;
            output = &mut collecting => return output,
            _ = activity.tick() => {
                interaction.session.send_event(
                    interaction.turn.as_ref(),
                    EventMsg::TerminalInteraction(TerminalInteractionEvent {
                        call_id: poll.call_id.to_string(),
                        process_id: poll.process_id.to_string(),
                        stdin: String::new(),
                    }),
                ).await;
            }
        }
    }
}
