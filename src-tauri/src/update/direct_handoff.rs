use super::{desktop::Task, desktop_cache, direct_installer, preferences, release, silent_safety};
use std::{path::Path, time::Duration};
use tauri::Manager;
use tauri_plugin_updater::Update;
use tokio::process::{Child, Command};

const CAPABILITY_TIMEOUT: Duration = Duration::from_secs(15);
const HANDOFF_TIMEOUT: Duration = Duration::from_secs(20 * 60);
const HANDOFF_POLL: Duration = Duration::from_millis(250);

async fn verify_capability(path: &Path, update: &Update) -> Result<(), String> {
    let output = tokio::time::timeout(
        CAPABILITY_TIMEOUT,
        crate::process::tokio_command(path)
            .arg("--capabilities")
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| "Installer capability check timed out")?
    .map_err(|e| e.to_string())?;
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| "Installer does not support direct payload deployment")?;
    if !output.status.success() || value["protocol"] != 2 {
        return Err("Installer protocol mismatch".into());
    }
    let supported = if super::direct_payload::selected(update) {
        value["payload"] == "app_payload_zip"
            && value["target"] == "windows"
            && value["arch"] == "x86_64"
    } else {
        let (target, arch) = super::direct_payload::platform();
        let platform = format!("{target}/{arch}");
        value["nsis_targets"].as_array().is_some_and(|targets| {
            targets
                .iter()
                .any(|v| v.as_str() == Some(platform.as_str()))
        })
    };
    if !supported {
        return Err("Installer does not support this update package and platform".into());
    }
    Ok(())
}

async fn recheck(task: &Task, update: &Update, silent: bool) -> Result<(), String> {
    let idle = if silent {
        silent_safety::IDLE_DELAY
    } else {
        Duration::ZERO
    };
    let blocker = if task.force_resume {
        silent_safety::non_agent_blocker(&task.app).await?
    } else {
        silent_safety::task_blocker_with_idle_delay(&task.app, idle).await?
    };
    if let Some(reason) = blocker {
        return Err(format!("update_blocked:{reason}"));
    }
    let settings = preferences::load(&task.conn)
        .await
        .map_err(|e| e.to_string())?;
    if settings.channel.as_str() != "stable" {
        return Err("Update channel changed".into());
    }
    if silent
        && (!settings.silent_auto_update
            || !settings.auto_check
            || silent_safety::blocker(&task.app).is_some()
            || settings.suppresses(
                &update.version,
                update.raw_json["update_policy"] == "required",
            ))
    {
        return Err("Automatic update deferred because eligibility changed".into());
    }
    Ok(())
}

fn arguments(command: &mut Command, task: &Task, update: &Update) -> Result<(), String> {
    let current = std::env::current_exe().map_err(|e| e.to_string())?;
    let root = crate::desktop_bootstrap::resolve_install_root(&current)
        .ok_or("Installed app root is unavailable")?;
    let ext = release::extensions(update).map_err(|e| e.to_string())?;
    let package_kind = if super::direct_payload::selected(update) {
        "app_payload_zip"
    } else {
        "nsis"
    };
    command
        .arg("--update-install-dir")
        .arg(root)
        .arg("--update-package-cache")
        .arg(desktop_cache::cache_path(&task.app, update)?)
        .args([
            "--update-version",
            &update.version,
            "--update-url",
            update.download_url.as_str(),
            "--update-signature",
            &update.signature,
            "--update-package-kind",
            package_kind,
            "--update-channel",
            "stable",
        ])
        .arg("--update-size")
        .arg(ext.size.ok_or("Missing payload size")?.to_string())
        .arg("--update-sha256")
        .arg(ext.sha256.ok_or("Missing payload digest")?)
        .arg("--update-release-id")
        .arg(ext.release_id.ok_or("Missing release ID")?);
    Ok(())
}

async fn wait_ready(child: &mut Child, ready: &Path) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + HANDOFF_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        if tokio::fs::metadata(ready)
            .await
            .is_ok_and(|m| m.len() < 64 * 1024)
        {
            if let Ok(bytes) = tokio::fs::read(ready).await {
                let value = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap_or_default();
                if value["protocol"] == 2
                    && value["error_pid"].as_u64() == child.id().map(u64::from)
                {
                    if let Some(error) = value["error"].as_str() {
                        return Err(error.into());
                    }
                }
                if value["protocol"] == 2 && value["pid"].as_u64() == child.id().map(u64::from) {
                    return Ok(());
                }
            }
        }
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            return Err("Installer exited before accepting the update".into());
        }
        tokio::time::sleep(HANDOFF_POLL).await;
    }
    Err("Installer preparation timed out; current application remains running".into())
}

pub(super) async fn launch(task: &Task, update: &Update, silent: bool) -> Result<(), String> {
    let installer = direct_installer::prepare(task, update).await?;
    verify_capability(&installer.path, update).await?;
    recheck(task, update, silent).await?;
    let directory = tempfile::Builder::new()
        .prefix("iyw-update-handoff-")
        .tempdir()
        .map_err(|e| e.to_string())?;
    let ready = directory.path().join("ready.json");
    let mut command = crate::process::tokio_command(&installer.path);
    command.arg(if silent {
        "--update-silent"
    } else {
        "--update-mode"
    });
    arguments(&mut command, task, update)?;
    let mut child = spawn_installer(&mut command, &ready)?;
    wait_ready(&mut child, &ready).await?;
    let _manual_relaunch = if silent || task.relaunch_claimed {
        None
    } else if task.force_resume {
        Some(super::task_recovery::pause(task).await?)
    } else {
        Some(
            task.app
                .state::<crate::acp::manager::ConnectionManager>()
                .claim_relaunch()
                .await
                .map_err(|e| e.to_string())?,
        )
    };
    // 下载、校验及准备期间用户可能恢复操作；退出前再次检查。
    recheck(task, update, silent).await?;
    super::state::set_installing(&task.handle, &task.emitter());
    directory.close().map_err(|e| e.to_string())?;
    tracing::info!(version=%update.version, "[app-update] verified installer ready; shutting down normally");
    shutdown(task).await
}

fn spawn_installer(command: &mut Command, ready: &Path) -> Result<Child, String> {
    command
        .arg("--update-parent-pid")
        .arg(std::process::id().to_string())
        .arg("--update-ready-file")
        .arg(ready)
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    command.spawn().map_err(|e| e.to_string())
}

async fn shutdown(task: &Task) -> Result<(), String> {
    let app = task.app.clone();
    tokio::task::spawn_blocking(move || {
        crate::desktop_shutdown::shutdown_blocking(
            &app,
            crate::desktop_shutdown::ShutdownReason::WindowsUpdate,
        )
    })
    .await
    .map_err(|e| e.to_string())?;
    task.app.cleanup_before_exit();
    // 不执行 Child 的 kill_on_drop：向导已接手，并等待这个父进程正常退出。
    std::process::exit(0);
}
