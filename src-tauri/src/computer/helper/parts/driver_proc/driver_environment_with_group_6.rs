// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// [`driver_environment`], reading the few variables it passes through
/// (Windows' system root; Linux's display and session bus) with `lookup`.
/// Nothing else of the helper's environment is ever consulted.
pub(super) fn driver_environment_with(
    run_dir: &Path,
    lookup: impl Fn(&str) -> Option<String>,
) -> Vec<(String, String)> {
    let run = run_dir.to_string_lossy().to_string();
    let tmp = run_dir.join("tmp").to_string_lossy().to_string();
    let mut env: Vec<(String, String)> = vec![
        ("CUA_DRIVER_RS_TELEMETRY_ENABLED".into(), "false".into()),
        ("CUA_TELEMETRY_ENABLED".into(), "false".into()),
        ("CUA_DRIVER_RS_UPDATE_CHECK".into(), "false".into()),
        ("CUA_DRIVER_EMBEDDED".into(), "1".into()),
        (
            "CUA_DRIVER_RS_SESSION_IDLE_TTL_SECS".into(),
            SESSION_IDLE_TTL_SECS.into(),
        ),
    ];
    if cfg!(windows) {
        let system_root = lookup("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
        env.extend([
            (
                "PATH".into(),
                format!(r"{system_root}\System32;{system_root};{system_root}\System32\Wbem"),
            ),
            ("SystemRoot".into(), system_root.clone()),
            ("windir".into(), system_root),
            ("TEMP".into(), tmp.clone()),
            ("TMP".into(), tmp),
            ("LOCALAPPDATA".into(), run.clone()),
            ("APPDATA".into(), run.clone()),
            ("USERPROFILE".into(), run),
        ]);
    } else {
        env.extend([
            ("PATH".into(), "/usr/bin:/bin:/usr/sbin:/sbin".into()),
            ("HOME".into(), run),
            ("TMPDIR".into(), format!("{tmp}/")),
        ]);
    }
    // Linux: the driver cannot find the display, the accessibility bus or
    // the session's runtime directory without these. X11 is not a boundary
    // against a same-user process to begin with, so passing the session's own
    // addresses through costs nothing the platform had not already given.
    if cfg!(target_os = "linux") {
        for key in [
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "XAUTHORITY",
            "XDG_RUNTIME_DIR",
            "XDG_SESSION_TYPE",
            "XDG_CURRENT_DESKTOP",
            "DBUS_SESSION_BUS_ADDRESS",
        ] {
            if let Some(value) = lookup(key) {
                env.push((key.into(), value));
            }
        }
    }
    env
}

/// Write the driver's configuration file into its home — `$HOME` on macOS and
/// Linux, `%USERPROFILE%` on Windows, both the run directory. See the module
/// note for what it sets.
pub(super) fn write_driver_config(run_dir: &Path) -> std::io::Result<()> {
    let dir = run_dir.join(".cua-driver");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("config.json"), DRIVER_CONFIG)
}

/// Hex SHA-256 of a file.
pub(super) fn file_sha256(path: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

pub(super) fn rejected(why: impl Into<String>) -> HelperError {
    HelperError::new(HelperErrorCode::DriverRejected, why)
}

pub(super) fn unavailable(why: impl Into<String>) -> HelperError {
    HelperError::new(HelperErrorCode::DriverUnavailable, why)
}
