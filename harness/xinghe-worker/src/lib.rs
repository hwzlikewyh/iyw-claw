// The worker statically links the Codex 0.154 harness, whose app-server call
// path pushes rustc's query stack past the default depth limit. Raise it here
// instead of relying on a compiler flag, which the release script does not pass.
#![recursion_limit = "512"]

//! C ABI boundary for the private 星河 worker library.

mod config;
mod identity;
mod worker;
#[path = "../../codex/src/diagnostics.rs"]
mod diagnostics;

use std::panic::{catch_unwind, AssertUnwindSafe};

/// Starts the internal ACP worker over inherited stdin/stdout.
#[no_mangle]
pub extern "C" fn iyw_xinghe_worker_run_v1() -> i32 {
    run_entry(worker::run)
}

/// Handles an upstream helper reexec from the internal worker process.
#[no_mangle]
pub extern "C" fn iyw_xinghe_worker_dispatch_helper_v1() -> i32 {
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
            eprintln!("[internal-xinghe-worker] {error}");
            1
        }
        Err(_) => {
            eprintln!("[internal-xinghe-worker] worker panicked");
            2
        }
    }
}
