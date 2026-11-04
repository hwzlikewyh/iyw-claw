use std::sync::atomic::{AtomicBool, Ordering};

use crate::web::event_bridge::{emit_event, EventEmitter};

static PREPARING: AtomicBool = AtomicBool::new(false);

struct PreparingGuard;

impl Drop for PreparingGuard {
    fn drop(&mut self) {
        PREPARING.store(false, Ordering::Release);
    }
}

pub fn prepare(task_id: &str, emitter: &EventEmitter) {
    if !super::python_environment::supported() || PREPARING.swap(true, Ordering::AcqRel) {
        return;
    }
    let task_id = task_id.to_owned();
    let emitter = emitter.clone();
    tokio::spawn(async move {
        let _guard = PreparingGuard;
        if let Err(error) = try_prepare(&task_id, &emitter).await {
            tracing::warn!(
                task_id,
                error,
                "[shared-python] unavailable; agent can repair when needed"
            );
            emit_event(
                &emitter,
                "app://bootstrap-init",
                serde_json::json!({
                    "taskId": task_id, "phase": "health_check", "component": "uv", "message": format!("共享 Python 环境尚未就绪：{error}"),
                }),
            );
        }
    });
}

async fn try_prepare(task_id: &str, emitter: &EventEmitter) -> Result<(), String> {
    let uv = crate::acp::version_center::managed_tool_executable("uv")
        .ok_or("共享 Python 初始化需要先安装受管 uv")?;
    let task_id = task_id.to_owned();
    let emitter = emitter.clone();
    tokio::task::spawn_blocking(move || {
        let progress = |message: &str| {
            tracing::info!(task_id, message, "[shared-python] initialization progress");
            emit_event(&emitter, "app://bootstrap-init", serde_json::json!({
                "taskId": task_id, "phase": "health_check", "component": "uv", "message": message,
            }));
        };
        super::python_environment::ensure(&super::root(), &uv, &progress)
    })
    .await
    .map_err(|error| format!("共享 Python 初始化任务异常：{error}"))?
}
