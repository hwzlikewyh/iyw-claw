use std::time::{Duration, Instant};
use std::{io, path::Path};

use anyhow::{Context, Result};

use crate::failure::Failure;

const ATTEMPTS: usize = 3;
const BACKOFF_SECONDS: [u64; 2] = [1, 3];
const FILE_RETRY_TIMEOUT: Duration = Duration::from_secs(15);
const FILE_RETRY_INITIAL_DELAY: Duration = Duration::from_millis(100);
const FILE_RETRY_MAX_DELAY: Duration = Duration::from_secs(2);
const RETRYABLE_WINDOWS_FILE_ERRORS: [i32; 3] = [5, 32, 33];

pub fn file<T>(
    action: &str,
    path: &Path,
    mut operation: impl FnMut() -> io::Result<T>,
) -> Result<T> {
    let started = Instant::now();
    let mut delay = FILE_RETRY_INITIAL_DELAY;
    let mut attempt = 0;
    loop {
        attempt += 1;
        match operation() {
            Ok(value) => {
                if attempt > 1 {
                    eprintln!(
                        "file operation recovered: action={action}; path={}; attempt={attempt}; elapsed_ms={}",
                        path.display(), started.elapsed().as_millis()
                    );
                }
                return Ok(value);
            }
            Err(error) => {
                let remaining = FILE_RETRY_TIMEOUT.saturating_sub(started.elapsed());
                if !is_transient_file_error(&error) || remaining.is_zero() {
                    return Err(error).with_context(|| {
                        format!(
                            "{action}: {} (attempt={attempt}, elapsed_ms={})",
                            path.display(),
                            started.elapsed().as_millis()
                        )
                    });
                }
                eprintln!(
                    "file operation retry: action={action}; path={}; attempt={attempt}; delay_ms={}; error={error}",
                    path.display(), delay.min(remaining).as_millis()
                );
                std::thread::sleep(delay.min(remaining));
                delay = (delay * 2).min(FILE_RETRY_MAX_DELAY);
            }
        }
    }
}

pub fn run<T>(component: &str, mut operation: impl FnMut() -> Result<T>) -> Result<T> {
    run_with(component, &mut operation, |error| {
        error.downcast_ref::<Failure>().is_some_and(|e| e.retryable)
    })
}

pub fn component<T>(component: &str, mut operation: impl FnMut() -> Result<T>) -> Result<T> {
    // 网络下载已有独立重试，组件级只恢复文件错误，避免嵌套放大网络请求。
    run_with(component, &mut operation, |error| {
        error.chain().any(|cause| {
            cause
                .downcast_ref::<io::Error>()
                .is_some_and(is_transient_file_error)
        })
    })
}

fn is_transient_file_error(error: &io::Error) -> bool {
    cfg!(windows)
        && error
            .raw_os_error()
            .is_some_and(|code| RETRYABLE_WINDOWS_FILE_ERRORS.contains(&code))
}

fn run_with<T>(
    component: &str,
    mut operation: impl FnMut() -> Result<T>,
    retryable: impl Fn(&anyhow::Error) -> bool,
) -> Result<T> {
    for attempt in 1..=ATTEMPTS {
        match operation() {
            Ok(value) => return Ok(value),
            Err(error) => {
                if !retryable(&error) || attempt == ATTEMPTS {
                    return Err(error)
                        .with_context(|| format!("组件 {component}，第 {attempt} 次尝试失败"));
                }
                println!(
                    "{}",
                    serde_json::json!({
                        "componentId": component, "phase": "retrying",
                        "attempt": attempt, "maxAttempts": ATTEMPTS,
                        "message": format!("{component}：第 {attempt}/{ATTEMPTS} 次尝试失败，正在重试：{error:#}"),
                    })
                );
                std::thread::sleep(Duration::from_secs(BACKOFF_SECONDS[attempt - 1]));
            }
        }
    }
    unreachable!("retry loop always returns")
}
