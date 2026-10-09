pub fn computer_snapshot_request(
    arguments: &Value,
) -> Result<(String, crate::acp::computer_tools::SnapshotRequest), String> {
    let tool = "computer_snapshot";
    let target_id = computer_target_id(arguments, tool)?;
    let query = match arguments.get("query") {
        None | Some(Value::Null) => None,
        Some(Value::String(q)) => Some(q.clone()).filter(|q| !q.trim().is_empty()),
        Some(other) => return Err(format!("{tool}: `query` must be a string, not {other}")),
    };
    Ok((
        target_id,
        crate::acp::computer_tools::SnapshotRequest {
            max_chars: computer_optional_u32(arguments, tool, "maxChars")?.map(|n| n as usize),
            max_depth: computer_optional_u32(arguments, tool, "maxDepth")?.filter(|n| *n > 0),
            max_elements: computer_optional_u32(arguments, tool, "maxElements")?.filter(|n| *n > 0),
            query,
        },
    ))
}
