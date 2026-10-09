pub(crate) fn computer_target(
    arguments: &Value,
    tool: &str,
) -> Result<Option<crate::computer::types::AgentTarget>, String> {
    use crate::computer::types::{AgentTarget, ElementTarget, PointTarget};
    let present = |k: &str| arguments.get(k).is_some_and(|v| !v.is_null());
    match (present("ref"), present("coordinate")) {
        (true, true) => Err(format!("{tool}: give `ref` or `coordinate`, not both")),
        (true, false) => Ok(Some(AgentTarget::Element(ElementTarget {
            index: computer_ref(&arguments["ref"], tool)?,
            generation: computer_generation(arguments, tool, "ref")?,
        }))),
        (false, true) => {
            let (x, y) = computer_coordinate(&arguments["coordinate"], tool)?;
            Ok(Some(AgentTarget::Point(PointTarget {
                x,
                y,
                generation: computer_generation(arguments, tool, "coordinate")?,
            })))
        }
        (false, false) => Ok(None),
    }
}
