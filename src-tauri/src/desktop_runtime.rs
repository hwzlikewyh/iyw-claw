use std::sync::OnceLock;

// 内置星河的配置加载和会话 Future 会叠加栈帧；Windows 默认栈不足以承载。
const WORKER_STACK_BYTES: usize = 8 * 1024 * 1024;
static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

pub(crate) fn install() {
    let runtime = RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .thread_stack_size(WORKER_STACK_BYTES)
            .enable_all()
            .build()
            .expect("Failed to initialize desktop async runtime")
    });
    tauri::async_runtime::set(runtime.handle().clone());
    tracing::info!(
        target: "iyw_claw_startup",
        worker_stack_bytes = WORKER_STACK_BYTES,
        "initialized Windows desktop async runtime"
    );
}
