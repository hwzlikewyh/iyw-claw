use crate::computer::types::ActDelivery;

type ParsedAction = (String, ComputerActRequest, Option<ActDelivery>);

/// 只调度已声明的工具；共享授权仍由宿主执行层重新核对。
pub fn computer_act_request(tool: &str, arguments: &Value) -> Result<ParsedAction, String> {
    check_action_arguments(tool, arguments)?;
    let target_id = computer_target_id(arguments, tool)?;
    let request = action_request(tool, arguments)?;
    let delivery = computer_choice(arguments, tool, "delivery", &["background", "foreground"])?
        .map(|word| match word {
            "foreground" => ActDelivery::Foreground,
            _ => ActDelivery::Background,
        });
    Ok((target_id, request, delivery))
}

fn check_action_arguments(tool: &str, arguments: &Value) -> Result<(), String> {
    let allowed = computer_act_arguments(tool);
    if let Some(unknown) = arguments
        .as_object()
        .and_then(|args| args.keys().find(|k| !allowed.contains(&k.as_str())))
    {
        return Err(format!(
            "{tool} takes no argument `{unknown}`; it takes {}",
            allowed
                .iter()
                .filter(|a| **a != "target_id")
                .map(|a| format!("`{a}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Ok(())
}

fn action_request(tool: &str, arguments: &Value) -> Result<ComputerActRequest, String> {
    match tool {
        "computer_click" => click_request(arguments, tool),
        "computer_drag" => drag_request(arguments, tool),
        "computer_scroll" => scroll_request(arguments, tool),
        "computer_type" => type_request(arguments, tool),
        "computer_press_key" | "computer_hold_key" => key_request(arguments, tool),
        "computer_set_value" => Ok(ComputerActRequest::SetValue {
            target: computer_element(arguments, tool)?,
            value: computer_text(arguments, tool, "value")?,
        }),
        "computer_restore" => Ok(ComputerActRequest::Restore),
        "computer_invoke_menu" => Ok(ComputerActRequest::InvokeMenu {
            path: computer_menu_path(arguments, tool)?,
        }),
        "computer_set_window_frame" => frame_request(arguments, tool),
        other => Err(format!("unknown tool: {other}")),
    }
}
