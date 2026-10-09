pub fn computer_verify_request(
    arguments: &Value,
) -> Result<(String, crate::computer::types::VerifyRequest), String> {
    use crate::computer::types::{VerifyPredicate, VerifyRequest, MAX_VERIFY_PREDICATES};
    let tool = "computer_verify";
    let target_id = computer_target_id(arguments, tool)?;
    let expect: Vec<VerifyPredicate> = match arguments.get("expect") {
        Some(Value::Array(items)) => items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                serde_json::from_value::<VerifyPredicate>(item.clone())
                    .map_err(|e| format!("{tool}: `expect[{i}]` is not a predicate this tool knows: {e}"))
            })
            .collect::<Result<_, _>>()?,
        _ => {
            return Err(format!(
                "{tool} requires `expect`: an array of 1 to {MAX_VERIFY_PREDICATES} predicates"
            ))
        }
    };
    if expect.is_empty() || expect.len() > MAX_VERIFY_PREDICATES {
        return Err(format!(
            "{tool}: `expect` must hold 1 to {MAX_VERIFY_PREDICATES} predicates, not {}",
            expect.len()
        ));
    }
    Ok((
        target_id,
        VerifyRequest {
            expect,
            timeout_ms: computer_optional_u32(arguments, tool, "timeoutMs")?,
            stable_samples: computer_optional_u32(arguments, tool, "stableSamples")?,
        },
    ))
}
