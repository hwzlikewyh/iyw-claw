pub(crate) fn computer_element(
    arguments: &Value,
    tool: &str,
) -> Result<crate::computer::types::ElementTarget, String> {
    use crate::computer::types::AgentTarget;
    match computer_target(arguments, tool)? {
        Some(AgentTarget::Element(element)) => Ok(element),
        _ => Err(format!(
            "{tool} requires `ref` (an element's number from computer_snapshot) and `generation`"
        )),
    }
}
