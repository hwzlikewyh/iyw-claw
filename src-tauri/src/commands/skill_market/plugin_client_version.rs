use std::collections::BTreeMap;

use crate::app_error::AppCommandError;

pub(super) const INCOMPATIBLE_I18N: &str = "SkillMarketV2.install.clientIncompatible";
const MAX_VERSION_BYTES: usize = 64;

pub(crate) fn validate(value: &str) -> Result<(), AppCommandError> {
    if value.is_empty() {
        return Ok(());
    }
    if value.len() > MAX_VERSION_BYTES || value != value.trim() {
        return Err(invalid());
    }
    let parsed = semver::Version::parse(value).map_err(|_| invalid())?;
    if parsed.to_string() != value {
        return Err(invalid());
    }
    Ok(())
}

pub(crate) fn require_current(minimum: &str) -> Result<(), AppCommandError> {
    validate(minimum)?;
    if minimum.is_empty() {
        return Ok(());
    }
    let current = env!("CARGO_PKG_VERSION");
    let have = semver::Version::parse(current).map_err(|_| invalid())?;
    let want = semver::Version::parse(minimum).map_err(|_| invalid())?;
    if have.cmp_precedence(&want).is_lt() {
        tracing::warn!(
            minimum,
            current,
            "[plugin-install] 宿主版本低于插件最低版本"
        );
        return Err(incompatible(minimum));
    }
    Ok(())
}

pub(super) fn incompatible(minimum: &str) -> AppCommandError {
    AppCommandError::invalid_input("请升级插件运行所在的客户端后再安装此版本")
        .with_detail("SKILL_CLIENT_INCOMPATIBLE")
        .with_i18n(
            INCOMPATIBLE_I18N,
            BTreeMap::from([("minimum".into(), minimum.into())]),
        )
}

pub(super) fn context() -> serde_json::Value {
    serde_json::json!({
        "clientVersion": env!("CARGO_PKG_VERSION"),
        "target": crate::acp::version_center::current_target(),
        "arch": crate::acp::version_center::current_arch(),
    })
}

pub(super) fn nonempty<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<String, D::Error> {
    use serde::Deserialize;
    let value = String::deserialize(deserializer)?;
    if value.is_empty() {
        return Err(serde::de::Error::custom(
            "minClientVersion must be a non-empty SemVer string",
        ));
    }
    validate(&value).map_err(serde::de::Error::custom)?;
    Ok(value)
}

fn invalid() -> AppCommandError {
    AppCommandError::configuration_invalid("插件最低客户端版本声明无效")
}
