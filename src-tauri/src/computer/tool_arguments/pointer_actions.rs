use crate::computer::types::{ComputerActRequest, PointerButton, ScrollDirection, ScrollUnit};

fn pointer_button(arguments: &Value, tool: &str) -> Result<PointerButton, String> {
    Ok(
        match computer_choice(arguments, tool, "button", &["left", "right", "middle"])? {
            Some("right") => PointerButton::Right,
            Some("middle") => PointerButton::Middle,
            _ => PointerButton::Left,
        },
    )
}

fn click_request(arguments: &Value, tool: &str) -> Result<ComputerActRequest, String> {
    let target = computer_target(arguments, tool)?.ok_or_else(|| {
        format!(
            "{tool} requires `ref` (from computer_snapshot) or `coordinate` (from \
                 computer_screenshot), with its `generation`"
        )
    })?;
    let button = pointer_button(arguments, tool)?;
    let count = computer_count(arguments, tool, "count", 1..=2, 1)?;
    if count == 2 && button != PointerButton::Left {
        return Err(format!(
            "{tool}: a double click (`count: 2`) is with the left button only"
        ));
    }
    Ok(ComputerActRequest::Click {
        target,
        button,
        count: count as u8,
        modifiers: computer_modifiers(arguments, tool)?,
    })
}

fn drag_request(arguments: &Value, tool: &str) -> Result<ComputerActRequest, String> {
    let generation = computer_generation(arguments, tool, "from")?;
    let point = |name: &str| -> Result<crate::computer::types::PointTarget, String> {
        let value = arguments
            .get(name)
            .filter(|v| !v.is_null())
            .ok_or_else(|| {
                format!(
                    "{tool} requires `from` and `to`, each an [x, y] point in the \
                     image computer_screenshot returned"
                )
            })?;
        let (x, y) = computer_coordinate(value, tool)?;
        Ok(crate::computer::types::PointTarget {
            generation: generation.clone(),
            x,
            y,
        })
    };
    let duration_ms = match arguments.get("durationMs").filter(|v| !v.is_null()) {
        None => None,
        Some(_) => Some(computer_count(
            arguments,
            tool,
            "durationMs",
            0..=crate::computer::types::MAX_DRAG_MS,
            0,
        )?),
    };
    Ok(ComputerActRequest::Drag {
        from: point("from")?,
        to: point("to")?,
        button: pointer_button(arguments, tool)?,
        modifiers: computer_modifiers(arguments, tool)?,
        duration_ms,
    })
}

fn scroll_request(arguments: &Value, tool: &str) -> Result<ComputerActRequest, String> {
    let direction = match computer_choice(
        arguments,
        tool,
        "direction",
        &["up", "down", "left", "right"],
    )? {
        Some("up") => ScrollDirection::Up,
        Some("down") => ScrollDirection::Down,
        Some("left") => ScrollDirection::Left,
        Some("right") => ScrollDirection::Right,
        _ => {
            return Err(format!(
                "{tool} requires `direction`: up, down, left or right"
            ))
        }
    };
    let unit = match computer_choice(arguments, tool, "unit", &["line", "page"])? {
        Some("page") => ScrollUnit::Page,
        _ => ScrollUnit::Line,
    };
    Ok(ComputerActRequest::Scroll {
        target: computer_target(arguments, tool)?,
        direction,
        amount: computer_count(
            arguments,
            tool,
            "amount",
            1..=crate::computer::types::MAX_SCROLL_AMOUNT,
            3,
        )?,
        unit,
    })
}
