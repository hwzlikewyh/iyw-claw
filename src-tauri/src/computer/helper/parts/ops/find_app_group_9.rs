// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The installed application listed under `key` — its bundle identifier or
/// its launch path, as the driver's application list gives them — or else
/// `name`, any case: the one application, running or not, that goes by it.
/// A name two applications go by names neither: the answer lists their
/// keys.
///
/// What it is, for iyw-claw to judge by, is kept apart from the command that
/// starts it ([`InstalledApp`]): its bundle identifier where it has one that
/// is not a path, and its executable — the path the driver gives, or else the
/// first word of its launch command.
pub async fn find_app(
    driver: &DriverProc,
    name: Option<&str>,
    key: Option<&str>,
) -> Result<InstalledApp, HelperError> {
    let result = call(driver, "list_apps", json!({}), LIST_TIMEOUT).await?;
    let listed = required_array("list_apps", structured("list_apps", &result)?, "apps")?;
    let apps = listed.iter().filter_map(|a| {
        let bundle = string(a, "bundle_id");
        let launch_path = string(a, "launch_path");
        // On Windows a "bundle identifier" can be the executable's path.
        let (bundle_id, path) = match bundle {
            Some(b) if b.contains(['/', '\\']) => (None, Some(b)),
            other => (
                other,
                launch_path
                    .as_deref()
                    .and_then(|l| crate::computer::agent::command_words(l).into_iter().next()),
            ),
        };
        Some((
            InstalledApp {
                app: RawApp {
                    pid: a
                        .get("pid")
                        .and_then(Value::as_u64)
                        .and_then(|p| u32::try_from(p).ok())
                        .unwrap_or(0),
                    name: string(a, "name")?,
                    bundle_id,
                    path,
                    active: false,
                    started_at: None,
                },
                launch_path: launch_path.clone(),
            },
            (string(a, "bundle_id"), launch_path),
        ))
    });
    let wanted = |listed: &(Option<String>, Option<String>), app: &RawApp| match (
        key.map(str::trim),
        name.map(str::trim),
    ) {
        (Some(key), _) => listed.0.as_deref() == Some(key) || listed.1.as_deref() == Some(key),
        (None, Some(name)) => app.name.eq_ignore_ascii_case(name),
        (None, None) => false,
    };
    let mut found: Vec<InstalledApp> = Vec::new();
    for (installed, listed) in apps {
        if !wanted(&listed, &installed.app) {
            continue;
        }
        let same = |f: &InstalledApp| {
            f.app.key() == installed.app.key() && f.launch_path == installed.launch_path
        };
        if installed.app.key().is_some() && !found.iter().any(same) {
            found.push(installed);
        }
    }
    let asked = key.or(name).unwrap_or_default();
    match found.len() {
        1 => Ok(found.remove(0)),
        0 => Err(HelperError::new(
            HelperErrorCode::ActionFailed,
            format!(
                "No installed application goes by \"{asked}\". Name it as the system lists \
                 it, or by its key (a bundle identifier or path)."
            ),
        )),
        _ => Err(HelperError::new(
            HelperErrorCode::ActionFailed,
            format!(
                "Several installed applications go by \"{asked}\"; name one by its key: {}.",
                found
                    .iter()
                    .filter_map(|f| f.launch_path.as_deref().or(f.app.key()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )),
    }
}

/// Start `app` — found by [`find_app`] — in the background, by what the
/// driver listed it under: its bundle identifier on macOS; on Windows its
/// launch path, or its application id; on Linux its launch command, which
/// the driver splits into a program and its arguments as its desktop file
/// gave them.
pub async fn launch_app(driver: &DriverProc, app: &InstalledApp) -> Result<RawLaunch, HelperError> {
    let missing = || HelperError::new(HelperErrorCode::BadRequest, "nothing to start it by");
    let args = match crate::computer::keys::Platform::current() {
        crate::computer::keys::Platform::Mac => {
            json!({ "bundle_id": app.app.bundle_id.as_deref().ok_or_else(missing)? })
        }
        crate::computer::keys::Platform::Windows => match (&app.launch_path, &app.app.bundle_id) {
            (Some(path), _) => json!({ "launch_path": path }),
            (None, Some(id)) => json!({ "bundle_id": id }),
            (None, None) => return Err(missing()),
        },
        crate::computer::keys::Platform::Linux => {
            json!({ "launch_path": app.launch_path.as_deref().ok_or_else(missing)? })
        }
    };
    let result = call(driver, "launch_app", args, LAUNCH_TIMEOUT).await?;
    let said = result.structured.as_ref();
    Ok(RawLaunch {
        pid: said
            .and_then(|s| s.get("pid"))
            .and_then(Value::as_u64)
            .and_then(|p| u32::try_from(p).ok())
            .filter(|p| *p > 0),
        name: said
            .and_then(|s| string(s, "name"))
            .unwrap_or_else(|| app.app.name.clone()),
    })
}
