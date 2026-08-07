use std::path::PathBuf;

use iyw_claw_lib::acp::delegation::companion::binary_capabilities;

pub struct Args {
    pub parent_connection_id: String,
    pub socket_path: String,
    pub token: String,
    pub parent_pid: Option<u32>,
    pub features: Option<String>,
    pub agent_type: Option<String>,
    pub working_dir: PathBuf,
}

#[derive(Default)]
struct ParsedArgs {
    parent_connection_id: Option<String>,
    socket_path: Option<String>,
    token: Option<String>,
    parent_pid: Option<u32>,
    features: Option<String>,
    agent_type: Option<String>,
    working_dir: Option<PathBuf>,
}

pub fn parse_args() -> Result<Args, String> {
    let mut parsed = ParsedArgs::default();
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        parse_arg(&arg, &mut iter, &mut parsed)?;
    }
    Ok(Args {
        parent_connection_id: required(parsed.parent_connection_id, "--parent-connection-id")?,
        socket_path: required(parsed.socket_path, "--socket-path")?,
        token: required(parsed.token, "--token")?,
        parent_pid: parsed.parent_pid,
        features: parsed.features,
        agent_type: parsed.agent_type,
        working_dir: parsed
            .working_dir
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))),
    })
}

fn parse_arg(
    arg: &str,
    iter: &mut impl Iterator<Item = String>,
    parsed: &mut ParsedArgs,
) -> Result<(), String> {
    match arg {
        "--parent-connection-id" => parsed.parent_connection_id = Some(next(iter, arg)?),
        "--socket-path" => parsed.socket_path = Some(next(iter, arg)?),
        "--token" => parsed.token = Some(next(iter, arg)?),
        "--features" => parsed.features = Some(next(iter, arg)?),
        "--agent-type" => parsed.agent_type = Some(next(iter, arg)?),
        "--working-dir" => parsed.working_dir = Some(PathBuf::from(next(iter, arg)?)),
        "--parent-pid" => {
            let raw = next(iter, arg)?;
            parsed.parent_pid = Some(
                raw.parse::<u32>()
                    .map_err(|error| format!("--parent-pid must be a u32: {error}"))?,
            );
        }
        "--help" | "-h" => print_help(),
        "--capabilities" => print_and_exit(binary_capabilities()),
        "--version" | "-V" => print_and_exit(env!("CARGO_PKG_VERSION")),
        other => return Err(format!("unknown arg: {other}")),
    }
    Ok(())
}

fn next(iter: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    iter.next()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn required(value: Option<String>, flag: &str) -> Result<String, String> {
    value.ok_or_else(|| format!("missing {flag}"))
}

fn print_help() {
    println!(
        "iyw-claw-mcp --parent-connection-id <uuid> --socket-path <path> --token <secret> [--agent-type <type>] [--parent-pid <pid>] [--features delegation,feedback,ask,sessions,images,memory,memory-proposal,artifacts] [--working-dir <path>]\n\
         iyw-claw-mcp tool scheduled-task <list|create|update|delete> [--input <json>|--stdin] [--socket-path <path>] [--agent-type <type>]"
    );
    std::process::exit(0);
}

fn print_and_exit(value: impl std::fmt::Display) {
    println!("{value}");
    std::process::exit(0);
}
