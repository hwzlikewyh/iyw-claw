const ACTION_ARGUMENTS_0: &[&str] = &[
    "targetId",
    "target_id",
    "ref",
    "coordinate",
    "generation",
    "button",
    "count",
    "modifiers",
    "delivery",
];

const ACTION_ARGUMENTS_1: &[&str] = &[
    "targetId",
    "target_id",
    "from",
    "to",
    "generation",
    "button",
    "modifiers",
    "durationMs",
    "delivery",
];

const ACTION_ARGUMENTS_2: &[&str] = &[
    "targetId",
    "target_id",
    "direction",
    "amount",
    "unit",
    "ref",
    "coordinate",
    "generation",
    "delivery",
];

const ACTION_ARGUMENTS_3: &[&str] = &[
    "targetId",
    "target_id",
    "ref",
    "generation",
    "text",
    "submit",
    "delivery",
];

const ACTION_ARGUMENTS_4: &[&str] = &[
    "targetId",
    "target_id",
    "key",
    "modifiers",
    "repeat",
    "ref",
    "generation",
    "delivery",
];

const ACTION_ARGUMENTS_5: &[&str] = &[
    "targetId",
    "target_id",
    "key",
    "modifiers",
    "durationMs",
    "ref",
    "generation",
    "delivery",
];

const ACTION_ARGUMENTS_6: &[&str] = &["targetId", "target_id", "ref", "generation", "value"];

const ACTION_ARGUMENTS_7: &[&str] = &["targetId", "target_id"];

const ACTION_ARGUMENTS_8: &[&str] = &["targetId", "target_id", "path"];

const ACTION_ARGUMENTS_9: &[&str] = &["targetId", "target_id", "x", "y", "width", "height"];

pub(crate) fn computer_act_arguments(tool: &str) -> &'static [&'static str] {
    match tool {
        "computer_click" => ACTION_ARGUMENTS_0,
        "computer_drag" => ACTION_ARGUMENTS_1,
        "computer_scroll" => ACTION_ARGUMENTS_2,
        "computer_type" => ACTION_ARGUMENTS_3,
        "computer_press_key" => ACTION_ARGUMENTS_4,
        "computer_hold_key" => ACTION_ARGUMENTS_5,
        "computer_set_value" => ACTION_ARGUMENTS_6,
        "computer_restore" => ACTION_ARGUMENTS_7,
        "computer_invoke_menu" => ACTION_ARGUMENTS_8,
        "computer_set_window_frame" => ACTION_ARGUMENTS_9,
        _ => &[],
    }
}
