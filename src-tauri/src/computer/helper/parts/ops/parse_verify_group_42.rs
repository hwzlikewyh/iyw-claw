// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The verdicts, without the observed values the driver reports beside them.
pub(super) fn parse_verify(meta: &Value) -> RawVerify {
    RawVerify {
        status: verify_status(meta.get("status")),
        stable: meta.get("stable").and_then(Value::as_bool).unwrap_or(false),
        samples: meta.get("samples").and_then(Value::as_u64).unwrap_or(0),
        elapsed_ms: meta.get("elapsed_ms").and_then(Value::as_u64).unwrap_or(0),
        predicates: meta
            .get("predicates")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|p| PredicateResult {
                        index: p
                            .get("index")
                            .and_then(Value::as_u64)
                            .and_then(|v| u32::try_from(v).ok())
                            .unwrap_or(0),
                        status: verify_status(p.get("status")),
                        unknown_reason: string(p, "unknown_reason"),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}
