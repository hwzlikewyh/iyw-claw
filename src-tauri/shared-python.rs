//! 安装引导与应用共用的持久 Python 环境；只在初始化或修复入口创建。

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const PYTHON_VERSION: &str = "3.13";
const SETUP_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const PROGRESS_INTERVAL: Duration = Duration::from_secs(15);
const ERROR_TAIL_BYTES: u64 = 4096;
const PROBE: &str = "import os,sys; assert sys.version_info[:2] == (3,13); assert sys.prefix != sys.base_prefix; assert os.path.normcase(os.path.abspath(sys.prefix)) == os.path.normcase(os.path.abspath(sys.argv[1]))";

pub fn supported() -> bool {
    !cfg!(all(windows, target_vendor = "win7"))
}

pub fn environment_dir(runtime: &Path) -> PathBuf {
    runtime.join("envs/python")
}

pub fn bin_dir(environment: &Path) -> PathBuf {
    environment.join(if cfg!(windows) { "Scripts" } else { "bin" })
}

pub fn interpreter(environment: &Path) -> PathBuf {
    bin_dir(environment).join(if cfg!(windows) {
        "python.exe"
    } else {
        "python"
    })
}

pub fn available(runtime: &Path) -> bool {
    let environment = environment_dir(runtime);
    environment.join("pyvenv.cfg").is_file() && interpreter(&environment).is_file()
}

pub fn ensure(runtime: &Path, uv: &Path, progress: &dyn Fn(&str)) -> Result<(), String> {
    if !supported() {
        return Ok(());
    }
    let environment = environment_dir(runtime);
    fs::create_dir_all(runtime.join("envs")).map_err(|error| error.to_string())?;
    let _lock = writer_lock(runtime, progress)?;
    let log = runtime.join("envs/.python-setup.log");
    if available(runtime) {
        verify(&environment, &log)?;
        progress("已复用共享 Python 环境");
        return Ok(());
    }
    // 不覆盖已安装依赖或切换已有环境的解释器；损坏时明确报错。
    if environment.exists()
        && fs::read_dir(&environment)
            .map_err(|error| error.to_string())?
            .next()
            .is_some()
    {
        return Err(format!(
            "共享 Python 环境损坏，保留原目录，请检查：{}",
            environment.display()
        ));
    }
    progress("正在准备共享 Python 3.13 环境；首次使用可能需要下载解释器");
    run(
        venv_command(runtime, uv, &environment),
        &log,
        (SETUP_TIMEOUT, progress),
    )?;
    verify(&environment, &log)?;
    progress("共享 Python 环境已就绪，后续任务直接复用");
    Ok(())
}

fn venv_command(runtime: &Path, uv: &Path, environment: &Path) -> Command {
    let mut command = Command::new(uv);
    command.args([
        "venv",
        "--python",
        PYTHON_VERSION,
        "--managed-python",
        "--no-project",
        "--no-config",
    ]);
    command.arg(&environment).current_dir(runtime);
    command.env("UV_CACHE_DIR", runtime.join("cache/uv"));
    command.env("UV_PYTHON_INSTALL_DIR", runtime.join("uv/python"));
    // 创建目标是固定的宿主环境，不继承任务对项目、解释器或清理行为的覆盖。
    for key in [
        "VIRTUAL_ENV",
        "CONDA_PREFIX",
        "PYTHONHOME",
        "PYTHONPATH",
        "UV_PROJECT",
        "UV_WORKING_DIR",
        "UV_PYTHON",
        "UV_NO_MANAGED_PYTHON",
        "UV_PYTHON_DOWNLOADS",
        "UV_VENV_CLEAR",
        "UV_VENV_SEED",
    ] {
        command.env_remove(key);
    }
    command
}

fn writer_lock(runtime: &Path, progress: &dyn Fn(&str)) -> Result<File, String> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(runtime.join("envs/.python-writer.lock"))
        .map_err(|error| format!("无法打开共享 Python 初始化锁：{error}"))?;
    let started = Instant::now();
    let mut last_progress = started;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) if started.elapsed() < SETUP_TIMEOUT => {
                if last_progress.elapsed() >= PROGRESS_INTERVAL {
                    progress("另一个窗口正在准备共享 Python 环境，等待完成后复用");
                    last_progress = Instant::now();
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(error) => return Err(format!("共享 Python 环境写入锁不可用：{error}")),
        }
    }
}

fn verify(environment: &Path, log: &Path) -> Result<(), String> {
    let mut command = Command::new(interpreter(environment));
    command.args(["-I", "-c", PROBE]).arg(environment);
    run(command, log, (PROBE_TIMEOUT, &|_| {})).map_err(|error| {
        format!(
            "共享 Python 环境不可用（{}）：{error}",
            environment.display()
        )
    })
}

fn run(mut command: Command, log: &Path, options: (Duration, &dyn Fn(&str))) -> Result<(), String> {
    let output =
        File::create(log).map_err(|error| format!("无法创建 Python 初始化日志：{error}"))?;
    command
        .stdin(Stdio::null())
        .stdout(output.try_clone().map_err(|error| error.to_string())?)
        .stderr(output);
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
    let mut child = command
        .spawn()
        .map_err(|error| format!("无法启动共享 Python 初始化命令：{error}"))?;
    let result = wait(&mut child, options);
    if result.is_err() && child.try_wait().ok().flatten().is_none() {
        terminate(&mut child);
    }
    result.map_err(|error| format!("{error}；{}", error_tail(log)))
}

fn terminate(child: &mut std::process::Child) {
    #[cfg(windows)]
    {
        let mut command = Command::new("taskkill.exe");
        command.args(["/PID", &child.id().to_string(), "/T", "/F"]);
        command.stdout(Stdio::null()).stderr(Stdio::null());
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
        let _ = command.status();
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

fn wait(child: &mut std::process::Child, options: (Duration, &dyn Fn(&str))) -> Result<(), String> {
    let (timeout, progress) = options;
    let started = Instant::now();
    let mut last_progress = started;
    loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            return if status.success() {
                Ok(())
            } else {
                Err(format!("Python 初始化命令失败（{status}）"))
            };
        }
        if started.elapsed() >= timeout {
            return Err(format!(
                "Python 初始化超时（{} 秒），请检查网络后重试",
                timeout.as_secs()
            ));
        }
        if last_progress.elapsed() >= PROGRESS_INTERVAL {
            progress(&format!(
                "正在准备共享 Python 环境，已等待 {} 秒",
                started.elapsed().as_secs()
            ));
            last_progress = Instant::now();
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

fn error_tail(log: &Path) -> String {
    let read = || -> std::io::Result<String> {
        let mut file = File::open(log)?;
        let start = file.metadata()?.len().saturating_sub(ERROR_TAIL_BYTES);
        file.seek(SeekFrom::Start(start))?;
        let mut bytes = Vec::new();
        file.take(ERROR_TAIL_BYTES).read_to_end(&mut bytes)?;
        Ok(String::from_utf8_lossy(&bytes).trim().to_string())
    };
    read().unwrap_or_else(|error| format!("无法读取初始化日志：{error}"))
}
