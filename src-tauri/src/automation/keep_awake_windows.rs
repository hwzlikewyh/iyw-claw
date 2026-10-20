use std::io;
use std::sync::mpsc::{self, Sender};
use std::thread::JoinHandle;

pub(super) struct Inhibitor {
    stop: Option<Sender<()>>,
    worker: Option<JoinHandle<()>>,
}

impl Inhibitor {
    pub(super) fn start() -> io::Result<Self> {
        let (stop, receiver) = mpsc::channel();
        let (ready, result) = mpsc::sync_channel(1);
        // 执行状态属于线程，不能跨 Tokio 工作线程设置和清除。
        let worker = std::thread::Builder::new()
            .name("automation-sleep-inhibitor".to_string())
            .spawn(move || {
                let initial = set_inhibited(true);
                let enabled = initial.is_ok();
                let _ = ready.send(initial);
                if !enabled {
                    return;
                }
                let _ = receiver.recv();
                if let Err(error) = set_inhibited(false) {
                    tracing::warn!(error = %error, "[automation] Windows sleep inhibitor release failed");
                }
            })?;
        let guard = Self {
            stop: Some(stop),
            worker: Some(worker),
        };
        result
            .recv()
            .map_err(|error| io::Error::new(io::ErrorKind::BrokenPipe, error))??;
        Ok(guard)
    }

    pub(super) fn refresh(&mut self) -> io::Result<()> {
        if self
            .worker
            .as_ref()
            .is_none_or(|worker| worker.is_finished())
        {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "Windows sleep inhibitor thread stopped",
            ));
        }
        Ok(())
    }
}

impl Drop for Inhibitor {
    fn drop(&mut self) {
        self.stop.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn set_inhibited(enabled: bool) -> io::Result<()> {
    const ES_CONTINUOUS: u32 = 0x80000000;
    const ES_SYSTEM_REQUIRED: u32 = 0x00000001;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetThreadExecutionState(flags: u32) -> u32;
    }
    let flags = ES_CONTINUOUS | if enabled { ES_SYSTEM_REQUIRED } else { 0 };
    // 参数只包含文档定义的标志；调用线程在 guard 释放前始终存活。
    if unsafe { SetThreadExecutionState(flags) } == 0 {
        return Err(io::Error::other("SetThreadExecutionState returned NULL"));
    }
    Ok(())
}
