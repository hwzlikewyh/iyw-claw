use super::*;

#[test]
fn environment_preview_is_isolated_and_release_ignores_overrides() {
    let base =
        std::env::temp_dir().join(format!("iyw-environment-location-{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    std::env::set_var("IYW_CLAW_ENVIRONMENT_TEST_ROOT", &base);
    std::env::set_var("IYW_CLAW_ENVIRONMENT_PREVIEW_VERSION", "0.1.331");
    #[cfg(debug_assertions)]
    {
        assert_eq!(root(), base);
        assert_eq!(version().unwrap(), "0.1.331");
        std::env::set_var(
            "IYW_CLAW_ENVIRONMENT_TEST_ROOT",
            crate::paths::iyw_claw_user_dir(),
        );
        assert!(version().is_err());
        std::env::set_var("IYW_CLAW_ENVIRONMENT_TEST_ROOT", "relative");
        assert!(version().is_err());
        std::env::set_var("IYW_CLAW_ENVIRONMENT_TEST_ROOT", &base);
        std::env::set_var("IYW_CLAW_ENVIRONMENT_PREVIEW_VERSION", "invalid");
        assert!(version().is_err());
    }
    #[cfg(not(debug_assertions))]
    {
        assert_eq!(root(), crate::paths::iyw_claw_user_dir());
        assert_eq!(version().unwrap(), env!("CARGO_PKG_VERSION"));
    }
    std::env::remove_var("IYW_CLAW_ENVIRONMENT_TEST_ROOT");
    std::env::remove_var("IYW_CLAW_ENVIRONMENT_PREVIEW_VERSION");
    assert_eq!(root(), crate::paths::iyw_claw_user_dir());
    assert_eq!(version().unwrap(), env!("CARGO_PKG_VERSION"));
    std::fs::remove_dir(&base).unwrap();
}
