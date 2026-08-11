use std::fs;

mod selectors;
mod tools;
mod wire;

pub(crate) use selectors::{
    build_set_model_params, parse_effort_specs, set_effort_selector_for_model, synthesize_options,
    EffortSpecs,
};
pub(crate) use tools::{live_tool_output, unwrap_use_tool};
pub(crate) use wire::{apply_preferred_options, set_config_option};

const PERMISSION_MODES: &[&str] = &[
    "default",
    "acceptEdits",
    "auto",
    "dontAsk",
    "bypassPermissions",
    "plan",
];

fn migrate_permission_mode(value: &str) -> &str {
    match value {
        "always-approve" => "bypassPermissions",
        "ask" => "default",
        other => other,
    }
}

fn permission_mode_from_toml(raw: &str) -> Option<String> {
    let value = raw.parse::<toml::Value>().ok()?;
    let mode = value
        .get("ui")?
        .get("permission_mode")?
        .as_str()
        .map(migrate_permission_mode)?;
    (mode != "default" && PERMISSION_MODES.contains(&mode)).then(|| mode.to_string())
}

fn launch_args_from_toml(
    subcommand: &[&str],
    raw: Option<&str>,
    rules: Option<&str>,
) -> Vec<String> {
    let mut args = vec!["--no-auto-update".to_string()];
    if let Some(mode) = raw.and_then(permission_mode_from_toml) {
        args.push("--permission-mode".to_string());
        args.push(mode);
    }
    if let Some(rules) = rules {
        args.extend(["--rules".to_string(), rules.to_string()]);
    }
    args.extend(subcommand.iter().map(|arg| (*arg).to_string()));
    args
}

pub(crate) fn launch_args(subcommand: &[&str], rules: Option<&str>) -> Vec<String> {
    let config_path = crate::parsers::grok::resolve_grok_home_dir().join("config.toml");
    let raw = fs::read_to_string(config_path).ok();
    launch_args_from_toml(subcommand, raw.as_deref(), rules)
}
