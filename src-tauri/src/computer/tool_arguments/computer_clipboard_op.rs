pub(crate) fn computer_clipboard_op(
    tool: &str,
    arguments: &Value,
) -> Result<crate::acp::computer_tools::ClipboardOp, String> {
    use crate::acp::computer_tools::ClipboardOp;
    let takes: &[&str] = if tool == "computer_clipboard_write" {
        &["text"]
    } else {
        &[]
    };
    if let Some(unknown) = arguments
        .as_object()
        .into_iter()
        .flat_map(|o| o.keys())
        .find(|k| !takes.contains(&k.as_str()))
    {
        return Err(format!("{tool} takes no argument `{unknown}`"));
    }
    if tool != "computer_clipboard_write" {
        return Ok(ClipboardOp::Read);
    }
    match arguments.get("text") {
        Some(Value::String(text)) if text.chars().count() <= MAX_CLIPBOARD_TEXT => {
            Ok(ClipboardOp::Write { text: text.clone() })
        }
        Some(Value::String(_)) => Err(format!(
            "{tool}: `text` may be at most {MAX_CLIPBOARD_TEXT} characters"
        )),
        _ => Err(format!("{tool} requires `text`, a string")),
    }
}
