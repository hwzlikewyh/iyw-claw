fn frame_request(arguments: &Value, tool: &str) -> Result<ComputerActRequest, String> {
    let number = |key: &str| -> Result<Option<f64>, String> {
        match arguments.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v
                .as_f64()
                .filter(|n| n.is_finite())
                .map(Some)
                .ok_or_else(|| format!("{tool}: `{key}` must be a number")),
        }
    };
    let (x, y, width, height) = (
        number("x")?,
        number("y")?,
        number("width")?,
        number("height")?,
    );
    if x.is_none() && y.is_none() && width.is_none() && height.is_none() {
        return Err(format!(
            "{tool} requires at least one of `x`, `y`, `width`, `height`"
        ));
    }
    Ok(ComputerActRequest::SetFrame {
        x,
        y,
        width,
        height,
    })
}
