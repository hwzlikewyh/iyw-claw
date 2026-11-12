use std::path::PathBuf;

#[cfg(test)]
#[path = "location_tests.rs"]
mod tests;

pub(super) fn root() -> PathBuf {
    #[cfg(debug_assertions)]
    if let Some(root) = std::env::var_os("IYW_CLAW_ENVIRONMENT_TEST_ROOT") {
        return PathBuf::from(root);
    }
    crate::paths::iyw_claw_user_dir()
}

pub(super) fn version() -> Result<String, String> {
    #[cfg(debug_assertions)]
    if let Ok(version) = std::env::var("IYW_CLAW_ENVIRONMENT_PREVIEW_VERSION") {
        let root = root();
        let user_root = crate::paths::iyw_claw_user_dir();
        let canonical_roots_match = matches!(
            (root.canonicalize().ok(), user_root.canonicalize().ok()),
            (Some(root), Some(user_root)) if root == user_root
        );
        if !root.is_absolute()
            || root == user_root
            || canonical_roots_match
            || semver::Version::parse(&version).is_err()
        {
            return Err("预览环境必须使用独立目录和有效的已发布版本".into());
        }
        return Ok(version);
    }
    Ok(env!("CARGO_PKG_VERSION").to_string())
}
