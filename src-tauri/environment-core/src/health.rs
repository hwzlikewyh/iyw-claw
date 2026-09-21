use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{bail, Context, Result};

use crate::paths::from_slash;
use crate::probe_process;

const PROBE_TIMEOUT: Duration = Duration::from_secs(30);

pub fn verify(root: &Path, component: &str, entries: &BTreeMap<String, String>) -> Result<()> {
    let entry = |name: &str| -> Result<PathBuf> {
        from_slash(root, entries.get(name).context("组件缺少必需入口")?)
    };
    let result = match component {
        "node" => node(&entry("node")?),
        "uv" => version(&entry("uv")?).and_then(|_| version(&entry("uvx")?)),
        "chromix" => browser(root, &entry("chromix")?),
        "agent-reach" => python(root),
        "open-computer-use" => help(&entry(component)?),
        "git" | "agent-browser" | "officecli" | "environment-maintainer" => {
            version(&entry(component)?)
        }
        _ => bail!("unknown health probe component"),
    };
    result.with_context(|| format!("{component} 实际启动检查未通过"))
}

fn version(path: &Path) -> Result<()> {
    let mut command = Command::new(path);
    command.arg("--version");
    let output = probe_process::run(command, PROBE_TIMEOUT)?;
    if !output.chars().any(|c| c.is_ascii_digit()) {
        bail!("组件没有返回有效版本号");
    }
    Ok(())
}

fn help(path: &Path) -> Result<()> {
    let mut command = Command::new(path);
    command.arg("--help");
    let output = probe_process::run(command, PROBE_TIMEOUT)?;
    if output.trim().is_empty() {
        bail!("组件未返回帮助信息");
    }
    Ok(())
}

fn node(executable: &Path) -> Result<()> {
    version(executable)?;
    let bin = executable.parent().context("Node has no parent")?;
    let npm_root = if cfg!(windows) {
        bin.join("node_modules/npm/bin")
    } else {
        bin.join("../lib/node_modules/npm/bin")
    };
    for script in ["npm-cli.js", "npx-cli.js"] {
        let mut command = Command::new(executable);
        command.arg(npm_root.join(script)).arg("--version");
        probe_process::run(command, PROBE_TIMEOUT)?;
    }
    Ok(())
}

fn python(root: &Path) -> Result<()> {
    let relative = if cfg!(windows) {
        "python/python.exe"
    } else {
        "python/bin/python3"
    };
    let mut command = Command::new(root.join(relative));
    command.args(["-B", "-m", "agent_reach.cli", "--help"]);
    probe_process::run(command, PROBE_TIMEOUT)?;
    Ok(())
}

fn browser(root: &Path, executable: &Path) -> Result<()> {
    let scratch = root
        .parent()
        .context("browser staging has no parent")?
        .join(format!("browser-probe-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir(&scratch)?;
    let result = browser_page(executable, &scratch);
    // 探针仅清理本次生成的独立目录，不触碰用户 profile。
    let cleanup = std::fs::remove_dir_all(&scratch);
    result.and_then(|()| cleanup.context("清理浏览器检查目录失败"))
}

fn browser_page(executable: &Path, scratch: &Path) -> Result<()> {
    crate::browser_probe::verify(executable, scratch)
}
