//! Walk the conversation parent chain to compute delegation depth.
//!
//! The walker is generic over an async closure so the broker can plug in a
//! real DB lookup in production and a stub `Vec<(id, parent_id)>` in tests
//! without any extra trait plumbing.
//!
//! `cap` saturates the walk so a corrupted chain (cycle, deep history) can't
//! cause unbounded DB load. Callers pass `depth_limit + 1` — that's all the
//! broker ever needs to decide rejection.

use std::future::Future;

use crate::acp::delegation::types::DelegationError;

pub async fn compute_depth<F, Fut>(
    start: i32,
    mut parent_resolver: F,
    cap: u32,
) -> Result<u32, DelegationError>
where
    F: FnMut(i32) -> Fut,
    Fut: Future<Output = Result<Option<i32>, DelegationError>>,
{
    let mut current = start;
    let mut depth = 0u32;
    while depth < cap {
        match parent_resolver(current).await? {
            None => return Ok(depth),
            Some(parent) => {
                current = parent;
                depth += 1;
            }
        }
    }
    Ok(depth)
}

