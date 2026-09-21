use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

const OUTPUT_LIMIT: usize = 16 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(100);

pub fn run(mut command: Command, timeout: Duration) -> Result<String> {
    command
        .env_remove("NODE_OPTIONS")
        .env_remove("NODE_PATH")
        .env_remove("PYTHONHOME")
        .env_remove("PYTHONPATH");
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure(&mut command);
    let mut child = command
        .spawn()
        .context("组件无法启动，请检查系统架构、运行库或安全软件拦截")?;
    let job = match crate::probe_job::ProbeJob::attach(&child) {
        Ok(job) => job,
        Err(error) => {
            terminate(&mut child);
            return Err(error);
        }
    };
    let stdout = drain(child.stdout.take().context("probe stdout unavailable")?);
    let stderr = drain(child.stderr.take().context("probe stderr unavailable")?);
    let result = wait(&mut child, timeout);
    if result.is_err() {
        terminate(&mut child);
    }
    drop(job);
    let out = stdout.join().unwrap_or_default();
    let err = stderr.join().unwrap_or_default();
    let status = result?;
    if !status.success() {
        bail!("组件启动检查失败（{status}）：{}", err.trim());
    }
    Ok(format!("{out}{err}"))
}

pub(crate) fn drain(mut stream: impl Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut result = Vec::new();
        let mut buffer = [0_u8; 4096];
        while let Ok(count) = stream.read(&mut buffer) {
            if count == 0 {
                break;
            }
            let keep = count.min(OUTPUT_LIMIT.saturating_sub(result.len()));
            result.extend_from_slice(&buffer[..keep]);
        }
        String::from_utf8_lossy(&result).into_owned()
    })
}

fn wait(child: &mut Child, timeout: Duration) -> Result<std::process::ExitStatus> {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if started.elapsed() >= timeout {
            bail!("组件启动检查超时（{} 秒）", timeout.as_secs());
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

pub fn configure(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
}

pub fn terminate(child: &mut Child) {
    #[cfg(windows)]
    if child.try_wait().ok().flatten().is_some() {
        return;
    }
    #[cfg(windows)]
    {
        let mut command = Command::new("taskkill.exe");
        command.args(["/PID", &child.id().to_string(), "/T", "/F"]);
        configure(&mut command);
        let _ = command.stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-KILL", "--", &format!("-{}", child.id())])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}
