pub(crate) fn computer_coordinate(value: &Value, tool: &str) -> Result<(f64, f64), String> {
    let pair = value.as_array().filter(|a| a.len() == 2).and_then(|a| {
        let x = a[0].as_f64().filter(|v| v.is_finite() && *v >= 0.0)?;
        let y = a[1].as_f64().filter(|v| v.is_finite() && *v >= 0.0)?;
        Some((x, y))
    });
    pair.ok_or_else(|| {
        format!(
            "{tool}: `coordinate` must be [x, y], two non-negative numbers in the pixels of the \
             window's latest computer_screenshot, not {value}"
        )
    })
}
