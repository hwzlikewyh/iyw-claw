use super::Runtime;
use crate::chat_channel::task_context::{self, TaskContext};

pub(super) async fn sync(runtime: &Runtime, task: &TaskContext, phase: &str) {
    let other_running = task_context::tasks().iter().any(|other| {
        other.id != task.id
            && other.connection_id == task.connection_id
            && other.origin.route.generation == task.origin.route.generation
            && matches!(other.phase.as_str(), "running" | "background" | "settling")
    });
    let bridge = runtime.channels.session_bridge();
    let identity = {
        let bridge = bridge.lock().await;
        bridge
            .get(&task.connection_id)
            .filter(|session| session.registration_generation == task.origin.route.generation)
            .map(|session| (session.route_key.clone(), session.target.clone()))
    };
    let Some((route, target)) = identity else {
        return;
    };
    let typing = runtime.channels.typing_controller();
    match phase {
        phase
            if phase != "waiting_input"
                && (other_running || matches!(phase, "running" | "background" | "settling")) =>
        {
            typing
                .ensure_running(&runtime.channels, target, (&route, &task.connection_id))
                .await;
        }
        "waiting_input" => {
            typing
                .pause(
                    &runtime.channels,
                    target.channel_id,
                    &route,
                    &task.connection_id,
                )
                .await;
        }
        _ => {
            typing
                .stop(
                    &runtime.channels,
                    target.channel_id,
                    &route,
                    &task.connection_id,
                )
                .await;
        }
    }
}
