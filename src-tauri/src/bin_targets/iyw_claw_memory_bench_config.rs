use std::fs;
use std::path::{Path, PathBuf};

use iyw_claw_lib::user_memory::bench::BenchQuery;

const DEFAULT_SEED: u64 = 20_260_815;

#[derive(Clone)]
pub(crate) struct Config {
    pub size: usize,
    pub seed: u64,
}

pub(crate) fn parse_config(args: &[String]) -> Result<Config, String> {
    let mut config = Config {
        size: 1_000,
        seed: DEFAULT_SEED,
    };
    parse_options(args, |flag, value| {
        match flag {
            "--size" => config.size = parse_size(value)?,
            "--seed" => config.seed = parse_seed(value)?,
            _ => return Err(format!("unsupported option: {flag}")),
        }
        Ok(())
    })?;
    Ok(config)
}

pub(crate) fn parse_suite_seed(args: &[String]) -> Result<u64, String> {
    let mut seed = DEFAULT_SEED;
    parse_options(args, |flag, value| {
        match flag {
            "--seed" => seed = parse_seed(value)?,
            _ => return Err(format!("unsupported suite option: {flag}")),
        }
        Ok(())
    })?;
    Ok(seed)
}

pub(crate) fn parse_cold_args(args: &[String]) -> Result<(PathBuf, BenchQuery), String> {
    let mut root = None;
    let mut query = None;
    parse_options(args, |flag, value| {
        match flag {
            "--db-root" => root = Some(PathBuf::from(value)),
            "--query-json" => {
                query = Some(serde_json::from_str(value).map_err(|error| error.to_string())?)
            }
            _ => return Err(format!("unsupported cold child option: {flag}")),
        }
        Ok(())
    })?;
    Ok((
        root.ok_or_else(|| "missing cold child database root".to_string())?,
        query.ok_or_else(|| "missing cold child query".to_string())?,
    ))
}

pub(crate) fn output_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("target/memory-recall-bench")
}

pub(crate) fn corpus_file_name(config: &Config) -> String {
    format!("corpus-v1-{}-{:016x}.jsonl", config.size, config.seed)
}

pub(crate) fn report_file_name(config: &Config) -> String {
    format!("report-v4-{}-{:016x}.json", config.size, config.seed)
}

pub(crate) fn suite_report_file_name(seed: u64) -> String {
    format!("suite-v1-{seed:016x}.json")
}

pub(crate) fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::create_dir_all(output_dir()).map_err(|error| error.to_string())?;
    fs::write(path, bytes).map_err(|error| error.to_string())
}

fn parse_options(
    args: &[String],
    mut apply: impl FnMut(&str, &str) -> Result<(), String>,
) -> Result<(), String> {
    let mut values = args.iter();
    while let Some(flag) = values.next() {
        let value = values
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        apply(flag, value)?;
    }
    Ok(())
}

fn parse_size(value: &str) -> Result<usize, String> {
    let size = value
        .parse()
        .map_err(|_| "invalid corpus size".to_string())?;
    [1_000, 10_000, 50_000]
        .contains(&size)
        .then_some(size)
        .ok_or_else(|| "size must be 1000, 10000, or 50000".to_string())
}

fn parse_seed(value: &str) -> Result<u64, String> {
    value.parse().map_err(|_| "invalid seed".to_string())
}
