pub fn computer_capture_request(arguments: &Value) -> Result<(String, Option<u32>), String> {
    let target_id = computer_target_id(arguments, "computer_screenshot")?;
    let max = computer_optional_u32(arguments, "computer_screenshot", "maxDimension")?;
    if max == Some(0) {
        return Err(
            "computer_screenshot: `maxDimension` must be at least 1; leave it out for the default"
                .to_string(),
        );
    }
    Ok((target_id, max))
}
