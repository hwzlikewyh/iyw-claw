mod activation;
mod archive;
mod client;
mod download;
mod install;
mod inventory;
mod model;
mod paths;

use std::process::ExitCode;

use anyhow::{bail, Context, Result};

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("iyw-environment: {error:#}");
            ExitCode::from(30)
        }
    }
}

fn run() -> Result<u8> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let command = args.first().map(String::as_str).unwrap_or("diagnose");
    match command {
        "diagnose" => install::diagnose(),
        "install" => run_install(&args[1..]),
        "repair" => run_repair(&args[1..]),
        "--version" | "-V" => {
            println!("iyw-environment {}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        _ => bail!("unsupported command: {command}"),
    }
}

fn run_install(args: &[String]) -> Result<u8> {
    let phase = option(args, "--phase").context("--phase is required")?;
    match phase.as_str() {
        "prepare" => {
            let version = option(args, "--app-version").context("--app-version is required")?;
            install::prepare(&version)?;
            Ok(0)
        }
        "commit" => {
            install::commit(option(args, "--transaction-id").as_deref())?;
            Ok(0)
        }
        _ => bail!("unsupported install phase: {phase}"),
    }
}

fn run_repair(args: &[String]) -> Result<u8> {
    let version = option(args, "--app-version")
        .or_else(inventory::current_pc_version)
        .context("repair requires an installed environment or --app-version")?;
    let transaction = install::prepare(&version)?;
    install::commit(Some(&transaction))?;
    Ok(0)
}

fn option(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}
