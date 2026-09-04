//! C ABI boundary for the private Codex worker library.

mod config;
mod worker;

use std::panic::{catch_unwind, AssertUnwindSafe};

/// Starts the internal ACP worker over inherited stdin/stdout.
#[no_mangle]
pub extern "C" fn iyw_codex_worker_run_v1() -> i32 {
    run_entry(worker::run)
}

/// Handles an upstream helper reexec from the internal worker process.
#[no_mangle]
pub extern "C" fn iyw_codex_worker_dispatch_helper_v1() -> i32 {
    if iyw_codex_harness::dispatch_upstream_helper() {
        0
    } else {
        64
    }
}

fn run_entry(run: fn() -> Result<(), worker::WorkerError>) -> i32 {
    match catch_unwind(AssertUnwindSafe(run)) {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => {
            eprintln!("[internal-codex-worker] {error}");
            1
        }
        Err(_) => {
            eprintln!("[internal-codex-worker] worker panicked");
            2
        }
    }
}
