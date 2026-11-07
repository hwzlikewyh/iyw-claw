use std::collections::BTreeSet;

use crate::app_error::AppCommandError;

const PLATFORMS: [&str; 8] = [
    "windows-x86_64",
    "windows-aarch64",
    "macos-x86_64",
    "macos-aarch64",
    "linux-x86_64",
    "linux-aarch64",
    "windows7-x86_64",
    "windows7-i686",
];

pub(crate) fn normalized(values: &[String]) -> Result<Vec<String>, AppCommandError> {
    if values.len() > PLATFORMS.len() {
        return Err(invalid());
    }
    let mut seen = BTreeSet::new();
    for value in values {
        if !PLATFORMS.contains(&value.as_str()) || !seen.insert(value.clone()) {
            return Err(invalid());
        }
    }
    Ok(seen.into_iter().collect())
}

pub(crate) fn supported_current(values: &[String]) -> bool {
    let Ok(values) = normalized(values) else {
        return false;
    };
    if values.is_empty() {
        return true;
    }
    values.contains(&current_platform())
}

pub(crate) fn require_current(values: &[String]) -> Result<(), AppCommandError> {
    normalized(values)?;
    if supported_current(values) {
        return Ok(());
    }
    let current = current_platform();
    tracing::warn!(platform=%current,supported_platforms=?values,"[plugin-install] 当前平台不在插件发布范围内");
    Err(
        AppCommandError::invalid_input(format!("插件不支持当前平台 {}，请选择适用的版本", current))
            .with_i18n(
                "SkillMarketV2.install.platformUnsupported",
                std::collections::BTreeMap::from([("platform".into(), current)]),
            ),
    )
}

pub(crate) fn current_platform() -> String {
    let target = crate::acp::version_center::current_target();
    let os = if target == "darwin" { "macos" } else { target };
    format!("{os}-{}", crate::acp::version_center::current_arch())
}

fn invalid() -> AppCommandError {
    AppCommandError::configuration_invalid("插件平台声明包含不支持或重复的平台")
}
