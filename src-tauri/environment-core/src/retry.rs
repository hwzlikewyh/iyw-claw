use std::time::Duration;

use anyhow::{Context, Result};

use crate::failure::Failure;

const ATTEMPTS: usize = 3;
const BACKOFF_SECONDS: [u64; 2] = [1, 3];

pub fn run<T>(component: &str, mut operation: impl FnMut() -> Result<T>) -> Result<T> {
    for attempt in 1..=ATTEMPTS {
        match operation() {
            Ok(value) => return Ok(value),
            Err(error) => {
                let retryable = error.downcast_ref::<Failure>().is_some_and(|e| e.retryable);
                if !retryable || attempt == ATTEMPTS {
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
